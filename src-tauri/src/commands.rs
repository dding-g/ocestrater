use crate::agent::AgentAdapter;
use crate::config::{ConfigStore, RepoRef};
use crate::git_ops::{self, FileVersion, MergeStrategy};
use crate::keychain::KeychainState;
use crate::pty_manager::PtyManager;
use crate::shortcuts::{ShortcutConfig, ShortcutState};
use crate::snippets::{self, Snippet};
use crate::trust::{self, TrustStatus};
use crate::workspace::{WorkspaceInfo, WorkspaceManager, WorkspaceState};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

/// Payload emitted when a repo requires trust verification before running setup
#[derive(Debug, Clone, Serialize)]
struct TrustRequiredPayload {
    workspace_id: String,
    repo_path: String,
    script_content: String,
    changed_files: Vec<String>,
}

// ── Shared Helpers ──

/// Data extracted from a workspace, used by multiple commands.
struct WorkspaceContext {
    agent_name: String,
    repo_path: String,
    worktree_path: String,
}

/// Extract workspace context (agent, repo_path, worktree_path) from the workspace manager.
fn get_workspace_context(
    ws_mgr: &Mutex<WorkspaceManager>,
    workspace_id: &str,
) -> Result<WorkspaceContext, String> {
    let ws = ws_mgr.lock().map_err(|e| e.to_string())?;
    let workspace = ws
        .get(workspace_id)
        .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;
    Ok(WorkspaceContext {
        agent_name: workspace.agent.clone(),
        repo_path: workspace.repo_path.clone(),
        worktree_path: workspace.worktree_path.clone(),
    })
}

/// Resolve agent config, fetch secrets, and spawn a PTY for the workspace.
fn resolve_and_spawn_agent(
    config: &Mutex<ConfigStore>,
    pty_mgr: &Mutex<PtyManager>,
    keychain: &KeychainState,
    workspace_id: &str,
    agent_name: String,
    repo_path: &str,
    worktree_path: &str,
    model_override: Option<&str>,
) -> Result<(), String> {
    let store = config.lock().map_err(|e| e.to_string())?;
    let agent_config = store
        .resolve_agent(repo_path, &agent_name)
        .ok_or_else(|| format!("unknown agent: {agent_name}"))?;
    let default_model = agent_config.default_model.clone();
    drop(store);

    let secret_env = {
        let kc = keychain.lock().map_err(|e| e.to_string())?;
        kc.env_vars().clone()
    };

    let model = model_override.or(default_model.as_deref());
    let adapter = AgentAdapter::new(agent_name, agent_config);
    let mut pty = pty_mgr.lock().map_err(|e| e.to_string())?;
    pty.spawn(workspace_id, &adapter, worktree_path, model, Some(&secret_env))
}

/// Canonicalize a repo path and verify it is a registered repository.
fn validate_repo_path(
    config: &Mutex<ConfigStore>,
    repo_path: &str,
) -> Result<String, String> {
    let canonical = std::fs::canonicalize(repo_path)
        .map_err(|e| format!("invalid repo path: {e}"))?;
    let canonical_str = canonical.to_string_lossy().to_string();

    let store = config.lock().map_err(|e| e.to_string())?;
    let registered = store.global.repositories.iter().any(|r| {
        // Compare canonicalized paths to handle inconsistent representations
        std::fs::canonicalize(&r.path)
            .map(|c| c == canonical)
            .unwrap_or(r.path == canonical_str)
    });
    if !registered {
        return Err(format!("repository not registered: {canonical_str}"));
    }
    Ok(canonical_str)
}

// ── Config Commands ──

