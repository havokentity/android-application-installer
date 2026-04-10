//! Tool status checks and staleness detection.

use serde::Serialize;
use tauri::AppHandle;

use super::config::{load_config, unix_now, STALE_THRESHOLD_SECS};
use super::paths::{get_data_dir, managed_adb_path, managed_bundletool_path, managed_java_path};

// ─── Data Types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ToolsStatus {
    pub adb_installed: bool,
    pub adb_path: String,
    pub bundletool_installed: bool,
    pub bundletool_path: String,
    pub java_installed: bool,
    pub java_path: String,
    pub data_dir: String,
}

/// Returned by `check_for_stale_tools`.  Each entry describes one tool that
/// hasn't been updated within the threshold.
#[derive(Debug, Clone, Serialize)]
pub struct StaleTool {
    pub tool: String,
    pub label: String,
    pub last_updated_secs: u64,
    pub age_days: u64,
}

// ─── Tauri Commands ──────────────────────────────────────────────────────────

/// Check whether managed ADB and bundletool are installed.
#[tauri::command]
pub fn get_tools_status(app: AppHandle) -> Result<ToolsStatus, String> {
    let data_dir = get_data_dir(&app)?;

    let adb = managed_adb_path(&data_dir);
    let bt = managed_bundletool_path(&data_dir);
    let java = managed_java_path(&data_dir);

    Ok(ToolsStatus {
        adb_installed: adb.exists(),
        adb_path: adb.to_string_lossy().to_string(),
        bundletool_installed: bt.exists(),
        bundletool_path: bt.to_string_lossy().to_string(),
        java_installed: java.is_some(),
        java_path: java
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        data_dir: data_dir.to_string_lossy().to_string(),
    })
}

/// Check which managed tools haven't been updated within the staleness
/// threshold (30 days by default).  Only reports tools that are actually
/// installed — if a tool hasn't been downloaded at all it won't appear.
#[tauri::command]
pub fn check_for_stale_tools(app: AppHandle) -> Result<Vec<StaleTool>, String> {
    let data_dir = get_data_dir(&app)?;
    let config = load_config(&data_dir);
    let now = unix_now();

    let tools: Vec<(&str, &str, bool)> = vec![
        ("platform-tools", "ADB Platform-Tools", managed_adb_path(&data_dir).exists()),
        ("bundletool", "bundletool", managed_bundletool_path(&data_dir).exists()),
        ("java", "Java JRE", managed_java_path(&data_dir).is_some()),
    ];

    let mut stale = Vec::new();

    for (key, label, installed) in tools {
        if !installed {
            continue;
        }

        let last = config.last_updated.get(key).copied().unwrap_or(0);
        let age = now.saturating_sub(last);

        if age >= STALE_THRESHOLD_SECS {
            stale.push(StaleTool {
                tool: key.to_string(),
                label: label.to_string(),
                last_updated_secs: last,
                age_days: age / 86_400,
            });
        }
    }

    Ok(stale)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_status_serializes() {
        let status = ToolsStatus {
            adb_installed: true,
            adb_path: "/path/adb".to_string(),
            bundletool_installed: false,
            bundletool_path: "".to_string(),
            java_installed: true,
            java_path: "/path/java".to_string(),
            data_dir: "/data".to_string(),
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"adb_installed\":true"));
        assert!(json.contains("\"bundletool_installed\":false"));
    }

    #[test]
    fn stale_tool_serializes() {
        let stale = StaleTool {
            tool: "bundletool".to_string(),
            label: "bundletool".to_string(),
            last_updated_secs: 1000,
            age_days: 45,
        };
        let json = serde_json::to_string(&stale).unwrap();
        assert!(json.contains("\"age_days\":45"));
    }
}
