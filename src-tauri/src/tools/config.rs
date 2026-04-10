//! Persistent configuration for managed tool timestamps.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Number of seconds before a tool is considered stale and the user is prompted.
pub(crate) const STALE_THRESHOLD_SECS: u64 = 30 * 24 * 60 * 60; // 30 days

/// Persisted in `tools_config.json` inside the app data directory.
/// Tracks when each managed tool was last downloaded / updated.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolsConfig {
    /// Map of tool name → Unix timestamp (seconds) of last update.
    /// Keys: "platform-tools", "bundletool", "java"
    #[serde(default)]
    pub last_updated: HashMap<String, u64>,
}

pub(crate) fn config_path(data_dir: &PathBuf) -> PathBuf {
    data_dir.join("tools_config.json")
}

pub(crate) fn load_config(data_dir: &PathBuf) -> ToolsConfig {
    let path = config_path(data_dir);
    if !path.exists() {
        return ToolsConfig::default();
    }
    match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => ToolsConfig::default(),
    }
}

pub(crate) fn save_config(data_dir: &PathBuf, config: &ToolsConfig) -> Result<(), String> {
    let path = config_path(data_dir);
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize tools config: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write tools config: {}", e))?;
    Ok(())
}

/// Record the current time as the "last_updated" timestamp for a tool.
pub(crate) fn mark_tool_updated(data_dir: &PathBuf, tool: &str) -> Result<(), String> {
    let mut config = load_config(data_dir);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("Clock error: {}", e))?
        .as_secs();
    config.last_updated.insert(tool.to_string(), now);
    save_config(data_dir, &config)
}

pub(crate) fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "aai_cfg_test_{}_{}", std::process::id(), id
        ));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    fn cleanup(dir: &PathBuf) {
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn stale_threshold_is_30_days() {
        assert_eq!(STALE_THRESHOLD_SECS, 30 * 24 * 60 * 60);
    }

    #[test]
    fn tools_config_default_has_empty_map() {
        let config = ToolsConfig::default();
        assert!(config.last_updated.is_empty());
    }

    #[test]
    fn tools_config_serializes_and_deserializes() {
        let mut config = ToolsConfig::default();
        config.last_updated.insert("platform-tools".to_string(), 1000);
        config.last_updated.insert("bundletool".to_string(), 2000);

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: ToolsConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.last_updated.get("platform-tools"), Some(&1000));
        assert_eq!(deserialized.last_updated.get("bundletool"), Some(&2000));
    }

    #[test]
    fn load_config_returns_default_when_no_file() {
        let dir = temp_dir();
        let config = load_config(&dir);
        assert!(config.last_updated.is_empty());
        cleanup(&dir);
    }

    #[test]
    fn save_and_load_config_round_trip() {
        let dir = temp_dir();
        let mut config = ToolsConfig::default();
        config.last_updated.insert("java".to_string(), 12345);

        save_config(&dir, &config).unwrap();
        let loaded = load_config(&dir);
        assert_eq!(loaded.last_updated.get("java"), Some(&12345));
        cleanup(&dir);
    }

    #[test]
    fn load_config_handles_corrupted_file() {
        let dir = temp_dir();
        fs::write(config_path(&dir), "not valid json {{{").unwrap();
        let config = load_config(&dir);
        assert!(config.last_updated.is_empty());
        cleanup(&dir);
    }

    #[test]
    fn mark_tool_updated_records_timestamp() {
        let dir = temp_dir();
        mark_tool_updated(&dir, "platform-tools").unwrap();
        let config = load_config(&dir);
        let ts = config.last_updated.get("platform-tools").unwrap();
        let now = unix_now();
        assert!(*ts <= now && *ts >= now.saturating_sub(10));
        cleanup(&dir);
    }

    #[test]
    fn mark_tool_updated_multiple_tools() {
        let dir = temp_dir();
        mark_tool_updated(&dir, "platform-tools").unwrap();
        mark_tool_updated(&dir, "bundletool").unwrap();
        mark_tool_updated(&dir, "java").unwrap();

        let config = load_config(&dir);
        assert!(config.last_updated.contains_key("platform-tools"));
        assert!(config.last_updated.contains_key("bundletool"));
        assert!(config.last_updated.contains_key("java"));
        cleanup(&dir);
    }

    #[test]
    fn config_path_ends_with_correct_filename() {
        let dir = PathBuf::from("/test");
        assert_eq!(config_path(&dir), PathBuf::from("/test/tools_config.json"));
    }

    #[test]
    fn unix_now_returns_positive() {
        assert!(unix_now() > 0);
    }

    #[test]
    fn unix_now_is_monotonic() {
        let t1 = unix_now();
        let t2 = unix_now();
        assert!(t2 >= t1);
    }
}