#[tauri::command]
pub fn get_config(config: State<'_, Mutex<ConfigStore>>) -> Result<serde_json::Value, String> {
    let store = config.lock().map_err(|e| e.to_string())?;
    serde_json::to_value(&store.global).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_config(
    config: State<'_, Mutex<ConfigStore>>,
    global: serde_json::Value,
) -> Result<(), String> {
    let mut store = config.lock().map_err(|e| e.to_string())?;
    store.global = serde_json::from_value(global).map_err(|e| e.to_string())?;
    store.save_global()
}

// ── Repository Commands ──

#[tauri::command]
pub fn add_repository(
    config: State<'_, Mutex<ConfigStore>>,
    path: String,
    alias: String,
) -> Result<(), String> {
    // Canonicalize path to prevent symlink attacks and inconsistent representations
    let canonical = std::fs::canonicalize(&path)
        .map_err(|e| format!("invalid path: {e}"))?;
    let canonical_str = canonical.to_string_lossy().to_string();

    // Validate that the path is a git repo
    if !canonical.join(".git").exists() {
        return Err(format!("not a git repository: {canonical_str}"));
    }

    let mut store = config.lock().map_err(|e| e.to_string())?;
    store.add_repository(canonical_str, alias);
    store.save_global()
}

#[tauri::command]
pub fn remove_repository(
    config: State<'_, Mutex<ConfigStore>>,
    path: String,
) -> Result<(), String> {
    let mut store = config.lock().map_err(|e| e.to_string())?;
    store.remove_repository(&path);
    store.save_global()
}

#[tauri::command]
pub fn list_repositories(
    config: State<'_, Mutex<ConfigStore>>,
) -> Result<Vec<RepoRef>, String> {
    let store = config.lock().map_err(|e| e.to_string())?;
    Ok(store.global.repositories.clone())
}

// ── Workspace Commands ──

#[derive(Deserialize)]
pub struct CreateWorkspaceArgs {
    pub repo_path: String,
    pub repo_alias: String,
    pub branch: String,
    pub agent: Option<String>,
}

#[tauri::command]
pub fn create_workspace(
    app: AppHandle,
    config: State<'_, Mutex<ConfigStore>>,
    ws_mgr: State<'_, Mutex<WorkspaceManager>>,
    pty_mgr: State<'_, Mutex<PtyManager>>,
    keychain: State<'_, KeychainState>,
    args: CreateWorkspaceArgs,
) -> Result<WorkspaceInfo, String> {
    // Validate and canonicalize the repo path
    let repo_path = validate_repo_path(&config, &args.repo_path)?;

    let store = config.lock().map_err(|e| e.to_string())?;

    // Resolve agent
    let agent_name = args.agent
        .or_else(|| {
            store.repo_configs
                .get(&repo_path)
                .and_then(|rc| rc.default_agent.clone())
        })
        .unwrap_or_else(|| store.global.defaults.agent.clone());

    let worktree_dir = store
        .repo_configs
        .get(&repo_path)
        .map(|rc| rc.worktree_dir.clone())
        .unwrap_or_else(|| ".worktrees".into());

    let setup_script = store
        .repo_configs
        .get(&repo_path)
        .and_then(|rc| rc.setup_script.clone());

    drop(store); // Release config lock

    // Create workspace (git worktree) — workspace.rs now canonicalizes internally
    let mut ws_manager = ws_mgr.lock().map_err(|e| e.to_string())?;
    let ws = ws_manager.create(
        &repo_path,
        &args.repo_alias,
        &args.branch,
        &agent_name,
        &worktree_dir,
    )?;
    drop(ws_manager);

    // Trust check for setup scripts
    if let Some(ref script) = setup_script {
        let trust_status = trust::check_trust(&repo_path).unwrap_or(TrustStatus::Untrusted);
        match trust_status {
            TrustStatus::Trusted => {
                run_setup_script(script, &ws.worktree_path);
            }
            TrustStatus::Untrusted => {
                let _ = app.emit("trust-required", TrustRequiredPayload {
                    workspace_id: ws.id.clone(),
                    repo_path: repo_path.clone(),
                    script_content: script.clone(),
                    changed_files: vec![],
                });
                return Ok(ws);
            }
            TrustStatus::Changed { changed_files } => {
                let _ = app.emit("trust-required", TrustRequiredPayload {
                    workspace_id: ws.id.clone(),
                    repo_path: repo_path.clone(),
                    script_content: script.clone(),
                    changed_files,
                });
                return Ok(ws);
            }
        }
    }

    // Spawn agent PTY
    resolve_and_spawn_agent(
        &config, &pty_mgr, &keychain,
        &ws.id, agent_name, &repo_path, &ws.worktree_path, None,
    )?;

    Ok(ws)
}

/// Helper: run a setup script in a worktree directory
fn run_setup_script(script: &str, worktree_path: &str) {
    let output = std::process::Command::new("sh")
        .args(["-c", script])
        .current_dir(worktree_path)
        .output();

    if let Ok(out) = output {
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            eprintln!("setup script warning: {stderr}");
        }
    }
}

