use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutConfig {
    pub version: u32,
    pub shortcuts: HashMap<String, String>,
}

pub struct ShortcutStore {
    config: ShortcutConfig,
}

impl ShortcutStore {
    pub fn load_or_default() -> Self {
        let path = Self::config_path();
        let config = if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    serde_json::from_str(&content).unwrap_or_else(|_| Self::default_config())
                }
                Err(_) => Self::default_config(),
            }
        } else {
            let config = Self::default_config();
            // Write defaults
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(
                &path,
                serde_json::to_string_pretty(&config).unwrap_or_default(),
            );
            config
        };

        ShortcutStore { config }
    }

    pub fn list(&self) -> ShortcutConfig {
        self.config.clone()
    }

    pub fn save(&mut self, config: ShortcutConfig) -> Result<(), String> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(&config)
            .map_err(|e| format!("serialize error: {e}"))?;
        std::fs::write(&path, json).map_err(|e| format!("write error: {e}"))?;
        self.config = config;
        Ok(())
    }

    fn config_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".ocestrater")
            .join("shortcuts.json")
    }

    fn default_config() -> ShortcutConfig {
        let mut shortcuts = HashMap::new();
        shortcuts.insert("workspace.new".into(), "Cmd+N".into());
        shortcuts.insert("workspace.close".into(), "Cmd+W".into());
        shortcuts.insert("tab.1".into(), "Cmd+1".into());
        shortcuts.insert("tab.2".into(), "Cmd+2".into());
        shortcuts.insert("tab.3".into(), "Cmd+3".into());
        shortcuts.insert("tab.4".into(), "Cmd+4".into());
        shortcuts.insert("tab.5".into(), "Cmd+5".into());
        shortcuts.insert("tab.6".into(), "Cmd+6".into());
        shortcuts.insert("tab.7".into(), "Cmd+7".into());
        shortcuts.insert("tab.8".into(), "Cmd+8".into());
        shortcuts.insert("tab.9".into(), "Cmd+9".into());
        shortcuts.insert("tab.next".into(), "Cmd+Tab".into());
        shortcuts.insert("tab.prev".into(), "Cmd+Shift+Tab".into());
        shortcuts.insert("palette.snippets".into(), "Cmd+P".into());
        shortcuts.insert("message.send".into(), "Cmd+Enter".into());
        shortcuts.insert("palette.command".into(), "Cmd+K".into());
        shortcuts.insert("settings.open".into(), "Cmd+,".into());
        shortcuts.insert("agent.restart".into(), "Cmd+Shift+R".into());

        ShortcutConfig {
            version: 1,
            shortcuts,
        }
    }
}

