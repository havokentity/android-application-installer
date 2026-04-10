//! Recent files management: track recently used APK/AAB files and keystores.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tauri::AppHandle;

use super::config::unix_now;
use super::paths::get_data_dir;

// ─── Constants & Types ───────────────────────────────────────────────────────

const MAX_RECENT_FILES: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentFile {
    pub path: String,
    pub name: String,
    pub last_used: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecentFilesConfig {
    #[serde(default)]
    pub packages: Vec<RecentFile>,
    #[serde(default)]
    pub keystores: Vec<RecentFile>,
}

// ─── Persistence ─────────────────────────────────────────────────────────────

fn recent_files_path(data_dir: &Path) -> PathBuf {
    data_dir.join("recent_files.json")
}

fn load_recent_files(data_dir: &Path) -> RecentFilesConfig {
    let path = recent_files_path(data_dir);
    if !path.exists() {
        return RecentFilesConfig::default();
    }
    match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => RecentFilesConfig::default(),
    }
}

fn save_recent_files(data_dir: &Path, config: &RecentFilesConfig) -> Result<(), String> {
    let path = recent_files_path(data_dir);
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize recent files: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write recent files: {}", e))?;
    Ok(())
}

/// Prune entries whose files no longer exist on disk, deduplicate by path,
/// sort by most-recently-used, and cap at MAX_RECENT_FILES.
fn prune_list(list: &mut Vec<RecentFile>) {
    // Remove entries for files that no longer exist
    list.retain(|f| Path::new(&f.path).exists());
    // Deduplicate by path (keep the one with the latest last_used)
    let mut seen = HashMap::new();
    for entry in list.iter() {
        let existing = seen.entry(entry.path.clone()).or_insert(entry.clone());
        if entry.last_used > existing.last_used {
            *existing = entry.clone();
        }
    }
    *list = seen.into_values().collect();
    // Sort by most recently used
    list.sort_by(|a, b| b.last_used.cmp(&a.last_used));
    // Cap at max
    list.truncate(MAX_RECENT_FILES);
}

// ─── Tauri Commands ──────────────────────────────────────────────────────────

/// Get the recent files lists, pruning stale entries.
#[tauri::command]
pub fn get_recent_files(app: AppHandle) -> Result<RecentFilesConfig, String> {
    let data_dir = get_data_dir(&app)?;
    let mut config = load_recent_files(&data_dir);
    prune_list(&mut config.packages);
    prune_list(&mut config.keystores);
    // Auto-save pruned state
    let _ = save_recent_files(&data_dir, &config);
    Ok(config)
}

