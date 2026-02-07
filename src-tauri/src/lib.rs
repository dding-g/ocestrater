mod config;
mod pty_manager;
mod agent;
mod workspace;
mod git_ops;
mod snippets;
mod trust;
mod keychain;
mod shortcuts;
mod commands;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let app_handle = app.handle().clone();

            // Initialize config on startup
            let config = config::ConfigStore::load_or_default();
            app.manage(std::sync::Mutex::new(config));

            // Initialize PTY manager with max_sessions from config
            let max_sessions = {
                let cfg = app.state::<std::sync::Mutex<config::ConfigStore>>();
                let store = cfg.lock().unwrap();
                store.global.defaults.max_concurrent_agents
            };
            let pty_mgr = pty_manager::PtyManager::new(app_handle, max_sessions);
            app.manage(std::sync::Mutex::new(pty_mgr));

            // Initialize workspace manager
            let ws_mgr = workspace::WorkspaceManager::new();
            app.manage(std::sync::Mutex::new(ws_mgr));

            // Initialize Keychain store (loads all secrets into memory cache)
            let keychain_store = keychain::KeychainStore::load();
            app.manage(std::sync::Mutex::new(keychain_store));

            // Initialize Shortcut store
            let shortcut_store = shortcuts::ShortcutStore::load_or_default();
            app.manage(std::sync::Mutex::new(shortcut_store));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::add_repository,
            commands::remove_repository,
            commands::list_repositories,
            commands::create_workspace,
            commands::stop_workspace,
            commands::remove_workspace,
            commands::list_workspaces,
            commands::send_to_agent,
            commands::get_agents,
            commands::run_snippet,
            commands::get_worktree_status,
            commands::get_diff,
            commands::get_file_content,
            commands::merge_workspace,
            commands::discard_workspace,
            commands::list_snippets,
            commands::save_snippet,
            commands::delete_snippet,
            commands::run_snippet_v2,
            commands::check_trust,
            commands::grant_trust,
            commands::revoke_trust,
            commands::get_secret,
            commands::set_secret,
            commands::delete_secret,
            commands::list_secret_keys,
            commands::list_shortcuts,
            commands::save_shortcuts,
            commands::switch_agent_model,
        ])
        .run(tauri::generate_context!())
        .expect("error running ocestrater");
}
