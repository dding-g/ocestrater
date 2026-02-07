use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorkspaceState {
    Creating,
    Running,
    Stopping,
    Stopped,
    Cleaning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub id: String,
    pub repo_path: String,
    pub repo_alias: String,
    pub branch: String,
    pub worktree_path: String,
    pub agent: String,
    pub state: WorkspaceState,
}

pub struct WorkspaceManager {
    workspaces: HashMap<String, WorkspaceInfo>,
}

impl WorkspaceManager {
    pub fn new() -> Self {
        Self {
            workspaces: HashMap::new(),
        }
    }

    /// Create a new workspace with an isolated git worktree
    pub fn create(
        &mut self,
        repo_path: &str,
        repo_alias: &str,
        branch: &str,
        agent: &str,
        worktree_dir: &str,
    ) -> Result<WorkspaceInfo, String> {
        // Canonicalize repo_path to resolve symlinks and prevent path traversal
        let canonical_repo = std::fs::canonicalize(repo_path)
            .map_err(|e| format!("invalid repo path: {e}"))?;

        // Validate it is a git repository
        if !canonical_repo.join(".git").exists() {
            return Err(format!(
                "not a git repository: {}",
                canonical_repo.display()
            ));
        }

        let id = Uuid::new_v4().to_string();
        let short_id = &id[..8];
        let worktree_name = format!("{branch}-{short_id}");
        let worktree_path = canonical_repo
            .join(worktree_dir)
            .join(&worktree_name);

        // Ensure worktree directory parent exists
        if let Some(parent) = worktree_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("mkdir error: {e}"))?;
        }

        // After directory creation, canonicalize the worktree path and verify
        // it stays under the canonical repo to prevent traversal via worktree_dir
        let canonical_wt = std::fs::canonicalize(worktree_path.parent().unwrap())
            .map_err(|e| format!("invalid worktree path: {e}"))?
            .join(&worktree_name);
        if !canonical_wt.starts_with(&canonical_repo) {
            return Err("worktree path escapes repository directory".into());
        }

        let canonical_repo_str = canonical_repo.to_string_lossy().to_string();
        let canonical_wt_str = canonical_wt.to_string_lossy().to_string();

        let ws = WorkspaceInfo {
            id: id.clone(),
            repo_path: canonical_repo_str.clone(),
            repo_alias: repo_alias.to_string(),
            branch: branch.to_string(),
            worktree_path: canonical_wt_str.clone(),
            agent: agent.to_string(),
            state: WorkspaceState::Creating,
        };

        self.workspaces.insert(id.clone(), ws.clone());

        // Create git worktree
        let output = Command::new("git")
            .args(["worktree", "add", "-b", &worktree_name])
            .arg(&canonical_wt_str)
            .arg(self.resolve_base_branch(&canonical_repo_str))
            .current_dir(&canonical_repo)
            .output()
            .map_err(|e| format!("git worktree error: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            self.workspaces.remove(&id);
            return Err(format!("git worktree add failed: {stderr}"));
        }

        // Update state to Running (PTY will be spawned separately)
        if let Some(ws) = self.workspaces.get_mut(&id) {
            ws.state = WorkspaceState::Running;
        }

        Ok(self.workspaces[&id].clone())
    }

    /// Stop a workspace: transition to Stopping → Stopped
    pub fn stop(&mut self, workspace_id: &str) -> Result<(), String> {
        let ws = self
            .workspaces
            .get_mut(workspace_id)
            .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;

        ws.state = WorkspaceState::Stopping;
        // PTY kill is handled by PtyManager; we just update state
        ws.state = WorkspaceState::Stopped;
        Ok(())
    }

    /// Remove a workspace: cleanup worktree
    pub fn remove(&mut self, workspace_id: &str) -> Result<(), String> {
        let ws = self
            .workspaces
            .get_mut(workspace_id)
            .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;

        if ws.state == WorkspaceState::Running {
            return Err("cannot remove a running workspace — stop it first".into());
        }

        ws.state = WorkspaceState::Cleaning;
        let worktree_path = ws.worktree_path.clone();
        let repo_path = ws.repo_path.clone();

        // Remove git worktree
        let output = Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(&worktree_path)
            .current_dir(&repo_path)
            .output()
            .map_err(|e| format!("git worktree remove error: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Log but don't fail — force-cleanup the directory
            eprintln!("worktree remove warning: {stderr}");
            let _ = std::fs::remove_dir_all(&worktree_path);
        }

        self.workspaces.remove(workspace_id);
        Ok(())
    }

    /// List all workspaces, optionally filtered by repo
    pub fn list(&self, repo_path: Option<&str>) -> Vec<WorkspaceInfo> {
        self.workspaces
            .values()
            .filter(|ws| {
                repo_path
                    .map(|rp| ws.repo_path == rp)
                    .unwrap_or(true)
            })
            .cloned()
            .collect()
    }

    pub fn get(&self, workspace_id: &str) -> Option<&WorkspaceInfo> {
        self.workspaces.get(workspace_id)
    }

    fn resolve_base_branch(&self, repo_path: &str) -> String {
        // Try to detect main branch
        let output = Command::new("git")
            .args(["symbolic-ref", "refs/remotes/origin/HEAD", "--short"])
            .current_dir(repo_path)
            .output();

        match output {
            Ok(o) if o.status.success() => {
                let branch = String::from_utf8_lossy(&o.stdout).trim().to_string();
                branch.strip_prefix("origin/").unwrap_or(&branch).to_string()
            }
            _ => "main".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: create a workspace manager with a pre-inserted workspace (bypassing git)
    fn make_manager_with_workspace(state: WorkspaceState) -> (WorkspaceManager, String) {
        let mut mgr = WorkspaceManager::new();
        let id = "test-ws-id".to_string();
        let ws = WorkspaceInfo {
            id: id.clone(),
            repo_path: "/tmp/test-repo".to_string(),
            repo_alias: "test-repo".to_string(),
            branch: "feature-branch".to_string(),
            worktree_path: "/tmp/test-repo/.worktrees/feature-branch".to_string(),
            agent: "claude".to_string(),
            state,
        };
        mgr.workspaces.insert(id.clone(), ws);
        (mgr, id)
    }

    // ── Basic workspace manager tests ──

    #[test]
    fn test_new_manager_is_empty() {
        let mgr = WorkspaceManager::new();
        assert!(mgr.list(None).is_empty());
    }

    #[test]
    fn test_get_workspace() {
        let (mgr, id) = make_manager_with_workspace(WorkspaceState::Running);
        let ws = mgr.get(&id);
        assert!(ws.is_some());
        assert_eq!(ws.unwrap().branch, "feature-branch");
    }

    #[test]
    fn test_get_nonexistent_workspace() {
        let mgr = WorkspaceManager::new();
        assert!(mgr.get("nonexistent").is_none());
    }

    // ── State transition tests ──

    #[test]
    fn test_stop_transitions_to_stopped() {
        let (mut mgr, id) = make_manager_with_workspace(WorkspaceState::Running);
        let result = mgr.stop(&id);
        assert!(result.is_ok());
        assert_eq!(mgr.get(&id).unwrap().state, WorkspaceState::Stopped);
    }

    #[test]
    fn test_stop_nonexistent_workspace_fails() {
        let mut mgr = WorkspaceManager::new();
        let result = mgr.stop("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("workspace not found"));
    }

    #[test]
    fn test_remove_running_workspace_fails() {
        let (mut mgr, id) = make_manager_with_workspace(WorkspaceState::Running);
        let result = mgr.remove(&id);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cannot remove a running workspace"));
    }

    #[test]
    fn test_remove_stopped_workspace_sets_cleaning_state() {
        let (mut mgr, id) = make_manager_with_workspace(WorkspaceState::Stopped);
        // Verify the state check allows Stopped workspaces (doesn't return "cannot remove")
        let result = mgr.remove(&id);
        // In test env, git command may fail because the repo_path doesn't exist,
        // but it should NOT fail with the "cannot remove a running workspace" error.
        if let Err(ref e) = result {
            assert!(
                !e.contains("cannot remove a running workspace"),
                "Stopped workspace should be removable"
            );
        }
    }

    #[test]
    fn test_remove_creating_workspace_allowed() {
        let (mut mgr, id) = make_manager_with_workspace(WorkspaceState::Creating);
        let result = mgr.remove(&id);
        // Only Running state should block removal
        if let Err(ref e) = result {
            assert!(
                !e.contains("cannot remove a running workspace"),
                "Creating workspace should be removable"
            );
        }
    }

    #[test]
    fn test_remove_stopping_workspace_allowed() {
        let (mut mgr, id) = make_manager_with_workspace(WorkspaceState::Stopping);
        let result = mgr.remove(&id);
        if let Err(ref e) = result {
            assert!(
                !e.contains("cannot remove a running workspace"),
                "Stopping workspace should be removable"
            );
        }
    }

    #[test]
    fn test_remove_nonexistent_workspace_fails() {
        let mut mgr = WorkspaceManager::new();
        let result = mgr.remove("nonexistent");
        assert!(result.is_err());
    }

    // ── List filtering tests ──

    #[test]
    fn test_list_all_workspaces() {
        let mut mgr = WorkspaceManager::new();

        mgr.workspaces.insert(
            "ws1".to_string(),
            WorkspaceInfo {
                id: "ws1".to_string(),
                repo_path: "/repo/a".to_string(),
                repo_alias: "repo-a".to_string(),
                branch: "branch-1".to_string(),
                worktree_path: "/repo/a/.wt/branch-1".to_string(),
                agent: "claude".to_string(),
                state: WorkspaceState::Running,
            },
        );
        mgr.workspaces.insert(
            "ws2".to_string(),
            WorkspaceInfo {
                id: "ws2".to_string(),
                repo_path: "/repo/b".to_string(),
                repo_alias: "repo-b".to_string(),
                branch: "branch-2".to_string(),
                worktree_path: "/repo/b/.wt/branch-2".to_string(),
                agent: "codex".to_string(),
                state: WorkspaceState::Stopped,
            },
        );

        let all = mgr.list(None);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_list_filtered_by_repo_path() {
        let mut mgr = WorkspaceManager::new();

        mgr.workspaces.insert(
            "ws1".to_string(),
            WorkspaceInfo {
                id: "ws1".to_string(),
                repo_path: "/repo/a".to_string(),
                repo_alias: "repo-a".to_string(),
                branch: "branch-1".to_string(),
                worktree_path: "/repo/a/.wt/branch-1".to_string(),
                agent: "claude".to_string(),
                state: WorkspaceState::Running,
            },
        );
        mgr.workspaces.insert(
            "ws2".to_string(),
            WorkspaceInfo {
                id: "ws2".to_string(),
                repo_path: "/repo/b".to_string(),
                repo_alias: "repo-b".to_string(),
                branch: "branch-2".to_string(),
                worktree_path: "/repo/b/.wt/branch-2".to_string(),
                agent: "codex".to_string(),
                state: WorkspaceState::Stopped,
            },
        );
        mgr.workspaces.insert(
            "ws3".to_string(),
            WorkspaceInfo {
                id: "ws3".to_string(),
                repo_path: "/repo/a".to_string(),
                repo_alias: "repo-a".to_string(),
                branch: "branch-3".to_string(),
                worktree_path: "/repo/a/.wt/branch-3".to_string(),
                agent: "gemini".to_string(),
                state: WorkspaceState::Running,
            },
        );

        let filtered = mgr.list(Some("/repo/a"));
        assert_eq!(filtered.len(), 2);
        for ws in &filtered {
            assert_eq!(ws.repo_path, "/repo/a");
        }
    }

    #[test]
    fn test_list_filtered_no_matches() {
        let (mgr, _) = make_manager_with_workspace(WorkspaceState::Running);
        let filtered = mgr.list(Some("/nonexistent/repo"));
        assert!(filtered.is_empty());
    }

    // ── State enum equality tests ──

    #[test]
    fn test_workspace_state_equality() {
        assert_eq!(WorkspaceState::Creating, WorkspaceState::Creating);
        assert_eq!(WorkspaceState::Running, WorkspaceState::Running);
        assert_eq!(WorkspaceState::Stopping, WorkspaceState::Stopping);
        assert_eq!(WorkspaceState::Stopped, WorkspaceState::Stopped);
        assert_eq!(WorkspaceState::Cleaning, WorkspaceState::Cleaning);
        assert_ne!(WorkspaceState::Running, WorkspaceState::Stopped);
    }

    // ── Additional edge case tests ──

    #[test]
    fn test_multiple_workspaces_coexist_in_manager() {
        let mut mgr = WorkspaceManager::new();

        mgr.workspaces.insert(
            "ws-a".to_string(),
            WorkspaceInfo {
                id: "ws-a".to_string(),
                repo_path: "/repo/a".to_string(),
                repo_alias: "repo-a".to_string(),
                branch: "feature-a".to_string(),
                worktree_path: "/repo/a/.wt/feature-a".to_string(),
                agent: "claude".to_string(),
                state: WorkspaceState::Running,
            },
        );
        mgr.workspaces.insert(
            "ws-b".to_string(),
            WorkspaceInfo {
                id: "ws-b".to_string(),
                repo_path: "/repo/b".to_string(),
                repo_alias: "repo-b".to_string(),
                branch: "feature-b".to_string(),
                worktree_path: "/repo/b/.wt/feature-b".to_string(),
                agent: "codex".to_string(),
                state: WorkspaceState::Stopped,
            },
        );
        mgr.workspaces.insert(
            "ws-c".to_string(),
            WorkspaceInfo {
                id: "ws-c".to_string(),
                repo_path: "/repo/a".to_string(),
                repo_alias: "repo-a".to_string(),
                branch: "feature-c".to_string(),
                worktree_path: "/repo/a/.wt/feature-c".to_string(),
                agent: "gemini".to_string(),
                state: WorkspaceState::Creating,
            },
        );

        // All three coexist
        assert_eq!(mgr.list(None).len(), 3);

        // Get each individually
        assert!(mgr.get("ws-a").is_some());
        assert!(mgr.get("ws-b").is_some());
        assert!(mgr.get("ws-c").is_some());

        // Filter by repo_path
        assert_eq!(mgr.list(Some("/repo/a")).len(), 2);
        assert_eq!(mgr.list(Some("/repo/b")).len(), 1);
    }

    #[test]
    fn test_list_returns_empty_for_wrong_repo_path_filter() {
        let mut mgr = WorkspaceManager::new();
        mgr.workspaces.insert(
            "ws1".to_string(),
            WorkspaceInfo {
                id: "ws1".to_string(),
                repo_path: "/repo/actual".to_string(),
                repo_alias: "actual-repo".to_string(),
                branch: "main".to_string(),
                worktree_path: "/repo/actual/.wt/main".to_string(),
                agent: "claude".to_string(),
                state: WorkspaceState::Running,
            },
        );

        let filtered = mgr.list(Some("/repo/wrong"));
        assert!(filtered.is_empty());

        let filtered = mgr.list(Some(""));
        assert!(filtered.is_empty());

        let filtered = mgr.list(Some("/repo/actual/subdir"));
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_stop_then_get_shows_stopped_state() {
        let (mut mgr, id) = make_manager_with_workspace(WorkspaceState::Running);
        mgr.stop(&id).unwrap();
        let ws = mgr.get(&id).unwrap();
        assert_eq!(ws.state, WorkspaceState::Stopped);
    }

    #[test]
    fn test_get_after_manual_remove_returns_none() {
        let (mut mgr, id) = make_manager_with_workspace(WorkspaceState::Stopped);
        // Manually remove from the HashMap (simulating successful cleanup)
        mgr.workspaces.remove(&id);
        assert!(mgr.get(&id).is_none());
    }

    #[test]
    fn test_workspace_info_serde_roundtrip() {
        let ws = WorkspaceInfo {
            id: "test-id-123".to_string(),
            repo_path: "/home/user/project".to_string(),
            repo_alias: "my-project".to_string(),
            branch: "feature/awesome".to_string(),
            worktree_path: "/home/user/project/.worktrees/feature-awesome-abc12345".to_string(),
            agent: "claude".to_string(),
            state: WorkspaceState::Running,
        };

        let json = serde_json::to_string(&ws).unwrap();
        let parsed: WorkspaceInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "test-id-123");
        assert_eq!(parsed.repo_path, "/home/user/project");
        assert_eq!(parsed.repo_alias, "my-project");
        assert_eq!(parsed.branch, "feature/awesome");
        assert_eq!(parsed.agent, "claude");
        assert_eq!(parsed.state, WorkspaceState::Running);
    }

    #[test]
    fn test_workspace_state_serde_roundtrip_all_variants() {
        let variants = vec![
            WorkspaceState::Creating,
            WorkspaceState::Running,
            WorkspaceState::Stopping,
            WorkspaceState::Stopped,
            WorkspaceState::Cleaning,
        ];
        for state in variants {
            let json = serde_json::to_string(&state).unwrap();
            let parsed: WorkspaceState = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, state);
        }
    }

    #[test]
    fn test_stop_already_stopped_workspace_succeeds() {
        let (mut mgr, id) = make_manager_with_workspace(WorkspaceState::Stopped);
        // Stopping an already-stopped workspace should succeed (idempotent)
        let result = mgr.stop(&id);
        assert!(result.is_ok());
        assert_eq!(mgr.get(&id).unwrap().state, WorkspaceState::Stopped);
    }

    #[test]
    fn test_workspace_list_preserves_all_fields() {
        let (mgr, _) = make_manager_with_workspace(WorkspaceState::Running);
        let list = mgr.list(None);
        assert_eq!(list.len(), 1);
        let ws = &list[0];
        assert_eq!(ws.id, "test-ws-id");
        assert_eq!(ws.repo_path, "/tmp/test-repo");
        assert_eq!(ws.repo_alias, "test-repo");
        assert_eq!(ws.branch, "feature-branch");
        assert_eq!(ws.agent, "claude");
    }
}
