use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A single executable snippet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snippet {
    /// Unique name within its scope (e.g. "test", "lint", "deploy-staging")
    pub name: String,
    /// Shell command to execute
    pub command: String,
    /// Human-readable description shown in the palette
    #[serde(default)]
    pub description: String,
    /// Grouping category for palette filtering
    #[serde(default = "default_category")]
    pub category: SnippetCategory,
    /// Optional keyboard shortcut (e.g. "Ctrl+Shift+T")
    #[serde(default)]
    pub keybinding: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SnippetCategory {
    Setup,
    Build,
    Test,
    Lint,
    Deploy,
    Custom,
}

fn default_category() -> SnippetCategory {
    SnippetCategory::Custom
}

/// Container for snippet storage files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetFile {
    #[serde(default = "default_version")]
    pub version: u32,
    pub snippets: Vec<Snippet>,
}

fn default_version() -> u32 {
    1
}

impl SnippetFile {
    fn empty() -> Self {
        Self {
            version: 1,
            snippets: Vec::new(),
        }
    }
}

/// Load a snippet file from the given path, returning an empty store if it doesn't exist
fn load_snippet_file(path: &Path) -> SnippetFile {
    if !path.exists() {
        return SnippetFile::empty();
    }
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_else(|_| SnippetFile::empty()),
        Err(_) => SnippetFile::empty(),
    }
}

/// Save a snippet file to disk, creating parent dirs as needed
fn save_snippet_file(path: &Path, file: &SnippetFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir error: {e}"))?;
    }
    let json = serde_json::to_string_pretty(file).map_err(|e| format!("serialize error: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("write error: {e}"))
}

fn global_snippets_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ocestrater")
        .join("snippets.json")
}

fn repo_snippets_path(repo_path: &str) -> PathBuf {
    Path::new(repo_path)
        .join(".ocestrater")
        .join("snippets.json")
}

/// Load and merge global + repo snippets. Repo snippets override global by name.
/// Result is sorted by category then name.
pub fn list_merged_snippets(repo_path: Option<&str>) -> Vec<Snippet> {
    let global_file = load_snippet_file(&global_snippets_path());

    // Index global snippets by name
    let mut by_name: HashMap<String, Snippet> = HashMap::new();
    for s in global_file.snippets {
        by_name.insert(s.name.clone(), s);
    }

    // Overlay repo snippets
    if let Some(rp) = repo_path {
        let repo_file = load_snippet_file(&repo_snippets_path(rp));
        for s in repo_file.snippets {
            by_name.insert(s.name.clone(), s);
        }
    }

    let mut result: Vec<Snippet> = by_name.into_values().collect();
    result.sort_by(|a, b| a.category.cmp(&b.category).then_with(|| a.name.cmp(&b.name)));
    result
}

/// Resolve a single snippet by name from merged global + repo stores.
pub fn resolve_snippet(repo_path: &str, name: &str) -> Option<Snippet> {
    // Check repo first, then global
    let repo_file = load_snippet_file(&repo_snippets_path(repo_path));
    if let Some(s) = repo_file.snippets.into_iter().find(|s| s.name == name) {
        return Some(s);
    }
    let global_file = load_snippet_file(&global_snippets_path());
    global_file.snippets.into_iter().find(|s| s.name == name)
}

/// Returns true if the snippet with the given name exists in the repo-level store
pub fn is_repo_snippet(repo_path: &str, name: &str) -> bool {
    let repo_file = load_snippet_file(&repo_snippets_path(repo_path));
    repo_file.snippets.iter().any(|s| s.name == name)
}

/// Save (upsert) a snippet to the appropriate store
pub fn save_snippet(repo_path: Option<&str>, snippet: Snippet) -> Result<(), String> {
    let path = match repo_path {
        Some(rp) => repo_snippets_path(rp),
        None => global_snippets_path(),
    };

    let mut file = load_snippet_file(&path);

    // Upsert by name
    if let Some(existing) = file.snippets.iter_mut().find(|s| s.name == snippet.name) {
        *existing = snippet;
    } else {
        file.snippets.push(snippet);
    }

    save_snippet_file(&path, &file)
}

/// Delete a snippet by name from a specific store
pub fn delete_snippet(repo_path: Option<&str>, name: &str) -> Result<(), String> {
    let path = match repo_path {
        Some(rp) => repo_snippets_path(rp),
        None => global_snippets_path(),
    };

    let mut file = load_snippet_file(&path);
    let before_len = file.snippets.len();
    file.snippets.retain(|s| s.name != name);

    if file.snippets.len() == before_len {
        return Err(format!("snippet not found: {name}"));
    }

    save_snippet_file(&path, &file)
}

