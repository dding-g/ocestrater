use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Process-level lock to prevent concurrent file access to trust.json
fn trust_file_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: OnceLock<std::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustStore {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub trust_all_repos: bool,
    #[serde(default)]
    pub repos: HashMap<String, TrustEntry>,
}

fn default_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustEntry {
    pub trusted: bool,
    pub trusted_at: String,
    /// Hash of the entire .ocestrater/config.json file (covers all execution-related fields)
    #[serde(default)]
    pub config_hash: Option<String>,
    pub snippets_hash: Option<String>,
    /// Deprecated: only hashed setup_script. Kept for backward-compat deserialization.
    #[serde(default)]
    pub setup_script_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum TrustStatus {
    Trusted,
    Untrusted,
    Changed { changed_files: Vec<String> },
}

fn trust_store_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ocestrater")
        .join("trust.json")
}

fn load_trust_store() -> TrustStore {
    let path = trust_store_path();
    if !path.exists() {
        return TrustStore {
            version: 1,
            trust_all_repos: false,
            repos: HashMap::new(),
        };
    }
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or(TrustStore {
            version: 1,
            trust_all_repos: false,
            repos: HashMap::new(),
        }),
        Err(_) => TrustStore {
            version: 1,
            trust_all_repos: false,
            repos: HashMap::new(),
        },
    }
}