/// Run setup script then start agent (called after user approves trust)
#[tauri::command]
pub fn run_setup_and_start_agent(
    config: State<'_, Mutex<ConfigStore>>,
    ws_mgr: State<'_, Mutex<WorkspaceManager>>,
    pty_mgr: State<'_, Mutex<PtyManager>>,
    keychain: State<'_, KeychainState>,
    workspace_id: String,
) -> Result<(), String> {
    let ctx = get_workspace_context(&ws_mgr, &workspace_id)?;

    // Get setup script
    let setup_script = {
        let store = config.lock().map_err(|e| e.to_string())?;
        store
            .repo_configs
            .get(&ctx.repo_path)
            .and_then(|rc| rc.setup_script.clone())
    };

    if let Some(script) = setup_script {
        run_setup_script(&script, &ctx.worktree_path);
    }

    resolve_and_spawn_agent(
        &config, &pty_mgr, &keychain,
        &workspace_id, ctx.agent_name, &ctx.repo_path, &ctx.worktree_path, None,
    )
}

/// Start agent without running setup script (called when user denies trust)
#[tauri::command]
pub fn start_agent_no_setup(
    config: State<'_, Mutex<ConfigStore>>,
    ws_mgr: State<'_, Mutex<WorkspaceManager>>,
    pty_mgr: State<'_, Mutex<PtyManager>>,
    keychain: State<'_, KeychainState>,
    workspace_id: String,
) -> Result<(), String> {
    let ctx = get_workspace_context(&ws_mgr, &workspace_id)?;

    resolve_and_spawn_agent(
        &config, &pty_mgr, &keychain,
        &workspace_id, ctx.agent_name, &ctx.repo_path, &ctx.worktree_path, None,
    )
}

#[tauri::command]
pub fn stop_workspace(
    ws_mgr: State<'_, Mutex<WorkspaceManager>>,
    pty_mgr: State<'_, Mutex<PtyManager>>,
    workspace_id: String,
) -> Result<(), String> {
    // Kill PTY first
    let mut pty = pty_mgr.lock().map_err(|e| e.to_string())?;
    pty.kill(&workspace_id)?;
    drop(pty);

    // Update workspace state
    let mut ws = ws_mgr.lock().map_err(|e| e.to_string())?;
    ws.stop(&workspace_id)
}

#[tauri::command]
pub fn remove_workspace(
    ws_mgr: State<'_, Mutex<WorkspaceManager>>,
    pty_mgr: State<'_, Mutex<PtyManager>>,
    workspace_id: String,
) -> Result<(), String> {
    // Ensure PTY is killed
    let mut pty = pty_mgr.lock().map_err(|e| e.to_string())?;
    let _ = pty.kill(&workspace_id);
    drop(pty);

    let mut ws = ws_mgr.lock().map_err(|e| e.to_string())?;
    ws.remove(&workspace_id)
}

#[tauri::command]
pub fn list_workspaces(
    ws_mgr: State<'_, Mutex<WorkspaceManager>>,
    repo_path: Option<String>,
) -> Result<Vec<WorkspaceInfo>, String> {
    let ws = ws_mgr.lock().map_err(|e| e.to_string())?;
    Ok(ws.list(repo_path.as_deref()))
}

// ── Agent Commands ──

#[tauri::command]
pub fn send_to_agent(
    pty_mgr: State<'_, Mutex<PtyManager>>,
    workspace_id: String,
    message: String,
) -> Result<(), String> {
    let mut pty = pty_mgr.lock().map_err(|e| e.to_string())?;
    pty.write(&workspace_id, &message)
}

#[tauri::command]
pub fn get_agents(
    config: State<'_, Mutex<ConfigStore>>,
) -> Result<Vec<String>, String> {
    let store = config.lock().map_err(|e| e.to_string())?;
    Ok(store.global.agents.keys().cloned().collect())
}

// ── Snippet Commands ──

