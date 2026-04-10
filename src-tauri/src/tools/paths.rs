//! Path helpers for managed tool binaries.

use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// Returns the app-managed path where ADB would live.
pub fn managed_adb_path(data_dir: &PathBuf) -> PathBuf {
    data_dir
        .join("platform-tools")
        .join(crate::cmd::adb_binary())
}

/// Returns the app-managed path where bundletool.jar would live.
pub fn managed_bundletool_path(data_dir: &PathBuf) -> PathBuf {
    data_dir.join("bundletool.jar")
}

/// Returns the app-managed Java binary path, if a JRE has been downloaded.
/// Scans the `jre/` subdirectory for extracted Adoptium JRE layouts:
///   macOS:   jre/<jdk-dir>/Contents/Home/bin/java
///   Linux:   jre/<jdk-dir>/bin/java
///   Windows: jre/<jdk-dir>/bin/java.exe
pub fn managed_java_path(data_dir: &PathBuf) -> Option<PathBuf> {
    let jre_dir = data_dir.join("jre");
    if !jre_dir.exists() {
        return None;
    }

    let bin = crate::cmd::java_binary();

    if let Ok(entries) = fs::read_dir(&jre_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            // macOS layout: Contents/Home/bin/java
            let mac_path = path
                .join("Contents")
                .join("Home")
                .join("bin")
                .join(bin);
            if mac_path.exists() {
                return Some(mac_path);
            }

            // Linux / Windows layout: bin/java[.exe]
            let other_path = path.join("bin").join(bin);
            if other_path.exists() {
                return Some(other_path);
            }
        }
    }

    None
}

/// Get the app's local data directory from an AppHandle.
pub fn get_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map_err(|e| format!("Failed to resolve app data directory: {}", e))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_adb_path_is_under_data_dir() {
        let dir = PathBuf::from("/test/data");
        let path = managed_adb_path(&dir);
        assert!(path.starts_with("/test/data/platform-tools"));
        assert!(path.to_string_lossy().contains(crate::cmd::adb_binary()));
    }

    #[test]
    fn managed_bundletool_path_is_under_data_dir() {
        let dir = PathBuf::from("/test/data");
        let path = managed_bundletool_path(&dir);
        assert_eq!(path, PathBuf::from("/test/data/bundletool.jar"));
    }

    #[test]
    fn managed_java_path_returns_none_when_no_jre() {
        let dir = std::env::temp_dir().join("aai_path_test_no_jre");
        let _ = fs::create_dir_all(&dir);
        let result = managed_java_path(&dir);
        assert!(result.is_none());
        let _ = fs::remove_dir_all(&dir);
    }
}