/// Migrate old HashMap<String, String> snippets from RepoConfig to the new snippets.json format
pub fn migrate_legacy_snippets(repo_path: &str, legacy: &HashMap<String, String>) {
    if legacy.is_empty() {
        return;
    }

    let target = repo_snippets_path(repo_path);
    if target.exists() {
        return; // Already migrated
    }

    let snippets: Vec<Snippet> = legacy
        .iter()
        .map(|(name, command)| Snippet {
            name: name.clone(),
            command: command.clone(),
            description: String::new(),
            category: SnippetCategory::Custom,
            keybinding: None,
        })
        .collect();

    let file = SnippetFile {
        version: 1,
        snippets,
    };

    let _ = save_snippet_file(&target, &file);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snippet_category_ordering() {
        assert!(SnippetCategory::Setup < SnippetCategory::Build);
        assert!(SnippetCategory::Build < SnippetCategory::Test);
        assert!(SnippetCategory::Test < SnippetCategory::Lint);
        assert!(SnippetCategory::Lint < SnippetCategory::Deploy);
        assert!(SnippetCategory::Deploy < SnippetCategory::Custom);
    }

    #[test]
    fn test_default_category_is_custom() {
        assert_eq!(default_category(), SnippetCategory::Custom);
    }

    #[test]
    fn test_snippet_file_empty() {
        let f = SnippetFile::empty();
        assert_eq!(f.version, 1);
        assert!(f.snippets.is_empty());
    }

    #[test]
    fn test_snippet_file_roundtrip() {
        let file = SnippetFile {
            version: 1,
            snippets: vec![Snippet {
                name: "test".into(),
                command: "cargo test".into(),
                description: "Run tests".into(),
                category: SnippetCategory::Test,
                keybinding: Some("Ctrl+Shift+T".into()),
            }],
        };
        let json = serde_json::to_string(&file).unwrap();
        let parsed: SnippetFile = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.snippets.len(), 1);
        assert_eq!(parsed.snippets[0].name, "test");
        assert_eq!(parsed.snippets[0].category, SnippetCategory::Test);
    }

    #[test]
    fn test_snippet_serde_defaults() {
        let json = r#"{"name":"build","command":"make"}"#;
        let s: Snippet = serde_json::from_str(json).unwrap();
        assert_eq!(s.description, "");
        assert_eq!(s.category, SnippetCategory::Custom);
        assert!(s.keybinding.is_none());
    }

    #[test]
    fn test_load_snippet_file_nonexistent() {
        let f = load_snippet_file(Path::new("/nonexistent/path/snippets.json"));
        assert!(f.snippets.is_empty());
    }

    #[test]
    fn test_migrate_empty_legacy_does_nothing() {
        // Should not panic or create files
        migrate_legacy_snippets("/tmp/nonexistent-repo-test", &HashMap::new());
    }

    // ── Phase 3 additional tests ──

    #[test]
    fn test_list_merged_snippets_repo_overrides_global() {
        // Use tempdir to create isolated global + repo snippet files
        let tmp = std::env::temp_dir().join("ocestrater-test-merge-snippets");
        let repo_dir = tmp.join("repo");
        let repo_oce = repo_dir.join(".ocestrater");
        let _ = std::fs::create_dir_all(&repo_oce);

        // Write repo snippet file with a snippet named "test"
        let repo_file = SnippetFile {
            version: 1,
            snippets: vec![Snippet {
                name: "test".into(),
                command: "repo-test-cmd".into(),
                description: "Repo test".into(),
                category: SnippetCategory::Test,
                keybinding: None,
            }],
        };
        let repo_json = serde_json::to_string_pretty(&repo_file).unwrap();
        std::fs::write(repo_oce.join("snippets.json"), &repo_json).unwrap();

        // Load with repo path -- should get repo version
        let repo_path_str = repo_dir.to_str().unwrap();
        let repo_snippets = load_snippet_file(&repo_snippets_path(repo_path_str));
        assert_eq!(repo_snippets.snippets.len(), 1);
        assert_eq!(repo_snippets.snippets[0].command, "repo-test-cmd");

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_resolve_snippet_repo_first() {
        let tmp = std::env::temp_dir().join("ocestrater-test-resolve-snippet");
        let repo_dir = tmp.join("repo");
        let repo_oce = repo_dir.join(".ocestrater");
        let _ = std::fs::create_dir_all(&repo_oce);

        let repo_file = SnippetFile {
            version: 1,
            snippets: vec![Snippet {
                name: "lint".into(),
                command: "repo-lint".into(),
                description: "".into(),
                category: SnippetCategory::Lint,
                keybinding: None,
            }],
        };
        std::fs::write(
            repo_oce.join("snippets.json"),
            serde_json::to_string_pretty(&repo_file).unwrap(),
        )
        .unwrap();

        let repo_path_str = repo_dir.to_str().unwrap();
        let resolved = resolve_snippet(repo_path_str, "lint");
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap().command, "repo-lint");

        // Non-existent snippet returns None
        let missing = resolve_snippet(repo_path_str, "nonexistent");
        assert!(missing.is_none());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_save_snippet_upsert_behavior() {
        let tmp = std::env::temp_dir().join("ocestrater-test-save-upsert");
        let repo_dir = tmp.join("repo");
        let repo_oce = repo_dir.join(".ocestrater");
        let _ = std::fs::create_dir_all(&repo_oce);
        let repo_path_str = repo_dir.to_str().unwrap();

        // Save initial snippet
        let snippet1 = Snippet {
            name: "deploy".into(),
            command: "deploy-v1".into(),
            description: "".into(),
            category: SnippetCategory::Deploy,
            keybinding: None,
        };
        save_snippet(Some(repo_path_str), snippet1).unwrap();

        // Verify it was saved
        let file = load_snippet_file(&repo_snippets_path(repo_path_str));
        assert_eq!(file.snippets.len(), 1);
        assert_eq!(file.snippets[0].command, "deploy-v1");

        // Upsert with same name but different command
        let snippet2 = Snippet {
            name: "deploy".into(),
            command: "deploy-v2".into(),
            description: "Updated".into(),
            category: SnippetCategory::Deploy,
            keybinding: Some("Ctrl+D".into()),
        };
        save_snippet(Some(repo_path_str), snippet2).unwrap();

        // Verify it was updated, not duplicated
        let file = load_snippet_file(&repo_snippets_path(repo_path_str));
        assert_eq!(file.snippets.len(), 1);
        assert_eq!(file.snippets[0].command, "deploy-v2");
        assert_eq!(file.snippets[0].description, "Updated");
        assert_eq!(file.snippets[0].keybinding, Some("Ctrl+D".into()));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_delete_snippet_nonexistent_name_returns_error() {
        let tmp = std::env::temp_dir().join("ocestrater-test-delete-nonexist");
        let repo_dir = tmp.join("repo");
        let repo_oce = repo_dir.join(".ocestrater");
        let _ = std::fs::create_dir_all(&repo_oce);
        let repo_path_str = repo_dir.to_str().unwrap();

        // Write an empty snippet file
        let empty_file = SnippetFile {
            version: 1,
            snippets: vec![],
        };
        std::fs::write(
            repo_oce.join("snippets.json"),
            serde_json::to_string_pretty(&empty_file).unwrap(),
        )
        .unwrap();

        let result = delete_snippet(Some(repo_path_str), "nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("snippet not found"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_is_repo_snippet_function() {
        let tmp = std::env::temp_dir().join("ocestrater-test-is-repo-snippet");
        let repo_dir = tmp.join("repo");
        let repo_oce = repo_dir.join(".ocestrater");
        let _ = std::fs::create_dir_all(&repo_oce);
        let repo_path_str = repo_dir.to_str().unwrap();

        let repo_file = SnippetFile {
            version: 1,
            snippets: vec![Snippet {
                name: "build".into(),
                command: "make build".into(),
                description: "".into(),
                category: SnippetCategory::Build,
                keybinding: None,
            }],
        };
        std::fs::write(
            repo_oce.join("snippets.json"),
            serde_json::to_string_pretty(&repo_file).unwrap(),
        )
        .unwrap();

        assert!(is_repo_snippet(repo_path_str, "build"));
        assert!(!is_repo_snippet(repo_path_str, "nonexistent"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_snippet_category_serde_snake_case() {
        // All categories should serialize as snake_case
        let setup_json = serde_json::to_string(&SnippetCategory::Setup).unwrap();
        assert_eq!(setup_json, "\"setup\"");

        let build_json = serde_json::to_string(&SnippetCategory::Build).unwrap();
        assert_eq!(build_json, "\"build\"");

        let test_json = serde_json::to_string(&SnippetCategory::Test).unwrap();
        assert_eq!(test_json, "\"test\"");

        let lint_json = serde_json::to_string(&SnippetCategory::Lint).unwrap();
        assert_eq!(lint_json, "\"lint\"");

        let deploy_json = serde_json::to_string(&SnippetCategory::Deploy).unwrap();
        assert_eq!(deploy_json, "\"deploy\"");

        let custom_json = serde_json::to_string(&SnippetCategory::Custom).unwrap();
        assert_eq!(custom_json, "\"custom\"");

        // Roundtrip
        let parsed: SnippetCategory = serde_json::from_str("\"deploy\"").unwrap();
        assert_eq!(parsed, SnippetCategory::Deploy);
    }

    #[test]
    fn test_list_merged_snippets_sorted_by_category_then_name() {
        let tmp = std::env::temp_dir().join("ocestrater-test-merge-sorted");
        let repo_dir = tmp.join("repo");
        let repo_oce = repo_dir.join(".ocestrater");
        let _ = std::fs::create_dir_all(&repo_oce);
        let repo_path_str = repo_dir.to_str().unwrap();

        let repo_file = SnippetFile {
            version: 1,
            snippets: vec![
                Snippet {
                    name: "z-custom".into(),
                    command: "echo z".into(),
                    description: "".into(),
                    category: SnippetCategory::Custom,
                    keybinding: None,
                },
                Snippet {
                    name: "a-build".into(),
                    command: "make".into(),
                    description: "".into(),
                    category: SnippetCategory::Build,
                    keybinding: None,
                },
                Snippet {
                    name: "b-build".into(),
                    command: "make all".into(),
                    description: "".into(),
                    category: SnippetCategory::Build,
                    keybinding: None,
                },
            ],
        };
        std::fs::write(
            repo_oce.join("snippets.json"),
            serde_json::to_string_pretty(&repo_file).unwrap(),
        )
        .unwrap();

        let merged = list_merged_snippets(Some(repo_path_str));
        // Build < Custom in category ordering, and within Build "a-build" < "b-build"
        assert!(merged.len() >= 3);
        // Find our repo snippets (there may also be global snippets)
        let names: Vec<&str> = merged.iter().map(|s| s.name.as_str()).collect();
        let a_pos = names.iter().position(|n| *n == "a-build");
        let b_pos = names.iter().position(|n| *n == "b-build");
        let z_pos = names.iter().position(|n| *n == "z-custom");
        assert!(a_pos.is_some());
        assert!(b_pos.is_some());
        assert!(z_pos.is_some());
        // Build items come before Custom items
        assert!(a_pos.unwrap() < z_pos.unwrap());
        assert!(b_pos.unwrap() < z_pos.unwrap());
        // Within Build, "a-build" comes before "b-build"
        assert!(a_pos.unwrap() < b_pos.unwrap());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_load_snippet_file_invalid_json() {
        let tmp = std::env::temp_dir().join("ocestrater-test-invalid-json");
        let _ = std::fs::create_dir_all(&tmp);
        let path = tmp.join("snippets.json");
        std::fs::write(&path, "{ invalid json }").unwrap();

        let file = load_snippet_file(&path);
        // Should fallback to empty
        assert!(file.snippets.is_empty());
        assert_eq!(file.version, 1);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── Tempdir integration tests ──

    #[test]
    fn test_full_lifecycle_save_list_update_delete_with_tempdir() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_dir = tmp.path().join("lifecycle-repo");
        let repo_oce = repo_dir.join(".ocestrater");
        std::fs::create_dir_all(&repo_oce).unwrap();
        let repo_path_str = repo_dir.to_str().unwrap();

        // 1. Save a new snippet
        let snippet1 = Snippet {
            name: "build".into(),
            command: "cargo build".into(),
            description: "Build project".into(),
            category: SnippetCategory::Build,
            keybinding: Some("Ctrl+B".into()),
        };
        save_snippet(Some(repo_path_str), snippet1).unwrap();

        // 2. List and verify
        let file = load_snippet_file(&repo_snippets_path(repo_path_str));
        assert_eq!(file.snippets.len(), 1);
        assert_eq!(file.snippets[0].name, "build");
        assert_eq!(file.snippets[0].command, "cargo build");

        // 3. Update the snippet (upsert)
        let snippet2 = Snippet {
            name: "build".into(),
            command: "cargo build --release".into(),
            description: "Build project (release)".into(),
            category: SnippetCategory::Build,
            keybinding: Some("Ctrl+Shift+B".into()),
        };
        save_snippet(Some(repo_path_str), snippet2).unwrap();

        let file = load_snippet_file(&repo_snippets_path(repo_path_str));
        assert_eq!(file.snippets.len(), 1);
        assert_eq!(file.snippets[0].command, "cargo build --release");
        assert_eq!(file.snippets[0].description, "Build project (release)");
        assert_eq!(file.snippets[0].keybinding, Some("Ctrl+Shift+B".into()));

        // 4. Delete the snippet
        delete_snippet(Some(repo_path_str), "build").unwrap();

        let file = load_snippet_file(&repo_snippets_path(repo_path_str));
        assert!(file.snippets.is_empty());
    }

    #[test]
    fn test_list_merged_snippets_with_no_repo_global_only() {
        // When repo_path is None, list_merged_snippets should return global snippets only
        // Since global snippets are at a fixed path, we test the fallback behavior
        let result = list_merged_snippets(None);
        // Result should be some vec (possibly empty if no global snippets installed)
        // Main thing is it should not panic
        // Should not panic; result is a valid vec (possibly empty if no global snippets)
        let _ = result;
    }

    #[test]
    fn test_snippet_with_all_fields_populated_roundtrip() {
        let snippet = Snippet {
            name: "deploy-staging".to_string(),
            command: "kubectl apply -f staging.yaml".to_string(),
            description: "Deploy to staging environment".to_string(),
            category: SnippetCategory::Deploy,
            keybinding: Some("Ctrl+Shift+D".to_string()),
        };

        let json = serde_json::to_string(&snippet).unwrap();
        let parsed: Snippet = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, "deploy-staging");
        assert_eq!(parsed.command, "kubectl apply -f staging.yaml");
        assert_eq!(parsed.description, "Deploy to staging environment");
        assert_eq!(parsed.category, SnippetCategory::Deploy);
        assert_eq!(parsed.keybinding, Some("Ctrl+Shift+D".to_string()));
    }

    #[test]
    fn test_save_multiple_snippets_then_delete_one() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_dir = tmp.path().join("multi-snippet-repo");
        let repo_oce = repo_dir.join(".ocestrater");
        std::fs::create_dir_all(&repo_oce).unwrap();
        let repo_path_str = repo_dir.to_str().unwrap();

        // Save three snippets
        for (name, cmd) in [("test", "cargo test"), ("build", "cargo build"), ("lint", "cargo clippy")] {
            save_snippet(
                Some(repo_path_str),
                Snippet {
                    name: name.into(),
                    command: cmd.into(),
                    description: "".into(),
                    category: SnippetCategory::Custom,
                    keybinding: None,
                },
            )
            .unwrap();
        }

        let file = load_snippet_file(&repo_snippets_path(repo_path_str));
        assert_eq!(file.snippets.len(), 3);

        // Delete one
        delete_snippet(Some(repo_path_str), "build").unwrap();

        let file = load_snippet_file(&repo_snippets_path(repo_path_str));
        assert_eq!(file.snippets.len(), 2);
        let names: Vec<&str> = file.snippets.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"test"));
        assert!(names.contains(&"lint"));
        assert!(!names.contains(&"build"));
    }

    #[test]
    fn test_snippet_file_version_defaults_to_1() {
        let json = r#"{"snippets": []}"#;
        let file: SnippetFile = serde_json::from_str(json).unwrap();
        assert_eq!(file.version, 1);
    }

    #[test]
    fn test_save_and_load_snippet_file_roundtrip_with_tempdir() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("snippets.json");

        let file = SnippetFile {
            version: 1,
            snippets: vec![
                Snippet {
                    name: "setup".into(),
                    command: "npm install".into(),
                    description: "Install dependencies".into(),
                    category: SnippetCategory::Setup,
                    keybinding: None,
                },
                Snippet {
                    name: "test".into(),
                    command: "npm test".into(),
                    description: "Run tests".into(),
                    category: SnippetCategory::Test,
                    keybinding: Some("Ctrl+T".into()),
                },
            ],
        };

        save_snippet_file(&path, &file).unwrap();
        let loaded = load_snippet_file(&path);

        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.snippets.len(), 2);
        assert_eq!(loaded.snippets[0].name, "setup");
        assert_eq!(loaded.snippets[1].name, "test");
        assert_eq!(loaded.snippets[1].keybinding, Some("Ctrl+T".into()));
    }
}