#[tauri::command]
pub fn run_snippet(
    config: State<'_, Mutex<ConfigStore>>,
    ws_mgr: State<'_, Mutex<WorkspaceManager>>,
    workspace_id: String,
    snippet_name: String,
) -> Result<String, String> {
    let ctx = get_workspace_context(&ws_mgr, &workspace_id)?;

    let script = {
        let store = config.lock().map_err(|e| e.to_string())?;
        store
            .repo_configs
            .get(&ctx.repo_path)
            .and_then(|rc| rc.snippets.get(&snippet_name))
            .cloned()
            .ok_or_else(|| format!("snippet not found: {snippet_name}"))?
    };

    // Trust check for repo-level snippets
    match trust::check_trust(&ctx.repo_path)? {
        TrustStatus::Trusted => {}
        TrustStatus::Untrusted => {
            return Err("repo not trusted — grant trust before running snippets".to_string());
        }
        TrustStatus::Changed { .. } => {
            return Err("repo trust stale — re-approve trust before running snippets".to_string());
        }
    }

    let output = std::process::Command::new("sh")
        .args(["-c", &script])
        .current_dir(&ctx.worktree_path)
        .output()
        .map_err(|e| format!("snippet exec error: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(stdout)
    } else {
        Err(format!("snippet failed:\n{stderr}\n{stdout}"))
    }
}

// ── Git Review Commands ──

#[tauri::command]
pub fn get_worktree_status(
    ws_mgr: State<'_, Mutex<WorkspaceManager>>,
    workspace_id: String,
) -> Result<git_ops::WorktreeStatus, String> {
    let ctx = get_workspace_context(&ws_mgr, &workspace_id)?;
    let base_branch = git_ops::detect_base_branch(&ctx.repo_path);
    git_ops::compute_status(&ctx.worktree_path, &workspace_id, &base_branch)
}

#[tauri::command]
pub fn get_diff(
    ws_mgr: State<'_, Mutex<WorkspaceManager>>,
    workspace_id: String,
    paths: Option<Vec<String>>,
) -> Result<Vec<git_ops::FileDiff>, String> {
    let ctx = get_workspace_context(&ws_mgr, &workspace_id)?;
    let base_branch = git_ops::detect_base_branch(&ctx.repo_path);
    git_ops::compute_diff(&ctx.worktree_path, &base_branch, paths.as_deref())
}

#[tauri::command]
pub fn get_file_content(
    ws_mgr: State<'_, Mutex<WorkspaceManager>>,
    workspace_id: String,
    path: String,
    version: FileVersion,
) -> Result<String, String> {
    let ctx = get_workspace_context(&ws_mgr, &workspace_id)?;
    let base_branch = git_ops::detect_base_branch(&ctx.repo_path);
    git_ops::read_file_at_version(&ctx.worktree_path, &path, &version, &base_branch)
}

#[tauri::command]
pub fn merge_workspace(
    ws_mgr: State<'_, Mutex<WorkspaceManager>>,
    workspace_id: String,
    strategy: MergeStrategy,
    commit_message: Option<String>,
) -> Result<git_ops::MergeResult, String> {
    // merge_workspace needs workspace state check, so use direct lock
    let ws = ws_mgr.lock().map_err(|e| e.to_string())?;
    let workspace = ws
        .get(&workspace_id)
        .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;

    if workspace.state == WorkspaceState::Running {
        return Err("cannot merge a running workspace — stop it first".into());
    }

    let repo_path = workspace.repo_path.clone();
    let worktree_path = workspace.worktree_path.clone();
    drop(ws);

    let worktree_branch = detect_worktree_branch(&worktree_path)?;
    let base_branch = git_ops::detect_base_branch(&repo_path);

    git_ops::merge_branch(
        &repo_path,
        &worktree_branch,
        &base_branch,
        &strategy,
        commit_message.as_deref(),
    )
}

fn detect_worktree_branch(worktree_path: &str) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(worktree_path)
        .output()
        .map_err(|e| format!("git exec error: {e}"))?;

    if !output.status.success() {
        return Err("failed to detect worktree branch".to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[tauri::command]
pub fn discard_workspace(
    ws_mgr: State<'_, Mutex<WorkspaceManager>>,
    pty_mgr: State<'_, Mutex<PtyManager>>,
    workspace_id: String,
) -> Result<(), String> {
    // Kill PTY if running
    {
        let mut pty = pty_mgr.lock().map_err(|e| e.to_string())?;
        let _ = pty.kill(&workspace_id);
    }

    // Stop workspace and extract context
    {
        let mut ws = ws_mgr.lock().map_err(|e| e.to_string())?;
        let _ = ws.stop(&workspace_id);
    }

    let ctx = get_workspace_context(&ws_mgr, &workspace_id)?;
    let branch_name = detect_worktree_branch(&ctx.worktree_path)?;

    git_ops::discard_worktree(&ctx.repo_path, &ctx.worktree_path, &branch_name)?;

    let mut ws = ws_mgr.lock().map_err(|e| e.to_string())?;
    ws.remove(&workspace_id)
}

// ── Snippet V2 Commands ──

#[tauri::command]
pub fn list_snippets(repo_path: Option<String>) -> Result<Vec<Snippet>, String> {
    Ok(snippets::list_merged_snippets(repo_path.as_deref()))
}

#[tauri::command]
pub fn save_snippet(repo_path: Option<String>, snippet: Snippet) -> Result<(), String> {
    snippets::save_snippet(repo_path.as_deref(), snippet)
}

#[tauri::command]
pub fn delete_snippet(repo_path: Option<String>, name: String) -> Result<(), String> {
    snippets::delete_snippet(repo_path.as_deref(), &name)
}

#[tauri::command]
pub async fn run_snippet_v2(
    app: AppHandle,
    ws_mgr: State<'_, Mutex<WorkspaceManager>>,
    workspace_id: String,
    name: String,
) -> Result<(), String> {
    let ctx = get_workspace_context(&ws_mgr, &workspace_id)?;
    let worktree_path = ctx.worktree_path;
    let repo_path = ctx.repo_path;

    // Resolve snippet
    let snippet = snippets::resolve_snippet(&repo_path, &name)
        .ok_or_else(|| format!("snippet not found: {name}"))?;

    // Trust check for repo-level snippets (with TOCTOU protection)
    let is_repo = snippets::is_repo_snippet(&repo_path, &name);
    if is_repo {
        match trust::check_trust(&repo_path)? {
            TrustStatus::Trusted => {}
            TrustStatus::Untrusted => {
                return Err("repo not trusted".to_string());
            }
            TrustStatus::Changed { .. } => {
                return Err("repo trust stale".to_string());
            }
        }
    }

    // Capture the command and re-resolve right before execution to prevent TOCTOU
    let command = snippet.command.clone();
    let ws_id = workspace_id.clone();
    let repo_path_clone = repo_path.clone();

    // Emit separator header
    let _ = app.emit(
        &format!("snippet-output-{ws_id}"),
        format!("── Running snippet: {} ──────────────────────\n{}\n", name, command),
    );

    // Spawn child process in background thread
    let app_clone = app.clone();
    std::thread::spawn(move || {
        // TOCTOU guard: re-verify the snippet command hasn't changed since trust check
        if is_repo {
            if let Some(current) = snippets::resolve_snippet(&repo_path_clone, &name) {
                if current.command != command {
                    let _ = app_clone.emit(
                        &format!("snippet-output-{ws_id}"),
                        "Error: snippet changed after trust check. Aborting.\n".to_string(),
                    );
                    let _ = app_clone.emit(&format!("snippet-complete-{ws_id}"), -1);
                    return;
                }
            }
        }

        let child = std::process::Command::new("sh")
            .args(["-c", &command])
            .current_dir(&worktree_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn();

        match child {
            Ok(mut child) => {
                // Read stdout and stderr in parallel, batch output like PTY manager
                let stdout = child.stdout.take();
                let stderr = child.stderr.take();

                let app_stdout = app_clone.clone();
                let ws_id_stdout = ws_id.clone();
                let stdout_handle = std::thread::spawn(move || {
                    if let Some(stdout) = stdout {
                        stream_output(stdout, &app_stdout, &ws_id_stdout);
                    }
                });

                let app_stderr = app_clone.clone();
                let ws_id_stderr = ws_id.clone();
                let stderr_handle = std::thread::spawn(move || {
                    if let Some(stderr) = stderr {
                        stream_output(stderr, &app_stderr, &ws_id_stderr);
                    }
                });

                let _ = stdout_handle.join();
                let _ = stderr_handle.join();

                let exit_code = child
                    .wait()
                    .map(|status| status.code().unwrap_or(-1))
                    .unwrap_or(-1);

                let _ = app_clone.emit(
                    &format!("snippet-output-{ws_id}"),
                    format!("── Snippet complete (exit: {exit_code}) ─────────────────\n"),
                );
                let _ = app_clone.emit(&format!("snippet-complete-{ws_id}"), exit_code);
            }
            Err(e) => {
                let _ = app_clone.emit(
                    &format!("snippet-output-{ws_id}"),
                    format!("Failed to start snippet: {e}\n"),
                );
                let _ = app_clone.emit(&format!("snippet-complete-{ws_id}"), -1);
            }
        }
    });

    Ok(())
}

/// Stream output from a reader to IPC events with batching (16ms / 4KB threshold)
fn stream_output(reader: impl std::io::Read, app: &AppHandle, workspace_id: &str) {
    use std::io::BufRead;

    let buf_reader = std::io::BufReader::new(reader);
    let mut batch = String::new();
    let mut last_flush = std::time::Instant::now();
    let event_name = format!("snippet-output-{workspace_id}");

    for line in buf_reader.lines() {
        match line {
            Ok(text) => {
                batch.push_str(&text);
                batch.push('\n');

                let elapsed = last_flush.elapsed().as_millis() as u64;
                if elapsed >= 16 || batch.len() > 4096 {
                    let _ = app.emit(&event_name, batch.clone());
                    batch.clear();
                    last_flush = std::time::Instant::now();
                }
            }
            Err(_) => break,
        }
    }

    if !batch.is_empty() {
        let _ = app.emit(&event_name, batch);
    }
}

// ── Trust Commands ──

#[tauri::command]
pub fn check_trust(repo_path: String) -> Result<TrustStatus, String> {
    trust::check_trust(&repo_path)
}

#[tauri::command]
pub fn grant_trust(repo_path: String) -> Result<(), String> {
    trust::grant_trust(&repo_path)
}

#[tauri::command]
pub fn revoke_trust(repo_path: String) -> Result<(), String> {
    trust::revoke_trust(&repo_path)
}

// ── Keychain Commands ──

#[tauri::command]
pub fn get_secret(
    keychain: State<'_, KeychainState>,
    key: String,
) -> Result<String, String> {
    let kc = keychain.lock().map_err(|e| e.to_string())?;
    kc.get(&key)
        .cloned()
        .ok_or_else(|| format!("secret not found: {key}"))
}

#[tauri::command]
pub fn set_secret(
    keychain: State<'_, KeychainState>,
    key: String,
    value: String,
) -> Result<(), String> {
    let mut kc = keychain.lock().map_err(|e| e.to_string())?;
    kc.set(&key, &value)
}

#[tauri::command]
pub fn delete_secret(
    keychain: State<'_, KeychainState>,
    key: String,
) -> Result<(), String> {
    let mut kc = keychain.lock().map_err(|e| e.to_string())?;
    kc.delete(&key)
}

#[tauri::command]
pub fn list_secret_keys(
    keychain: State<'_, KeychainState>,
) -> Result<Vec<String>, String> {
    let kc = keychain.lock().map_err(|e| e.to_string())?;
    Ok(kc.list_keys())
}

// ── Shortcut Commands ──

#[tauri::command]
pub fn list_shortcuts(
    shortcuts: State<'_, ShortcutState>,
) -> Result<ShortcutConfig, String> {
    let store = shortcuts.lock().map_err(|e| e.to_string())?;
    Ok(store.list())
}

#[tauri::command]
pub fn save_shortcuts(
    app: AppHandle,
    shortcuts: State<'_, ShortcutState>,
    config: ShortcutConfig,
) -> Result<(), String> {
    let mut store = shortcuts.lock().map_err(|e| e.to_string())?;
    store.save(config.clone())?;
    let _ = app.emit("shortcuts-updated", config);
    Ok(())
}

// ── Model Switch Command ──

#[tauri::command]
pub fn switch_agent_model(
    config: State<'_, Mutex<ConfigStore>>,
    ws_mgr: State<'_, Mutex<WorkspaceManager>>,
    pty_mgr: State<'_, Mutex<PtyManager>>,
    keychain: State<'_, KeychainState>,
    workspace_id: String,
    model: String,
) -> Result<(), String> {
    let ctx = get_workspace_context(&ws_mgr, &workspace_id)?;

    // Kill existing PTY before respawning
    {
        let mut pty = pty_mgr.lock().map_err(|e| e.to_string())?;
        pty.kill(&workspace_id)?;
    }

    resolve_and_spawn_agent(
        &config, &pty_mgr, &keychain,
        &workspace_id, ctx.agent_name, &ctx.repo_path, &ctx.worktree_path, Some(&model),
    )
}
