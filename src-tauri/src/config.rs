use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub version: u32,
    pub agents: HashMap<String, AgentConfig>,
    pub defaults: Defaults,
    pub repositories: Vec<RepoRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(default)]
    pub model_flag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Defaults {
    pub agent: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_max_concurrent_agents")]
    pub max_concurrent_agents: usize,
}

fn default_max_concurrent_agents() -> usize {
    8
}

fn default_theme() -> String {
    "system".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoRef {
    pub path: String,
    pub alias: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub setup_script: Option<String>,
    #[serde(default)]
    pub default_agent: Option<String>,
    #[serde(default)]
    pub default_branch: Option<String>,
    #[serde(default = "default_worktree_dir")]
    pub worktree_dir: String,
    #[serde(default)]
    pub snippets: HashMap<String, String>,
    #[serde(default)]
    pub agent_overrides: HashMap<String, AgentOverride>,
}

fn default_version() -> u32 {
    CONFIG_VERSION
}
fn default_worktree_dir() -> String {
    ".worktrees".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentOverride {
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// Holds resolved configuration (global merged with repo-level)
pub struct ConfigStore {
    pub global: GlobalConfig,
    pub repo_configs: HashMap<String, RepoConfig>,
    config_dir: PathBuf,
}

impl ConfigStore {
    pub fn config_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".ocestrater")
    }

    pub fn load_or_default() -> Self {
        let config_dir = Self::config_dir();
        let global_path = config_dir.join("config.json");

        let global = if global_path.exists() {
            match std::fs::read_to_string(&global_path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_else(|_| Self::default_global()),
                Err(_) => Self::default_global(),
            }
        } else {
            let g = Self::default_global();
            // Create config dir and save defaults
            let _ = std::fs::create_dir_all(&config_dir);
            let _ = std::fs::write(
                &global_path,
                serde_json::to_string_pretty(&g).unwrap_or_default(),
            );
            g
        };

        let mut store = Self {
            global,
            repo_configs: HashMap::new(),
            config_dir,
        };

        // Load per-repo configs
        for repo in &store.global.repositories.clone() {
            store.load_repo_config(&repo.path);
        }

        store
    }

    fn default_global() -> GlobalConfig {
        let mut agents = HashMap::new();
        agents.insert(
            "claude".into(),
            AgentConfig {
                command: "claude".into(),
                args: vec![],
                env: HashMap::new(),
                models: vec!["opus".into(), "sonnet".into(), "haiku".into()],
                default_model: Some("sonnet".into()),
                model_flag: Some("--model".into()),
            },
        );
        agents.insert(
            "codex".into(),
            AgentConfig {
                command: "codex".into(),
                args: vec![],
                env: HashMap::new(),
                models: vec!["o3".into(), "o4-mini".into(), "gpt-4.1".into()],
                default_model: Some("o4-mini".into()),
                model_flag: Some("--model".into()),
            },
        );
        agents.insert(
            "gemini".into(),
            AgentConfig {
                command: "gemini".into(),
                args: vec![],
                env: HashMap::new(),
                models: vec!["gemini-2.5-pro".into(), "gemini-2.5-flash".into()],
                default_model: Some("gemini-2.5-flash".into()),
                model_flag: Some("--model".into()),
            },
        );

        GlobalConfig {
            version: CONFIG_VERSION,
            agents,
            defaults: Defaults {
                agent: "claude".into(),
                theme: "system".into(),
                max_concurrent_agents: default_max_concurrent_agents(),
            },
            repositories: vec![],
        }
    }

    fn default_repo_config() -> RepoConfig {
        RepoConfig {
            version: CONFIG_VERSION,
            setup_script: None,
            default_agent: None,
            default_branch: None,
            worktree_dir: ".worktrees".into(),
            snippets: HashMap::new(),
            agent_overrides: HashMap::new(),
        }
    }

    pub fn load_repo_config(&mut self, repo_path: &str) {
        let repo_config_path = Path::new(repo_path).join(".ocestrater/config.json");
        let config = if repo_config_path.exists() {
            match std::fs::read_to_string(&repo_config_path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_else(|_| Self::default_repo_config()),
                Err(_) => Self::default_repo_config(),
            }
        } else {
            Self::default_repo_config()
        };
        self.repo_configs.insert(repo_path.to_string(), config);
    }

    /// Resolve agent config: repo overrides > global agent config
    pub fn resolve_agent(&self, repo_path: &str, agent_name: &str) -> Option<AgentConfig> {
        let base = self.global.agents.get(agent_name)?.clone();

        if let Some(repo_cfg) = self.repo_configs.get(repo_path) {
            if let Some(overrides) = repo_cfg.agent_overrides.get(agent_name) {
                let args = if overrides.args.is_empty() {
                    base.args
                } else {
                    overrides.args.clone()
                };
                let mut env = base.env;
                env.extend(overrides.env.clone());
                return Some(AgentConfig {
                    command: base.command,
                    args,
                    env,
                    models: base.models,
                    default_model: base.default_model,
                    model_flag: base.model_flag,
                });
            }
        }

        Some(base)
    }

    pub fn save_global(&self) -> Result<(), String> {
        let _ = std::fs::create_dir_all(&self.config_dir);
        let path = self.config_dir.join("config.json");
        let json = serde_json::to_string_pretty(&self.global)
            .map_err(|e| format!("serialize error: {e}"))?;
        std::fs::write(&path, json).map_err(|e| format!("write error: {e}"))
    }

    pub fn add_repository(&mut self, path: String, alias: String) {
        if !self.global.repositories.iter().any(|r| r.path == path) {
            self.global.repositories.push(RepoRef {
                path: path.clone(),
                alias,
            });
            self.load_repo_config(&path);
        }
    }

    pub fn remove_repository(&mut self, path: &str) {
        self.global.repositories.retain(|r| r.path != path);
        self.repo_configs.remove(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_store() -> ConfigStore {
        ConfigStore {
            global: ConfigStore::default_global(),
            repo_configs: HashMap::new(),
            config_dir: PathBuf::from("/tmp/test-ocestrater"),
        }
    }

    // ── Default config tests ──

    #[test]
    fn test_default_global_has_three_agents() {
        let g = ConfigStore::default_global();
        assert_eq!(g.agents.len(), 3);
        assert!(g.agents.contains_key("claude"));
        assert!(g.agents.contains_key("codex"));
        assert!(g.agents.contains_key("gemini"));
    }

    #[test]
    fn test_default_global_version() {
        let g = ConfigStore::default_global();
        assert_eq!(g.version, CONFIG_VERSION);
    }

    #[test]
    fn test_default_global_defaults() {
        let g = ConfigStore::default_global();
        assert_eq!(g.defaults.agent, "claude");
        assert_eq!(g.defaults.theme, "system");
        assert_eq!(g.defaults.max_concurrent_agents, 8);
    }

    #[test]
    fn test_default_global_empty_repositories() {
        let g = ConfigStore::default_global();
        assert!(g.repositories.is_empty());
    }

    #[test]
    fn test_default_repo_config() {
        let rc = ConfigStore::default_repo_config();
        assert_eq!(rc.version, CONFIG_VERSION);
        assert_eq!(rc.worktree_dir, ".worktrees");
        assert!(rc.setup_script.is_none());
        assert!(rc.default_agent.is_none());
        assert!(rc.default_branch.is_none());
        assert!(rc.snippets.is_empty());
        assert!(rc.agent_overrides.is_empty());
    }

    // ── resolve_agent tests ──

    #[test]
    fn test_resolve_agent_no_repo_overrides() {
        let store = make_store();
        let agent = store.resolve_agent("/some/repo", "claude").unwrap();
        assert_eq!(agent.command, "claude");
        assert!(agent.args.is_empty());
        assert!(agent.env.is_empty());
    }

    #[test]
    fn test_resolve_agent_unknown_agent_returns_none() {
        let store = make_store();
        assert!(store.resolve_agent("/some/repo", "nonexistent").is_none());
    }

    #[test]
    fn test_resolve_agent_with_repo_override_args() {
        let mut store = make_store();

        let mut overrides = HashMap::new();
        overrides.insert(
            "claude".to_string(),
            AgentOverride {
                args: vec!["--custom-flag".to_string()],
                env: HashMap::new(),
            },
        );

        store.repo_configs.insert(
            "/my/repo".to_string(),
            RepoConfig {
                version: 1,
                setup_script: None,
                default_agent: None,
                default_branch: None,
                worktree_dir: ".worktrees".into(),
                snippets: HashMap::new(),
                agent_overrides: overrides,
            },
        );

        let agent = store.resolve_agent("/my/repo", "claude").unwrap();
        assert_eq!(agent.command, "claude");
        assert_eq!(agent.args, vec!["--custom-flag"]);
    }

    #[test]
    fn test_resolve_agent_with_repo_override_env() {
        let mut store = make_store();

        let mut override_env = HashMap::new();
        override_env.insert("MY_VAR".to_string(), "my_value".to_string());

        let mut overrides = HashMap::new();
        overrides.insert(
            "claude".to_string(),
            AgentOverride {
                args: vec![],
                env: override_env,
            },
        );

        store.repo_configs.insert(
            "/my/repo".to_string(),
            RepoConfig {
                version: 1,
                setup_script: None,
                default_agent: None,
                default_branch: None,
                worktree_dir: ".worktrees".into(),
                snippets: HashMap::new(),
                agent_overrides: overrides,
            },
        );

        let agent = store.resolve_agent("/my/repo", "claude").unwrap();
        // When override args are empty, base args are used
        assert!(agent.args.is_empty());
        // Override env is merged
        assert_eq!(agent.env.get("MY_VAR").unwrap(), "my_value");
    }

    #[test]
    fn test_resolve_agent_repo_override_env_merges_with_base() {
        let mut store = make_store();

        // Add base env to the global agent config
        store
            .global
            .agents
            .get_mut("claude")
            .unwrap()
            .env
            .insert("BASE_VAR".to_string(), "base_value".to_string());

        let mut override_env = HashMap::new();
        override_env.insert("REPO_VAR".to_string(), "repo_value".to_string());

        let mut overrides = HashMap::new();
        overrides.insert(
            "claude".to_string(),
            AgentOverride {
                args: vec![],
                env: override_env,
            },
        );

        store.repo_configs.insert(
            "/my/repo".to_string(),
            RepoConfig {
                version: 1,
                setup_script: None,
                default_agent: None,
                default_branch: None,
                worktree_dir: ".worktrees".into(),
                snippets: HashMap::new(),
                agent_overrides: overrides,
            },
        );

        let agent = store.resolve_agent("/my/repo", "claude").unwrap();
        assert_eq!(agent.env.get("BASE_VAR").unwrap(), "base_value");
        assert_eq!(agent.env.get("REPO_VAR").unwrap(), "repo_value");
    }

    // ── add_repository / remove_repository tests ──

    #[test]
    fn test_add_repository() {
        let mut store = make_store();
        assert!(store.global.repositories.is_empty());

        store.add_repository("/path/to/repo".into(), "my-repo".into());
        assert_eq!(store.global.repositories.len(), 1);
        assert_eq!(store.global.repositories[0].path, "/path/to/repo");
        assert_eq!(store.global.repositories[0].alias, "my-repo");
        // Also loads repo config
        assert!(store.repo_configs.contains_key("/path/to/repo"));
    }

    #[test]
    fn test_add_repository_no_duplicates() {
        let mut store = make_store();
        store.add_repository("/path/to/repo".into(), "my-repo".into());
        store.add_repository("/path/to/repo".into(), "different-alias".into());
        assert_eq!(store.global.repositories.len(), 1);
        // Original alias is preserved
        assert_eq!(store.global.repositories[0].alias, "my-repo");
    }

    #[test]
    fn test_remove_repository() {
        let mut store = make_store();
        store.add_repository("/path/to/repo".into(), "my-repo".into());
        store.add_repository("/path/to/other".into(), "other-repo".into());

        store.remove_repository("/path/to/repo");

        assert_eq!(store.global.repositories.len(), 1);
        assert_eq!(store.global.repositories[0].path, "/path/to/other");
        assert!(!store.repo_configs.contains_key("/path/to/repo"));
        assert!(store.repo_configs.contains_key("/path/to/other"));
    }

    #[test]
    fn test_remove_nonexistent_repository_is_noop() {
        let mut store = make_store();
        store.add_repository("/path/to/repo".into(), "my-repo".into());
        store.remove_repository("/nonexistent");
        assert_eq!(store.global.repositories.len(), 1);
    }

    // ── Serialization roundtrip tests ──

    #[test]
    fn test_global_config_serialization_roundtrip() {
        let original = ConfigStore::default_global();
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: GlobalConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.version, original.version);
        assert_eq!(deserialized.defaults.agent, original.defaults.agent);
        assert_eq!(deserialized.defaults.theme, original.defaults.theme);
        assert_eq!(
            deserialized.defaults.max_concurrent_agents,
            original.defaults.max_concurrent_agents
        );
        assert_eq!(deserialized.agents.len(), original.agents.len());
        assert_eq!(deserialized.repositories.len(), original.repositories.len());
    }

    #[test]
    fn test_repo_config_serialization_roundtrip() {
        let mut snippets = HashMap::new();
        snippets.insert("test".to_string(), "cargo test".to_string());

        let original = RepoConfig {
            version: 1,
            setup_script: Some("npm install".to_string()),
            default_agent: Some("claude".to_string()),
            default_branch: Some("develop".to_string()),
            worktree_dir: ".wt".into(),
            snippets,
            agent_overrides: HashMap::new(),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: RepoConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.version, original.version);
        assert_eq!(deserialized.setup_script, original.setup_script);
        assert_eq!(deserialized.default_agent, original.default_agent);
        assert_eq!(deserialized.default_branch, original.default_branch);
        assert_eq!(deserialized.worktree_dir, original.worktree_dir);
        assert_eq!(deserialized.snippets.get("test").unwrap(), "cargo test");
    }

    #[test]
    fn test_agent_config_serialization_roundtrip() {
        let mut env = HashMap::new();
        env.insert("KEY".to_string(), "VALUE".to_string());

        let original = AgentConfig {
            command: "claude".to_string(),
            args: vec!["--flag".to_string()],
            env,
            models: vec!["opus".to_string(), "sonnet".to_string()],
            default_model: Some("sonnet".to_string()),
            model_flag: Some("--model".to_string()),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: AgentConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.command, "claude");
        assert_eq!(deserialized.args, vec!["--flag"]);
        assert_eq!(deserialized.env.get("KEY").unwrap(), "VALUE");
    }

    // ── Defaults serde tests ──

    #[test]
    fn test_defaults_serde_defaults() {
        let json = r#"{"agent": "codex"}"#;
        let defaults: Defaults = serde_json::from_str(json).unwrap();
        assert_eq!(defaults.agent, "codex");
        assert_eq!(defaults.theme, "system");
        assert_eq!(defaults.max_concurrent_agents, 8);
    }

    #[test]
    fn test_repo_config_serde_defaults() {
        let json = "{}";
        let rc: RepoConfig = serde_json::from_str(json).unwrap();
        assert_eq!(rc.version, CONFIG_VERSION);
        assert_eq!(rc.worktree_dir, ".worktrees");
        assert!(rc.setup_script.is_none());
        assert!(rc.snippets.is_empty());
    }

    // ── Phase 3 additional tests: model fields ──

    #[test]
    fn test_agent_config_models_field() {
        let config = ConfigStore::default_global();
        let claude = config.agents.get("claude").unwrap();
        assert_eq!(claude.models, vec!["opus", "sonnet", "haiku"]);
    }

    #[test]
    fn test_agent_config_default_model_field() {
        let config = ConfigStore::default_global();
        let claude = config.agents.get("claude").unwrap();
        assert_eq!(claude.default_model, Some("sonnet".to_string()));

        let codex = config.agents.get("codex").unwrap();
        assert_eq!(codex.default_model, Some("o4-mini".to_string()));

        let gemini = config.agents.get("gemini").unwrap();
        assert_eq!(gemini.default_model, Some("gemini-2.5-flash".to_string()));
    }

    #[test]
    fn test_agent_config_model_flag_field() {
        let config = ConfigStore::default_global();
        let claude = config.agents.get("claude").unwrap();
        assert_eq!(claude.model_flag, Some("--model".to_string()));

        let codex = config.agents.get("codex").unwrap();
        assert_eq!(codex.model_flag, Some("--model".to_string()));
    }

    #[test]
    fn test_agent_config_serde_defaults_for_new_fields() {
        // When models, default_model, model_flag are missing from JSON,
        // they should default to empty vec / None / None
        let json = r#"{"command": "test-agent", "args": []}"#;
        let config: AgentConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.command, "test-agent");
        assert!(config.models.is_empty());
        assert!(config.default_model.is_none());
        assert!(config.model_flag.is_none());
    }

    #[test]
    fn test_agent_config_full_roundtrip_with_model_fields() {
        let config = AgentConfig {
            command: "my-agent".to_string(),
            args: vec!["--verbose".to_string()],
            env: HashMap::new(),
            models: vec!["model-a".to_string(), "model-b".to_string()],
            default_model: Some("model-a".to_string()),
            model_flag: Some("--model".to_string()),
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: AgentConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.models, vec!["model-a", "model-b"]);
        assert_eq!(parsed.default_model, Some("model-a".to_string()));
        assert_eq!(parsed.model_flag, Some("--model".to_string()));
    }

    #[test]
    fn test_resolve_agent_preserves_model_fields() {
        let store = make_store();
        let agent = store.resolve_agent("/some/repo", "claude").unwrap();
        assert_eq!(agent.models, vec!["opus", "sonnet", "haiku"]);
        assert_eq!(agent.default_model, Some("sonnet".to_string()));
        assert_eq!(agent.model_flag, Some("--model".to_string()));
    }

    #[test]
    fn test_resolve_agent_with_override_preserves_model_fields() {
        let mut store = make_store();

        let mut overrides = HashMap::new();
        overrides.insert(
            "claude".to_string(),
            AgentOverride {
                args: vec!["--custom".to_string()],
                env: HashMap::new(),
            },
        );

        store.repo_configs.insert(
            "/model/repo".to_string(),
            RepoConfig {
                version: 1,
                setup_script: None,
                default_agent: None,
                default_branch: None,
                worktree_dir: ".worktrees".into(),
                snippets: HashMap::new(),
                agent_overrides: overrides,
            },
        );

        let agent = store.resolve_agent("/model/repo", "claude").unwrap();
        // Override changes args but model fields come from base
        assert_eq!(agent.args, vec!["--custom"]);
        assert_eq!(agent.models, vec!["opus", "sonnet", "haiku"]);
        assert_eq!(agent.default_model, Some("sonnet".to_string()));
        assert_eq!(agent.model_flag, Some("--model".to_string()));
    }

    #[test]
    fn test_codex_agent_models() {
        let config = ConfigStore::default_global();
        let codex = config.agents.get("codex").unwrap();
        assert_eq!(codex.models, vec!["o3", "o4-mini", "gpt-4.1"]);
        assert_eq!(codex.command, "codex");
    }

    #[test]
    fn test_gemini_agent_models() {
        let config = ConfigStore::default_global();
        let gemini = config.agents.get("gemini").unwrap();
        assert_eq!(
            gemini.models,
            vec!["gemini-2.5-pro", "gemini-2.5-flash"]
        );
        assert_eq!(gemini.command, "gemini");
    }

    // ── Additional edge case tests ──

    #[test]
    fn test_config_dir_ends_with_ocestrater() {
        let dir = ConfigStore::config_dir();
        let dir_str = dir.to_string_lossy();
        assert!(
            dir_str.ends_with(".ocestrater"),
            "config_dir should end with .ocestrater, got: {dir_str}"
        );
    }

    #[test]
    fn test_agent_override_serde_roundtrip() {
        let mut env = HashMap::new();
        env.insert("CUSTOM_VAR".to_string(), "custom_val".to_string());

        let original = AgentOverride {
            args: vec!["--verbose".to_string(), "--debug".to_string()],
            env,
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: AgentOverride = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.args, vec!["--verbose", "--debug"]);
        assert_eq!(parsed.env.get("CUSTOM_VAR").unwrap(), "custom_val");
    }

    #[test]
    fn test_agent_override_empty_roundtrip() {
        let original = AgentOverride {
            args: vec![],
            env: HashMap::new(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: AgentOverride = serde_json::from_str(&json).unwrap();
        assert!(parsed.args.is_empty());
        assert!(parsed.env.is_empty());
    }

    #[test]
    fn test_repo_ref_serde_roundtrip() {
        let original = RepoRef {
            path: "/home/user/projects/my-app".to_string(),
            alias: "my-app".to_string(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: RepoRef = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.path, "/home/user/projects/my-app");
        assert_eq!(parsed.alias, "my-app");
    }

    #[test]
    fn test_multiple_repo_configs_loaded_independently() {
        let mut store = make_store();

        let mut overrides_a = HashMap::new();
        overrides_a.insert(
            "claude".to_string(),
            AgentOverride {
                args: vec!["--repo-a-flag".to_string()],
                env: HashMap::new(),
            },
        );

        let mut overrides_b = HashMap::new();
        overrides_b.insert(
            "claude".to_string(),
            AgentOverride {
                args: vec!["--repo-b-flag".to_string()],
                env: HashMap::new(),
            },
        );

        store.repo_configs.insert(
            "/repo/a".to_string(),
            RepoConfig {
                version: 1,
                setup_script: None,
                default_agent: None,
                default_branch: None,
                worktree_dir: ".worktrees".into(),
                snippets: HashMap::new(),
                agent_overrides: overrides_a,
            },
        );

        store.repo_configs.insert(
            "/repo/b".to_string(),
            RepoConfig {
                version: 1,
                setup_script: None,
                default_agent: None,
                default_branch: None,
                worktree_dir: ".worktrees".into(),
                snippets: HashMap::new(),
                agent_overrides: overrides_b,
            },
        );

        let agent_a = store.resolve_agent("/repo/a", "claude").unwrap();
        let agent_b = store.resolve_agent("/repo/b", "claude").unwrap();

        assert_eq!(agent_a.args, vec!["--repo-a-flag"]);
        assert_eq!(agent_b.args, vec!["--repo-b-flag"]);
    }

    #[test]
    fn test_resolve_agent_empty_override_args_uses_base_args() {
        let mut store = make_store();

        // Set base args on claude
        store
            .global
            .agents
            .get_mut("claude")
            .unwrap()
            .args = vec!["--base-flag".to_string()];

        let mut overrides = HashMap::new();
        overrides.insert(
            "claude".to_string(),
            AgentOverride {
                args: vec![], // empty override args
                env: HashMap::new(),
            },
        );

        store.repo_configs.insert(
            "/my/repo".to_string(),
            RepoConfig {
                version: 1,
                setup_script: None,
                default_agent: None,
                default_branch: None,
                worktree_dir: ".worktrees".into(),
                snippets: HashMap::new(),
                agent_overrides: overrides,
            },
        );

        let agent = store.resolve_agent("/my/repo", "claude").unwrap();
        // Empty override args should fallback to base args
        assert_eq!(agent.args, vec!["--base-flag"]);
    }

    #[test]
    fn test_resolve_agent_override_with_both_args_and_env() {
        let mut store = make_store();

        let mut override_env = HashMap::new();
        override_env.insert("ENV_KEY".to_string(), "env_value".to_string());

        let mut overrides = HashMap::new();
        overrides.insert(
            "codex".to_string(),
            AgentOverride {
                args: vec!["--custom-codex-arg".to_string()],
                env: override_env,
            },
        );

        store.repo_configs.insert(
            "/my/repo".to_string(),
            RepoConfig {
                version: 1,
                setup_script: None,
                default_agent: None,
                default_branch: None,
                worktree_dir: ".worktrees".into(),
                snippets: HashMap::new(),
                agent_overrides: overrides,
            },
        );

        let agent = store.resolve_agent("/my/repo", "codex").unwrap();
        assert_eq!(agent.args, vec!["--custom-codex-arg"]);
        assert_eq!(agent.env.get("ENV_KEY").unwrap(), "env_value");
        assert_eq!(agent.command, "codex");
    }

    #[test]
    fn test_add_then_remove_repository_leaves_store_empty() {
        let mut store = make_store();
        assert!(store.global.repositories.is_empty());
        assert!(store.repo_configs.is_empty());

        store.add_repository("/path/to/repo".into(), "my-repo".into());
        assert_eq!(store.global.repositories.len(), 1);
        assert_eq!(store.repo_configs.len(), 1);

        store.remove_repository("/path/to/repo");
        assert!(store.global.repositories.is_empty());
        assert!(store.repo_configs.is_empty());
    }

    #[test]
    fn test_global_config_custom_max_concurrent_agents() {
        let json = r#"{
            "version": 1,
            "agents": {},
            "defaults": {
                "agent": "claude",
                "theme": "dark",
                "max_concurrent_agents": 16
            },
            "repositories": []
        }"#;
        let config: GlobalConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.defaults.max_concurrent_agents, 16);
        assert_eq!(config.defaults.theme, "dark");
    }

    #[test]
    fn test_global_config_default_max_concurrent_agents_is_8() {
        assert_eq!(default_max_concurrent_agents(), 8);
    }

    #[test]
    fn test_global_config_default_theme_is_system() {
        assert_eq!(default_theme(), "system");
    }

    #[test]
    fn test_repo_config_with_all_fields_populated() {
        let mut snippets = HashMap::new();
        snippets.insert("test".to_string(), "cargo test".to_string());
        snippets.insert("build".to_string(), "cargo build".to_string());

        let mut agent_overrides = HashMap::new();
        agent_overrides.insert(
            "claude".to_string(),
            AgentOverride {
                args: vec!["--flag".to_string()],
                env: HashMap::new(),
            },
        );

        let config = RepoConfig {
            version: 1,
            setup_script: Some("./setup.sh".to_string()),
            default_agent: Some("codex".to_string()),
            default_branch: Some("develop".to_string()),
            worktree_dir: ".wt".to_string(),
            snippets,
            agent_overrides,
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: RepoConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.setup_script, Some("./setup.sh".to_string()));
        assert_eq!(parsed.default_agent, Some("codex".to_string()));
        assert_eq!(parsed.default_branch, Some("develop".to_string()));
        assert_eq!(parsed.worktree_dir, ".wt");
        assert_eq!(parsed.snippets.len(), 2);
        assert_eq!(parsed.agent_overrides.len(), 1);
    }
}
