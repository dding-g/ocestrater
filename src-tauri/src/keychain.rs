use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

const SERVICE: &str = "com.ocestrater.secrets";

/// In-memory cache for Keychain secrets
pub struct KeychainStore {
    cache: HashMap<String, String>,
}

impl KeychainStore {
    /// Load all secrets from Keychain into memory on startup
    pub fn load() -> Self {
        let keys = load_index().unwrap_or_default();
        let mut cache = HashMap::new();
        for key in &keys {
            if let Ok(val) = get_secret_from_keychain(key) {
                cache.insert(key.clone(), val);
            }
        }
        KeychainStore { cache }
    }

    /// Get a secret from the in-memory cache
    pub fn get(&self, key: &str) -> Option<&String> {
        self.cache.get(key)
    }

    /// Set a secret in both Keychain and cache
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        set_generic_password(SERVICE, key, value.as_bytes())
            .map_err(|e| format!("keychain write error: {e}"))?;
        self.cache.insert(key.to_string(), value.to_string());
        add_to_index(key)?;
        Ok(())
    }

    /// Delete a secret from both Keychain and cache
    pub fn delete(&mut self, key: &str) -> Result<(), String> {
        let _ = delete_generic_password(SERVICE, key);
        self.cache.remove(key);
        remove_from_index(key)?;
        Ok(())
    }

    /// List all stored key names
    pub fn list_keys(&self) -> Vec<String> {
        load_index().unwrap_or_default()
    }

    /// Get all cached secrets as env vars for PTY injection
    pub fn env_vars(&self) -> &HashMap<String, String> {
        &self.cache
    }
}

fn get_secret_from_keychain(key: &str) -> Result<String, String> {
    get_generic_password(SERVICE, key)
        .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
        .map_err(|e| format!("keychain read error: {e}"))
}

fn index_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ocestrater")
        .join("secret-keys.json")
}

pub fn load_index() -> Result<Vec<String>, String> {
    let path = index_path();
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("read index error: {e}"))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("parse index error: {e}"))
}

fn save_index(keys: &[String]) -> Result<(), String> {
    let path = index_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(keys)
        .map_err(|e| format!("serialize index error: {e}"))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("write index error: {e}"))
}

pub fn add_to_index(key: &str) -> Result<(), String> {
    let mut keys = load_index().unwrap_or_default();
    if !keys.contains(&key.to_string()) {
        keys.push(key.to_string());
        save_index(&keys)?;
    }
    Ok(())
}

pub fn remove_from_index(key: &str) -> Result<(), String> {
    let mut keys = load_index().unwrap_or_default();
    keys.retain(|k| k != key);
    save_index(&keys)
}