fn save_trust_store(store: &TrustStore) -> Result<(), String> {
    let path = trust_store_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir error: {e}"))?;
    }
    let json =
        serde_json::to_string_pretty(store).map_err(|e| format!("serialize error: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("write error: {e}"))
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Compute the SHA-256 hash of the entire .ocestrater/config.json file.
/// This covers all execution-related fields (setup_script, agent_overrides,
/// snippets, worktree_dir, etc.) so any modification is detected.
fn compute_config_hash(repo_path: &str) -> Option<String> {
    let config_path = Path::new(repo_path).join(".ocestrater/config.json");
    let content = std::fs::read(&config_path).ok()?;
    if content.is_empty() {
        return None;
    }
    Some(sha256_hex(&content))
}

/// Legacy: compute hash of just the setup_script field for backward-compat checks.
fn compute_setup_script_hash_legacy(repo_path: &str) -> Option<String> {
    let config_path = Path::new(repo_path).join(".ocestrater/config.json");
    let content = std::fs::read_to_string(&config_path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;
    let script = parsed.get("setup_script")?.as_str()?;
    Some(sha256_hex(script.as_bytes()))
}

/// Compute the SHA-256 hash of the entire snippets.json file for a repo
fn compute_snippets_hash(repo_path: &str) -> Option<String> {
    let snippets_path = Path::new(repo_path).join(".ocestrater/snippets.json");
    let content = std::fs::read(&snippets_path).ok()?;
    Some(sha256_hex(&content))
}

/// Check trust status for a repo per the algorithm in the spec (section 2.4)
pub fn check_trust(repo_path: &str) -> Result<TrustStatus, String> {
    let _guard = trust_file_lock().lock().map_err(|e| e.to_string())?;
    let store = load_trust_store();

    if store.trust_all_repos {
        return Ok(TrustStatus::Trusted);
    }

    let entry = match store.repos.get(repo_path) {
        Some(e) => e,
        None => return Ok(TrustStatus::Untrusted),
    };

    if !entry.trusted {
        return Ok(TrustStatus::Untrusted);
    }

    // Compare hashes
    let mut changed_files = Vec::new();

    // Check config_hash (covers all execution-related fields in config.json).
    // Fall back to legacy setup_script_hash for entries created before config_hash existed.
    let current_config_hash = compute_config_hash(repo_path);
    if entry.config_hash.is_some() || current_config_hash.is_some() {
        if current_config_hash != entry.config_hash {
            changed_files.push("config.json".to_string());
        }
    } else if entry.setup_script_hash.is_some() {
        // Legacy entry: only had setup_script_hash. Still check it for backward compat.
        let current_setup_hash = compute_setup_script_hash_legacy(repo_path);
        if current_setup_hash != entry.setup_script_hash {
            if current_setup_hash.is_some() || entry.setup_script_hash.is_some() {
                changed_files.push("config.json".to_string());
            }
        }
    }

    let current_snippets_hash = compute_snippets_hash(repo_path);
    if current_snippets_hash != entry.snippets_hash {
        if current_snippets_hash.is_some() || entry.snippets_hash.is_some() {
            changed_files.push("snippets.json".to_string());
        }
    }

    if !changed_files.is_empty() {
        return Ok(TrustStatus::Changed { changed_files });
    }

    Ok(TrustStatus::Trusted)
}

/// Grant trust for a repo: compute current hashes and store
pub fn grant_trust(repo_path: &str) -> Result<(), String> {
    let _guard = trust_file_lock().lock().map_err(|e| e.to_string())?;
    let mut store = load_trust_store();

    let now = chrono_iso8601_now();
    let config_hash = compute_config_hash(repo_path);
    let snippets_hash = compute_snippets_hash(repo_path);

    store.repos.insert(
        repo_path.to_string(),
        TrustEntry {
            trusted: true,
            trusted_at: now,
            config_hash,
            snippets_hash,
            setup_script_hash: None, // deprecated; config_hash covers everything
        },
    );

    save_trust_store(&store)
}

/// Revoke trust for a repo
pub fn revoke_trust(repo_path: &str) -> Result<(), String> {
    let _guard = trust_file_lock().lock().map_err(|e| e.to_string())?;
    let mut store = load_trust_store();

    if let Some(entry) = store.repos.get_mut(repo_path) {
        entry.trusted = false;
    }

    save_trust_store(&store)
}

/// Simple ISO 8601 UTC timestamp without pulling in a full chrono dependency
fn chrono_iso8601_now() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    // Format as basic ISO 8601 with seconds precision
    // We compute year/month/day/hour/min/sec from epoch manually
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Compute date from days since epoch (1970-01-01)
    let (year, month, day) = days_to_ymd(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z"
    )
}

fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Algorithm adapted from Howard Hinnant's civil_from_days
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year as u64, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_hex() {
        let hash = sha256_hex(b"hello world");
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_trust_status_serde_trusted() {
        let status = TrustStatus::Trusted;
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"type\":\"trusted\""));
    }

    #[test]
    fn test_trust_status_serde_untrusted() {
        let status = TrustStatus::Untrusted;
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"type\":\"untrusted\""));
    }

    #[test]
    fn test_trust_status_serde_changed() {
        let status = TrustStatus::Changed {
            changed_files: vec!["setup_script".to_string()],
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"type\":\"changed\""));
        assert!(json.contains("setup_script"));
    }

    #[test]
    fn test_trust_entry_roundtrip() {
        let entry = TrustEntry {
            trusted: true,
            trusted_at: "2026-02-07T12:00:00Z".into(),
            config_hash: Some("abc123".into()),
            snippets_hash: None,
            setup_script_hash: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: TrustEntry = serde_json::from_str(&json).unwrap();
        assert!(parsed.trusted);
        assert_eq!(parsed.config_hash, Some("abc123".into()));
        assert!(parsed.snippets_hash.is_none());
    }

    #[test]
    fn test_trust_store_roundtrip() {
        let mut repos = HashMap::new();
        repos.insert(
            "/my/repo".to_string(),
            TrustEntry {
                trusted: true,
                trusted_at: "2026-01-01T00:00:00Z".into(),
                config_hash: None,
                snippets_hash: None,
                setup_script_hash: None,
            },
        );
        let store = TrustStore {
            version: 1,
            trust_all_repos: false,
            repos,
        };
        let json = serde_json::to_string(&store).unwrap();
        let parsed: TrustStore = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.repos.len(), 1);
        assert!(parsed.repos.contains_key("/my/repo"));
    }

    #[test]
    fn test_days_to_ymd_epoch() {
        let (y, m, d) = days_to_ymd(0);
        assert_eq!((y, m, d), (1970, 1, 1));
    }

    #[test]
    fn test_days_to_ymd_known_date() {
        // 2026-02-07 is day 20491 since epoch (1970-01-01 = day 0)
        let (y, m, d) = days_to_ymd(20491);
        assert_eq!((y, m, d), (2026, 2, 7));
    }

    #[test]
    fn test_chrono_iso8601_now_format() {
        let ts = chrono_iso8601_now();
        // Should match YYYY-MM-DDTHH:MM:SSZ
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.len(), 20);
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], "T");
        assert_eq!(&ts[13..14], ":");
        assert_eq!(&ts[16..17], ":");
    }

    // ── Phase 3 additional tests ──

    #[test]
    fn test_trust_store_default_construction() {
        let store = TrustStore {
            version: 1,
            trust_all_repos: false,
            repos: HashMap::new(),
        };
        assert_eq!(store.version, 1);
        assert!(!store.trust_all_repos);
        assert!(store.repos.is_empty());
    }

    #[test]
    fn test_trust_store_serde_defaults() {
        // When deserializing with only version, trust_all_repos defaults to false
        // and repos defaults to empty
        let json = r#"{"version": 1}"#;
        let store: TrustStore = serde_json::from_str(json).unwrap();
        assert_eq!(store.version, 1);
        assert!(!store.trust_all_repos);
        assert!(store.repos.is_empty());
    }

    #[test]
    fn test_trust_store_trust_all_repos_serde() {
        let store = TrustStore {
            version: 1,
            trust_all_repos: true,
            repos: HashMap::new(),
        };
        let json = serde_json::to_string(&store).unwrap();
        let parsed: TrustStore = serde_json::from_str(&json).unwrap();
        assert!(parsed.trust_all_repos);
    }

    #[test]
    fn test_days_to_ymd_leap_year_feb_29() {
        // 2000-02-29 is a leap year date
        // Days from 1970-01-01 to 2000-02-29:
        // 1970 to 2000 = 30 years
        // From 1970-01-01 to 2000-01-01 = 10957 days
        // Jan 2000 = 31 days, Feb 1-29 = 29 days -> 31 + 29 - 1 = 59
        // Total = 10957 + 59 = 11016
        let (y, m, d) = days_to_ymd(11016);
        assert_eq!((y, m, d), (2000, 2, 29));
    }

    #[test]
    fn test_days_to_ymd_leap_year_mar_1() {
        // 2000-03-01 = day 11017
        let (y, m, d) = days_to_ymd(11017);
        assert_eq!((y, m, d), (2000, 3, 1));
    }

    #[test]
    fn test_days_to_ymd_non_leap_year() {
        // 2023 is NOT a leap year. Verify Feb 28 and Mar 1 are consecutive.
        // Empirically from the algorithm: day 19416 = 2023-02-28
        let (y, m, d) = days_to_ymd(19416);
        assert_eq!((y, m, d), (2023, 2, 28));
        // Day 19417 should be 2023-03-01 (no Feb 29 in non-leap year)
        let (y2, m2, d2) = days_to_ymd(19417);
        assert_eq!((y2, m2, d2), (2023, 3, 1));
    }

    #[test]
    fn test_days_to_ymd_2024_leap_year_feb_29() {
        // 2024 is a leap year
        // 1970-01-01 to 2024-01-01:
        // 54 years, leap years: 1972..2024 step 4, minus century non-leaps
        // 14 leap years (1972,1976,1980,1984,1988,1992,1996,2000,2004,2008,2012,2016,2020,2024)
        // Wait, 2024-01-01 hasn't passed the leap day yet, so count leaps before 2024
        // Leap years with Feb 29 between 1970 and end of 2023: 13 (1972..2020)
        // 54*365 + 13 = 19710 + 13 = 19723
        // Actually let's just compute from a known point:
        // 2026-02-07 = day 20491 (from existing test)
        // 2024-02-29 = 20491 - (365 + 31 + 7) - (365 + 31 + 28 + ... )
        // Let's just verify via calculation:
        // 2024-01-01 = day 19723
        // + 31 (Jan) + 29 (Feb 1-29) - 1 = +59
        // 2024-02-29 = day 19782
        let (y, m, d) = days_to_ymd(19782);
        assert_eq!((y, m, d), (2024, 2, 29));
    }

    #[test]
    fn test_days_to_ymd_end_of_year() {
        // 2025-12-31
        // 2026-02-07 = day 20491
        // 2025-12-31 = 20491 - 31 - 7 = 20491 - 38 = 20453
        let (y, m, d) = days_to_ymd(20453);
        assert_eq!((y, m, d), (2025, 12, 31));
    }

    #[test]
    fn test_trust_status_changed_roundtrip() {
        let status = TrustStatus::Changed {
            changed_files: vec!["setup_script".to_string(), "snippets.json".to_string()],
        };
        let json = serde_json::to_string(&status).unwrap();
        let parsed: TrustStatus = serde_json::from_str(&json).unwrap();
        match parsed {
            TrustStatus::Changed { changed_files } => {
                assert_eq!(changed_files.len(), 2);
                assert!(changed_files.contains(&"setup_script".to_string()));
                assert!(changed_files.contains(&"snippets.json".to_string()));
            }
            _ => panic!("Expected Changed variant"),
        }
    }

    #[test]
    fn test_trust_status_trusted_roundtrip() {
        let json = serde_json::to_string(&TrustStatus::Trusted).unwrap();
        let parsed: TrustStatus = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, TrustStatus::Trusted));
    }

    #[test]
    fn test_trust_status_untrusted_roundtrip() {
        let json = serde_json::to_string(&TrustStatus::Untrusted).unwrap();
        let parsed: TrustStatus = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, TrustStatus::Untrusted));
    }

    #[test]
    fn test_sha256_hex_empty_input() {
        let hash = sha256_hex(b"");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_sha256_hex_deterministic() {
        let hash1 = sha256_hex(b"test data");
        let hash2 = sha256_hex(b"test data");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_trust_entry_with_all_fields() {
        let entry = TrustEntry {
            trusted: true,
            trusted_at: "2026-01-15T10:30:00Z".into(),
            config_hash: Some("abc123".into()),
            snippets_hash: Some("def456".into()),
            setup_script_hash: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: TrustEntry = serde_json::from_str(&json).unwrap();
        assert!(parsed.trusted);
        assert_eq!(parsed.trusted_at, "2026-01-15T10:30:00Z");
        assert_eq!(parsed.config_hash, Some("abc123".into()));
        assert_eq!(parsed.snippets_hash, Some("def456".into()));
    }

    #[test]
    fn test_trust_store_multiple_repos() {
        let mut repos = HashMap::new();
        repos.insert(
            "/repo/a".to_string(),
            TrustEntry {
                trusted: true,
                trusted_at: "2026-01-01T00:00:00Z".into(),
                config_hash: None,
                snippets_hash: None,
                setup_script_hash: None,
            },
        );
        repos.insert(
            "/repo/b".to_string(),
            TrustEntry {
                trusted: false,
                trusted_at: "2026-01-02T00:00:00Z".into(),
                config_hash: Some("config_hash_value".into()),
                snippets_hash: None,
                setup_script_hash: None,
            },
        );
        let store = TrustStore {
            version: 1,
            trust_all_repos: false,
            repos,
        };
        let json = serde_json::to_string(&store).unwrap();
        let parsed: TrustStore = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.repos.len(), 2);
        assert!(parsed.repos.get("/repo/a").unwrap().trusted);
        assert!(!parsed.repos.get("/repo/b").unwrap().trusted);
    }

    // ── Additional edge case tests ──

    #[test]
    fn test_check_trust_for_repo_not_in_store_returns_untrusted() {
        // check_trust reads from the global trust store file.
        // With a nonexistent repo path that is guaranteed not to be in the store,
        // we can verify it returns Untrusted.
        let result = check_trust("/tmp/ocestrater-test-nonexistent-repo-for-trust-check-9999");
        assert!(result.is_ok());
        let status = result.unwrap();
        assert!(matches!(status, TrustStatus::Untrusted));
    }

    #[test]
    fn test_trust_entry_with_all_none_optional_fields() {
        let entry = TrustEntry {
            trusted: false,
            trusted_at: "2026-06-01T00:00:00Z".into(),
            config_hash: None,
            snippets_hash: None,
            setup_script_hash: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: TrustEntry = serde_json::from_str(&json).unwrap();
        assert!(!parsed.trusted);
        assert_eq!(parsed.trusted_at, "2026-06-01T00:00:00Z");
        assert!(parsed.setup_script_hash.is_none());
        assert!(parsed.snippets_hash.is_none());
    }

    #[test]
    fn test_trust_store_save_load_cycle_with_tempdir() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("trust.json");

        let mut repos = HashMap::new();
        repos.insert(
            "/test/repo".to_string(),
            TrustEntry {
                trusted: true,
                trusted_at: "2026-03-15T10:00:00Z".into(),
                config_hash: Some("abc123".into()),
                snippets_hash: Some("def456".into()),
                setup_script_hash: None,
            },
        );
        repos.insert(
            "/another/repo".to_string(),
            TrustEntry {
                trusted: false,
                trusted_at: "2026-03-16T12:00:00Z".into(),
                config_hash: None,
                snippets_hash: None,
                setup_script_hash: None,
            },
        );

        let store = TrustStore {
            version: 1,
            trust_all_repos: false,
            repos,
        };

        // Save
        let json = serde_json::to_string_pretty(&store).unwrap();
        std::fs::write(&path, &json).unwrap();

        // Load
        let content = std::fs::read_to_string(&path).unwrap();
        let loaded: TrustStore = serde_json::from_str(&content).unwrap();

        assert_eq!(loaded.version, 1);
        assert!(!loaded.trust_all_repos);
        assert_eq!(loaded.repos.len(), 2);

        let test_entry = loaded.repos.get("/test/repo").unwrap();
        assert!(test_entry.trusted);
        assert_eq!(test_entry.trusted_at, "2026-03-15T10:00:00Z");
        assert_eq!(test_entry.config_hash, Some("abc123".into()));
        assert_eq!(test_entry.snippets_hash, Some("def456".into()));

        let another_entry = loaded.repos.get("/another/repo").unwrap();
        assert!(!another_entry.trusted);
        assert!(another_entry.config_hash.is_none());
    }

    #[test]
    fn test_trust_store_with_trust_all_repos_true() {
        let store = TrustStore {
            version: 1,
            trust_all_repos: true,
            repos: HashMap::new(),
        };
        let json = serde_json::to_string(&store).unwrap();
        let parsed: TrustStore = serde_json::from_str(&json).unwrap();
        assert!(parsed.trust_all_repos);
        assert!(parsed.repos.is_empty());
    }

    #[test]
    fn test_trust_store_empty_json_uses_defaults() {
        let json = "{}";
        let store: TrustStore = serde_json::from_str(json).unwrap();
        assert_eq!(store.version, 1);
        assert!(!store.trust_all_repos);
        assert!(store.repos.is_empty());
    }

    #[test]
    fn test_sha256_hex_different_inputs_different_hashes() {
        let hash1 = sha256_hex(b"input one");
        let hash2 = sha256_hex(b"input two");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_trust_status_changed_empty_files_list() {
        let status = TrustStatus::Changed {
            changed_files: vec![],
        };
        let json = serde_json::to_string(&status).unwrap();
        let parsed: TrustStatus = serde_json::from_str(&json).unwrap();
        match parsed {
            TrustStatus::Changed { changed_files } => {
                assert!(changed_files.is_empty());
            }
            _ => panic!("Expected Changed variant"),
        }
    }
}