pub type ShortcutState = Mutex<ShortcutStore>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_has_expected_shortcut_count() {
        let config = ShortcutStore::default_config();
        assert_eq!(config.shortcuts.len(), 18);
    }

    #[test]
    fn test_default_config_version_is_1() {
        let config = ShortcutStore::default_config();
        assert_eq!(config.version, 1);
    }

    #[test]
    fn test_shortcut_config_serde_roundtrip() {
        let config = ShortcutStore::default_config();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: ShortcutConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, config.version);
        assert_eq!(parsed.shortcuts.len(), config.shortcuts.len());
        for (key, value) in &config.shortcuts {
            assert_eq!(parsed.shortcuts.get(key).unwrap(), value);
        }
    }

    #[test]
    fn test_default_config_contains_workspace_new() {
        let config = ShortcutStore::default_config();
        assert_eq!(config.shortcuts.get("workspace.new").unwrap(), "Cmd+N");
    }

    #[test]
    fn test_default_config_contains_workspace_close() {
        let config = ShortcutStore::default_config();
        assert_eq!(config.shortcuts.get("workspace.close").unwrap(), "Cmd+W");
    }

    #[test]
    fn test_default_config_contains_palette_snippets() {
        let config = ShortcutStore::default_config();
        assert_eq!(config.shortcuts.get("palette.snippets").unwrap(), "Cmd+P");
    }

    #[test]
    fn test_default_config_contains_message_send() {
        let config = ShortcutStore::default_config();
        assert_eq!(config.shortcuts.get("message.send").unwrap(), "Cmd+Enter");
    }

    #[test]
    fn test_default_config_contains_palette_command() {
        let config = ShortcutStore::default_config();
        assert_eq!(config.shortcuts.get("palette.command").unwrap(), "Cmd+K");
    }

    #[test]
    fn test_default_config_contains_settings_open() {
        let config = ShortcutStore::default_config();
        assert_eq!(config.shortcuts.get("settings.open").unwrap(), "Cmd+,");
    }

    #[test]
    fn test_default_config_contains_agent_restart() {
        let config = ShortcutStore::default_config();
        assert_eq!(
            config.shortcuts.get("agent.restart").unwrap(),
            "Cmd+Shift+R"
        );
    }

    #[test]
    fn test_default_config_contains_all_tab_shortcuts() {
        let config = ShortcutStore::default_config();
        for i in 1..=9 {
            let key = format!("tab.{i}");
            assert!(
                config.shortcuts.contains_key(&key),
                "Missing shortcut: {key}"
            );
            assert_eq!(
                config.shortcuts.get(&key).unwrap(),
                &format!("Cmd+{i}")
            );
        }
    }

    #[test]
    fn test_default_config_contains_tab_navigation() {
        let config = ShortcutStore::default_config();
        assert_eq!(config.shortcuts.get("tab.next").unwrap(), "Cmd+Tab");
        assert_eq!(
            config.shortcuts.get("tab.prev").unwrap(),
            "Cmd+Shift+Tab"
        );
    }

    #[test]
    fn test_shortcut_config_empty_shortcuts() {
        let config = ShortcutConfig {
            version: 1,
            shortcuts: HashMap::new(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: ShortcutConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, 1);
        assert!(parsed.shortcuts.is_empty());
    }

    #[test]
    fn test_shortcut_config_custom_shortcuts() {
        let mut shortcuts = HashMap::new();
        shortcuts.insert("custom.action".into(), "Ctrl+Alt+Z".into());
        let config = ShortcutConfig {
            version: 2,
            shortcuts,
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: ShortcutConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, 2);
        assert_eq!(
            parsed.shortcuts.get("custom.action").unwrap(),
            "Ctrl+Alt+Z"
        );
    }

    #[test]
    fn test_default_config_all_action_names_have_dot_separator() {
        let config = ShortcutStore::default_config();
        for key in config.shortcuts.keys() {
            assert!(
                key.contains('.'),
                "Action name '{key}' should contain a dot separator"
            );
        }
    }

    #[test]
    fn test_default_config_no_duplicate_bindings_by_value() {
        let config = ShortcutStore::default_config();
        let mut seen = std::collections::HashSet::new();
        for value in config.shortcuts.values() {
            assert!(
                seen.insert(value.clone()),
                "Duplicate binding found: {value}"
            );
        }
    }

    // ── Additional tests: tempdir load/save cycle and merge ──

    #[test]
    fn test_shortcut_config_save_load_cycle_with_tempdir() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("shortcuts.json");

        let mut shortcuts = HashMap::new();
        shortcuts.insert("custom.action1".into(), "Ctrl+1".into());
        shortcuts.insert("custom.action2".into(), "Ctrl+2".into());

        let config = ShortcutConfig {
            version: 1,
            shortcuts,
        };

        // Save
        let json = serde_json::to_string_pretty(&config).unwrap();
        std::fs::write(&path, &json).unwrap();

        // Load
        let content = std::fs::read_to_string(&path).unwrap();
        let loaded: ShortcutConfig = serde_json::from_str(&content).unwrap();

        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.shortcuts.len(), 2);
        assert_eq!(loaded.shortcuts.get("custom.action1").unwrap(), "Ctrl+1");
        assert_eq!(loaded.shortcuts.get("custom.action2").unwrap(), "Ctrl+2");
    }

    #[test]
    fn test_custom_shortcuts_merge_with_defaults() {
        let default_config = ShortcutStore::default_config();
        let mut merged = default_config.shortcuts.clone();

        // Custom shortcuts override existing and add new
        merged.insert("workspace.new".into(), "Ctrl+Shift+N".into()); // override
        merged.insert("custom.action".into(), "Ctrl+Alt+X".into()); // new

        assert_eq!(merged.get("workspace.new").unwrap(), "Ctrl+Shift+N");
        assert_eq!(merged.get("custom.action").unwrap(), "Ctrl+Alt+X");
        // Existing defaults still present
        assert_eq!(merged.get("workspace.close").unwrap(), "Cmd+W");
        assert_eq!(merged.len(), default_config.shortcuts.len() + 1);
    }

    #[test]
    fn test_shortcut_store_save_updates_config_in_memory() {
        // We can't use save() directly because it writes to the global path,
        // but we can test that the config field is updated after assignment
        let initial_config = ShortcutConfig {
            version: 1,
            shortcuts: HashMap::new(),
        };
        let mut store = ShortcutStore {
            config: initial_config,
        };

        assert!(store.list().shortcuts.is_empty());

        // Simulate what save() does to in-memory state
        let mut new_shortcuts = HashMap::new();
        new_shortcuts.insert("test.action".into(), "Ctrl+T".into());
        let new_config = ShortcutConfig {
            version: 2,
            shortcuts: new_shortcuts,
        };
        store.config = new_config;

        let listed = store.list();
        assert_eq!(listed.version, 2);
        assert_eq!(listed.shortcuts.len(), 1);
        assert_eq!(listed.shortcuts.get("test.action").unwrap(), "Ctrl+T");
    }

    #[test]
    fn test_shortcut_config_with_many_shortcuts_roundtrip() {
        let mut shortcuts = HashMap::new();
        for i in 0..50 {
            shortcuts.insert(format!("action.{i}"), format!("Ctrl+Alt+{i}"));
        }
        let config = ShortcutConfig {
            version: 1,
            shortcuts,
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: ShortcutConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.shortcuts.len(), 50);
        assert_eq!(parsed.shortcuts.get("action.0").unwrap(), "Ctrl+Alt+0");
        assert_eq!(parsed.shortcuts.get("action.49").unwrap(), "Ctrl+Alt+49");
    }

    #[test]
    fn test_config_path_ends_with_shortcuts_json() {
        let path = ShortcutStore::config_path();
        let path_str = path.to_string_lossy();
        assert!(path_str.ends_with("shortcuts.json"));
        assert!(path_str.contains(".ocestrater"));
    }

    #[test]
    fn test_shortcut_store_list_returns_clone() {
        let config = ShortcutConfig {
            version: 1,
            shortcuts: {
                let mut s = HashMap::new();
                s.insert("a".into(), "b".into());
                s
            },
        };
        let store = ShortcutStore { config };
        let listed1 = store.list();
        let listed2 = store.list();
        // Both should be equal
        assert_eq!(listed1.version, listed2.version);
        assert_eq!(listed1.shortcuts.len(), listed2.shortcuts.len());
    }
}