/// Add (or bump) a file in the recent list for the given category.
/// category: "packages" or "keystores"
#[tauri::command]
pub fn add_recent_file(app: AppHandle, path: String, category: String) -> Result<RecentFilesConfig, String> {
    let data_dir = get_data_dir(&app)?;
    let mut config = load_recent_files(&data_dir);

    let name = Path::new(&path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let entry = RecentFile {
        path,
        name,
        last_used: unix_now(),
    };

    let list = match category.as_str() {
        "packages" => &mut config.packages,
        "keystores" => &mut config.keystores,
        _ => return Err(format!("Unknown category: {}", category)),
    };

    list.push(entry);
    prune_list(list);
    save_recent_files(&data_dir, &config)?;
    Ok(config)
}

/// Remove a specific file from the recent list.
#[tauri::command]
pub fn remove_recent_file(app: AppHandle, path: String, category: String) -> Result<RecentFilesConfig, String> {
    let data_dir = get_data_dir(&app)?;
    let mut config = load_recent_files(&data_dir);

    let list = match category.as_str() {
        "packages" => &mut config.packages,
        "keystores" => &mut config.keystores,
        _ => return Err(format!("Unknown category: {}", category)),
    };

    list.retain(|f| f.path != path);
    save_recent_files(&data_dir, &config)?;
    Ok(config)
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
            "aai_recent_test_{}_{}", std::process::id(), id
        ));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    fn cleanup(dir: &PathBuf) {
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn max_recent_files_is_ten() {
        assert_eq!(MAX_RECENT_FILES, 10);
    }

    #[test]
    fn recent_file_serializes() {
        let rf = RecentFile {
            path: "/path/to/app.apk".to_string(),
            name: "app.apk".to_string(),
            last_used: 999,
        };
        let json = serde_json::to_string(&rf).unwrap();
        assert!(json.contains("app.apk"));
        assert!(json.contains("999"));
    }

    #[test]
    fn recent_files_config_default_is_empty() {
        let config = RecentFilesConfig::default();
        assert!(config.packages.is_empty());
        assert!(config.keystores.is_empty());
    }

    #[test]
    fn save_and_load_recent_files() {
        let dir = temp_dir();
        let config = RecentFilesConfig {
            packages: vec![RecentFile {
                path: "/test.apk".to_string(),
                name: "test.apk".to_string(),
                last_used: 100,
            }],
            keystores: vec![],
        };

        save_recent_files(&dir, &config).unwrap();
        let loaded = load_recent_files(&dir);
        assert_eq!(loaded.packages.len(), 1);
        assert_eq!(loaded.packages[0].path, "/test.apk");
        cleanup(&dir);
    }

    #[test]
    fn load_recent_files_returns_default_when_missing() {
        let dir = temp_dir();
        let config = load_recent_files(&dir);
        assert!(config.packages.is_empty());
        assert!(config.keystores.is_empty());
        cleanup(&dir);
    }

    #[test]
    fn load_recent_files_handles_corrupt_json() {
        let dir = temp_dir();
        fs::write(recent_files_path(&dir), "corrupt!!!").unwrap();
        let config = load_recent_files(&dir);
        assert!(config.packages.is_empty());
        cleanup(&dir);
    }

    #[test]
    fn recent_files_path_ends_with_correct_filename() {
        let dir = PathBuf::from("/test");
        assert_eq!(recent_files_path(&dir), PathBuf::from("/test/recent_files.json"));
    }

    #[test]
    fn prune_list_removes_nonexistent_files() {
        let mut list = vec![
            RecentFile {
                path: "/definitely/does/not/exist/app.apk".to_string(),
                name: "app.apk".to_string(),
                last_used: 100,
            },
        ];
        prune_list(&mut list);
        assert!(list.is_empty());
    }

    #[test]
    fn prune_list_keeps_existing_files() {
        let tmp = std::env::temp_dir().join("prune_test_file.tmp");
        fs::write(&tmp, "test").unwrap();

        let mut list = vec![
            RecentFile {
                path: tmp.to_string_lossy().to_string(),
                name: "prune_test_file.tmp".to_string(),
                last_used: 100,
            },
        ];
        prune_list(&mut list);
        assert_eq!(list.len(), 1);

        let _ = fs::remove_file(&tmp);
    }

    #[test]
    fn prune_list_deduplicates_by_path() {
        let tmp = std::env::temp_dir().join("dedup_test.tmp");
        fs::write(&tmp, "test").unwrap();
        let path = tmp.to_string_lossy().to_string();

        let mut list = vec![
            RecentFile { path: path.clone(), name: "dedup_test.tmp".to_string(), last_used: 100 },
            RecentFile { path: path.clone(), name: "dedup_test.tmp".to_string(), last_used: 200 },
        ];
        prune_list(&mut list);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].last_used, 200);

        let _ = fs::remove_file(&tmp);
    }

    #[test]
    fn prune_list_sorts_by_most_recent() {
        let tmp1 = std::env::temp_dir().join("sort_test1.tmp");
        let tmp2 = std::env::temp_dir().join("sort_test2.tmp");
        fs::write(&tmp1, "a").unwrap();
        fs::write(&tmp2, "b").unwrap();

        let mut list = vec![
            RecentFile { path: tmp1.to_string_lossy().to_string(), name: "sort_test1.tmp".to_string(), last_used: 100 },
            RecentFile { path: tmp2.to_string_lossy().to_string(), name: "sort_test2.tmp".to_string(), last_used: 200 },
        ];
        prune_list(&mut list);
        assert_eq!(list[0].last_used, 200);
        assert_eq!(list[1].last_used, 100);

        let _ = fs::remove_file(&tmp1);
        let _ = fs::remove_file(&tmp2);
    }

    #[test]
    fn prune_list_caps_at_max_recent_files() {
        let mut tmpfiles = Vec::new();
        let mut list = Vec::new();

        for i in 0..15 {
            let tmp = std::env::temp_dir().join(format!("cap_test_{}.tmp", i));
            fs::write(&tmp, "x").unwrap();
            list.push(RecentFile {
                path: tmp.to_string_lossy().to_string(),
                name: format!("cap_test_{}.tmp", i),
                last_used: i as u64,
            });
            tmpfiles.push(tmp);
        }

        prune_list(&mut list);
        assert!(list.len() <= MAX_RECENT_FILES);

        for f in &tmpfiles {
            let _ = fs::remove_file(f);
        }
    }

    #[test]
    fn prune_list_empty_list() {
        let mut list: Vec<RecentFile> = vec![];
        prune_list(&mut list);
        assert!(list.is_empty());
    }
}