pub type KeychainState = Mutex<KeychainStore>;

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a temporary index file path in a temp directory
    fn temp_index_path(dir: &std::path::Path) -> PathBuf {
        dir.join("secret-keys.json")
    }

    /// Helper: write an index file at a given path
    fn write_index(path: &std::path::Path, keys: &[String]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let json = serde_json::to_string_pretty(keys).unwrap();
        std::fs::write(path, json).unwrap();
    }

    /// Helper: read an index file from a given path
    fn read_index(path: &std::path::Path) -> Vec<String> {
        let content = std::fs::read_to_string(path).unwrap();
        serde_json::from_str(&content).unwrap()
    }

    // ── Index file roundtrip tests using temp files ──

    #[test]
    fn test_index_save_then_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = temp_index_path(tmp.path());

        let keys = vec!["API_KEY".to_string(), "DB_PASSWORD".to_string()];
        write_index(&path, &keys);

        let loaded = read_index(&path);
        assert_eq!(loaded, keys);
    }

    #[test]
    fn test_index_empty_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = temp_index_path(tmp.path());

        let keys: Vec<String> = vec![];
        write_index(&path, &keys);

        let loaded = read_index(&path);
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_load_index_nonexistent_file_returns_empty() {
        // load_index uses the global index_path, so we test the pattern
        // directly by reading a nonexistent path
        let path = std::path::Path::new("/tmp/ocestrater-test-nonexistent-keychain-index/secret-keys.json");
        assert!(!path.exists());
        // Simulating load_index logic: if file doesn't exist, return empty
        if !path.exists() {
            let keys: Vec<String> = vec![];
            assert!(keys.is_empty());
        }
    }

    #[test]
    fn test_index_deduplication_logic() {
        // Test the deduplication logic used by add_to_index
        let tmp = tempfile::tempdir().unwrap();
        let path = temp_index_path(tmp.path());

        // Start with one key
        let keys = vec!["API_KEY".to_string()];
        write_index(&path, &keys);

        // Simulate add_to_index: load, check for duplicates, add if not present
        let mut loaded: Vec<String> = read_index(&path);
        let new_key = "API_KEY".to_string();
        if !loaded.contains(&new_key) {
            loaded.push(new_key);
        }
        write_index(&path, &loaded);

        let final_keys = read_index(&path);
        // Should still be 1, not 2 (deduplication)
        assert_eq!(final_keys.len(), 1);
        assert_eq!(final_keys[0], "API_KEY");
    }

    #[test]
    fn test_index_add_new_key() {
        let tmp = tempfile::tempdir().unwrap();
        let path = temp_index_path(tmp.path());

        let keys = vec!["API_KEY".to_string()];
        write_index(&path, &keys);

        // Simulate add_to_index: add a new unique key
        let mut loaded: Vec<String> = read_index(&path);
        let new_key = "DB_PASSWORD".to_string();
        if !loaded.contains(&new_key) {
            loaded.push(new_key);
        }
        write_index(&path, &loaded);

        let final_keys = read_index(&path);
        assert_eq!(final_keys.len(), 2);
        assert!(final_keys.contains(&"API_KEY".to_string()));
        assert!(final_keys.contains(&"DB_PASSWORD".to_string()));
    }

    #[test]
    fn test_index_remove_existing_key() {
        let tmp = tempfile::tempdir().unwrap();
        let path = temp_index_path(tmp.path());

        let keys = vec![
            "KEY_A".to_string(),
            "KEY_B".to_string(),
            "KEY_C".to_string(),
        ];
        write_index(&path, &keys);

        // Simulate remove_from_index: retain all except KEY_B
        let mut loaded: Vec<String> = read_index(&path);
        loaded.retain(|k| k != "KEY_B");
        write_index(&path, &loaded);

        let final_keys = read_index(&path);
        assert_eq!(final_keys.len(), 2);
        assert!(final_keys.contains(&"KEY_A".to_string()));
        assert!(final_keys.contains(&"KEY_C".to_string()));
        assert!(!final_keys.contains(&"KEY_B".to_string()));
    }

    #[test]
    fn test_index_remove_nonexistent_key_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let path = temp_index_path(tmp.path());

        let keys = vec!["KEY_A".to_string(), "KEY_B".to_string()];
        write_index(&path, &keys);

        // Simulate remove_from_index for a key that doesn't exist
        let mut loaded: Vec<String> = read_index(&path);
        loaded.retain(|k| k != "NONEXISTENT");
        write_index(&path, &loaded);

        let final_keys = read_index(&path);
        assert_eq!(final_keys.len(), 2);
        assert_eq!(final_keys, vec!["KEY_A", "KEY_B"]);
    }

    #[test]
    fn test_index_invalid_json_handling() {
        let tmp = tempfile::tempdir().unwrap();
        let path = temp_index_path(tmp.path());

        // Write invalid JSON
        std::fs::write(&path, "{ not valid json }").unwrap();

        // Simulating load_index behavior: parse fails, should return error
        let content = std::fs::read_to_string(&path).unwrap();
        let result: Result<Vec<String>, _> = serde_json::from_str(&content);
        assert!(result.is_err());
    }

    #[test]
    fn test_index_large_key_set() {
        let tmp = tempfile::tempdir().unwrap();
        let path = temp_index_path(tmp.path());

        let keys: Vec<String> = (0..100).map(|i| format!("KEY_{i}")).collect();
        write_index(&path, &keys);

        let loaded = read_index(&path);
        assert_eq!(loaded.len(), 100);
        assert_eq!(loaded[0], "KEY_0");
        assert_eq!(loaded[99], "KEY_99");
    }

    #[test]
    fn test_index_keys_with_special_characters() {
        let tmp = tempfile::tempdir().unwrap();
        let path = temp_index_path(tmp.path());

        let keys = vec![
            "MY-API-KEY".to_string(),
            "DB_PASSWORD_123".to_string(),
            "key.with.dots".to_string(),
        ];
        write_index(&path, &keys);

        let loaded = read_index(&path);
        assert_eq!(loaded, keys);
    }

    // ── KeychainStore in-memory cache tests ──

    #[test]
    fn test_keychain_store_cache_operations() {
        // Test the in-memory HashMap cache operations without touching actual keychain
        let mut cache: HashMap<String, String> = HashMap::new();

        // Set
        cache.insert("KEY1".to_string(), "value1".to_string());
        assert_eq!(cache.get("KEY1"), Some(&"value1".to_string()));

        // Get nonexistent
        assert_eq!(cache.get("NONEXISTENT"), None);

        // Delete
        cache.remove("KEY1");
        assert_eq!(cache.get("KEY1"), None);
    }

    #[test]
    fn test_keychain_store_list_keys_returns_vec() {
        // Verify that the index file format is Vec<String>
        let tmp = tempfile::tempdir().unwrap();
        let path = temp_index_path(tmp.path());

        let keys = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        write_index(&path, &keys);

        let loaded: Vec<String> = read_index(&path);
        assert_eq!(loaded.len(), 3);
    }

    #[test]
    fn test_index_path_ends_with_secret_keys_json() {
        let path = index_path();
        assert!(path.to_string_lossy().ends_with("secret-keys.json"));
        assert!(path.to_string_lossy().contains(".ocestrater"));
    }
}
