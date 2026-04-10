//! Managed tools: downloads ADB platform-tools and bundletool into the app's
//! local data directory so the user never needs to install the Android SDK.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};

/// Number of seconds before a tool is considered stale and the user is prompted.
const STALE_THRESHOLD_SECS: u64 = 30 * 24 * 60 * 60; // 30 days

// ─── Data Types ───────────────────────────────────────────────────────────────

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

#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub tool: String,
    pub downloaded: u64,
    pub total: u64,
    pub percent: u32,
    pub status: String, // "downloading" | "extracting" | "done" | "error"
}

/// Persisted in `tools_config.json` inside the app data directory.
/// Tracks when each managed tool was last downloaded / updated.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolsConfig {
    /// Map of tool name → Unix timestamp (seconds) of last update.
    /// Keys: "platform-tools", "bundletool", "java"
    #[serde(default)]
    pub last_updated: HashMap<String, u64>,
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

// ─── Config Persistence ───────────────────────────────────────────────────────

fn config_path(data_dir: &PathBuf) -> PathBuf {
    data_dir.join("tools_config.json")
}

fn load_config(data_dir: &PathBuf) -> ToolsConfig {
    let path = config_path(data_dir);
    if !path.exists() {
        return ToolsConfig::default();
    }
    match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => ToolsConfig::default(),
    }
}

fn save_config(data_dir: &PathBuf, config: &ToolsConfig) -> Result<(), String> {
    let path = config_path(data_dir);
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize tools config: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write tools config: {}", e))?;
    Ok(())
}

/// Record the current time as the "last_updated" timestamp for a tool.
fn mark_tool_updated(data_dir: &PathBuf, tool: &str) -> Result<(), String> {
    let mut config = load_config(data_dir);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("Clock error: {}", e))?
        .as_secs();
    config.last_updated.insert(tool.to_string(), now);
    save_config(data_dir, &config)
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ─── Path Helpers ─────────────────────────────────────────────────────────────

/// Returns the app-managed path where ADB would live.
pub fn managed_adb_path(data_dir: &PathBuf) -> PathBuf {
    data_dir
        .join("platform-tools")
        .join(crate::adb_binary())
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

    let bin = crate::java_binary();

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

// ─── Tauri Commands ───────────────────────────────────────────────────────────

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

/// Download and extract Android SDK platform-tools (contains ADB) into the
/// app's data directory.  Works on macOS, Windows, and Linux.
#[tauri::command]
pub async fn setup_platform_tools(app: AppHandle) -> Result<String, String> {
    let data_dir = get_data_dir(&app)?;
    fs::create_dir_all(&data_dir).map_err(|e| format!("Failed to create data dir: {}", e))?;

    let zip_path = data_dir.join("platform-tools.zip");

    let url = if cfg!(target_os = "macos") {
        "https://dl.google.com/android/repository/platform-tools-latest-darwin.zip"
    } else if cfg!(target_os = "windows") {
        "https://dl.google.com/android/repository/platform-tools-latest-windows.zip"
    } else {
        "https://dl.google.com/android/repository/platform-tools-latest-linux.zip"
    };

    // ── Download ──────────────────────────────────────────────────────────
    emit_progress(&app, "platform-tools", 0, 0, 0, "downloading");

    let client = reqwest::Client::builder()
        .user_agent("AndroidApplicationInstaller/0.1")
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Download failed — HTTP {}", response.status()));
    }

    let total_size = response.content_length().unwrap_or(0);
    let mut stream = response.bytes_stream();

    let mut file = tokio::fs::File::create(&zip_path)
        .await
        .map_err(|e| format!("Failed to create file: {}", e))?;

    let mut downloaded: u64 = 0;
    let mut last_emit: u64 = 0;

    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download error: {}", e))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Write error: {}", e))?;
        downloaded += chunk.len() as u64;

        // Throttle progress events to ~every 100 KB
        if downloaded - last_emit > 100_000 || downloaded >= total_size {
            last_emit = downloaded;
            let pct = if total_size > 0 {
                (downloaded as f64 / total_size as f64 * 100.0) as u32
            } else {
                0
            };
            emit_progress(&app, "platform-tools", downloaded, total_size, pct, "downloading");
        }
    }

    file.flush()
        .await
        .map_err(|e| format!("Flush error: {}", e))?;
    drop(file);

    // ── Extract ───────────────────────────────────────────────────────────
    emit_progress(&app, "platform-tools", downloaded, total_size, 100, "extracting");

    // Remove old install
    let pt_dir = data_dir.join("platform-tools");
    if pt_dir.exists() {
        fs::remove_dir_all(&pt_dir)
            .map_err(|e| format!("Failed to remove old platform-tools: {}", e))?;
    }

    let zip_clone = zip_path.clone();
    let data_clone = data_dir.clone();

    tokio::task::spawn_blocking(move || extract_zip(&zip_clone, &data_clone))
        .await
        .map_err(|e| format!("Extract thread error: {}", e))??;

    // ── Set executable permissions (Unix) ─────────────────────────────────
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(entries) = fs::read_dir(&pt_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() {
                    if let Ok(meta) = fs::metadata(&path) {
                        let mut perms = meta.permissions();
                        perms.set_mode(0o755);
                        let _ = fs::set_permissions(&path, perms);
                    }
                }
            }
        }
    }

    // ── Cleanup ───────────────────────────────────────────────────────────
    let _ = fs::remove_file(&zip_path);

    emit_progress(&app, "platform-tools", downloaded, total_size, 100, "done");

    let adb_path = managed_adb_path(&data_dir);
    if !adb_path.exists() {
        return Err("Extraction completed but ADB binary not found in the archive.".into());
    }

    // Record update timestamp
    let _ = mark_tool_updated(&data_dir, "platform-tools");

    Ok(adb_path.to_string_lossy().to_string())
}

/// Download the latest bundletool.jar from GitHub into the app's data directory.
#[tauri::command]
pub async fn setup_bundletool(app: AppHandle) -> Result<String, String> {
    let data_dir = get_data_dir(&app)?;
    fs::create_dir_all(&data_dir).map_err(|e| format!("Failed to create data dir: {}", e))?;

    let dest = managed_bundletool_path(&data_dir);

    // ── Resolve latest version ────────────────────────────────────────────
    emit_progress(&app, "bundletool", 0, 0, 0, "downloading");

    let no_redirect = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("AndroidApplicationInstaller/0.1")
        .build()
        .map_err(|e| e.to_string())?;

    let resp = no_redirect
        .head("https://github.com/google/bundletool/releases/latest")
        .send()
        .await
        .map_err(|e| format!("Failed to resolve bundletool version: {}", e))?;

    let location = resp
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .ok_or("Could not resolve latest bundletool version (no redirect from GitHub).")?;

    let version = location
        .rsplit('/')
        .next()
        .ok_or("Could not parse bundletool version from redirect URL.")?;

    if version.is_empty() {
        return Err("Could not determine bundletool version.".into());
    }

    let download_url = format!(
        "https://github.com/google/bundletool/releases/download/{}/bundletool-all-{}.jar",
        version, version
    );

    // ── Download ──────────────────────────────────────────────────────────
    let client = reqwest::Client::builder()
        .user_agent("AndroidApplicationInstaller/0.1")
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Download failed — HTTP {}", response.status()));
    }

    let total_size = response.content_length().unwrap_or(0);
    let mut stream = response.bytes_stream();

    // Remove old file first
    let _ = fs::remove_file(&dest);

    let mut file = tokio::fs::File::create(&dest)
        .await
        .map_err(|e| format!("Failed to create file: {}", e))?;

    let mut downloaded: u64 = 0;
    let mut last_emit: u64 = 0;

    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download error: {}", e))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Write error: {}", e))?;
        downloaded += chunk.len() as u64;

        if downloaded - last_emit > 50_000 || downloaded >= total_size {
            last_emit = downloaded;
            let pct = if total_size > 0 {
                (downloaded as f64 / total_size as f64 * 100.0) as u32
            } else {
                0
            };
            emit_progress(&app, "bundletool", downloaded, total_size, pct, "downloading");
        }
    }

    file.flush()
        .await
        .map_err(|e| format!("Flush error: {}", e))?;
    drop(file);

    // Verify file size (bundletool jar is typically > 10 MB)
    let meta = fs::metadata(&dest).map_err(|e| e.to_string())?;
    if meta.len() < 1_000_000 {
        let _ = fs::remove_file(&dest);
        return Err("Downloaded file is too small — download may have failed.".into());
    }

    emit_progress(&app, "bundletool", downloaded, total_size, 100, "done");

    // Record update timestamp
    let _ = mark_tool_updated(&data_dir, "bundletool");

    Ok(format!(
        "Downloaded bundletool {} to {}",
        version,
        dest.to_string_lossy()
    ))
}

// ─── Internal Helpers ─────────────────────────────────────────────────────────

/// Adoptium (Eclipse Temurin) JRE download URL for the current platform.
/// Uses the v3 binary API which returns a redirect to the latest JRE 21 LTS
/// archive (.tar.gz on macOS/Linux, .zip on Windows).
fn adoptium_jre_url() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    { "https://api.adoptium.net/v3/binary/latest/21/ga/mac/aarch64/jre/hotspot/normal/eclipse" }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    { "https://api.adoptium.net/v3/binary/latest/21/ga/mac/x64/jre/hotspot/normal/eclipse" }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    { "https://api.adoptium.net/v3/binary/latest/21/ga/windows/x64/jre/hotspot/normal/eclipse" }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    { "https://api.adoptium.net/v3/binary/latest/21/ga/windows/aarch64/jre/hotspot/normal/eclipse" }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    { "https://api.adoptium.net/v3/binary/latest/21/ga/linux/x64/jre/hotspot/normal/eclipse" }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    { "https://api.adoptium.net/v3/binary/latest/21/ga/linux/aarch64/jre/hotspot/normal/eclipse" }
}

/// Download and extract an Eclipse Temurin (Adoptium) JRE into the app's
/// data directory.  No system-wide install, no environment variables needed.
#[tauri::command]
pub async fn setup_java(app: AppHandle) -> Result<String, String> {
    let data_dir = get_data_dir(&app)?;
    fs::create_dir_all(&data_dir).map_err(|e| format!("Failed to create data dir: {}", e))?;

    let jre_dir = data_dir.join("jre");
    let url = adoptium_jre_url();

    let archive_name = if cfg!(target_os = "windows") {
        "jre.zip"
    } else {
        "jre.tar.gz"
    };
    let archive_path = data_dir.join(archive_name);

    // ── Download ──────────────────────────────────────────────────────────
    emit_progress(&app, "java", 0, 0, 0, "downloading");

    let client = reqwest::Client::builder()
        .user_agent("AndroidApplicationInstaller/0.1")
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Download failed — HTTP {}", response.status()));
    }

    let total_size = response.content_length().unwrap_or(0);
    let mut stream = response.bytes_stream();

    let mut file = tokio::fs::File::create(&archive_path)
        .await
        .map_err(|e| format!("Failed to create file: {}", e))?;

    let mut downloaded: u64 = 0;
    let mut last_emit: u64 = 0;

    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download error: {}", e))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Write error: {}", e))?;
        downloaded += chunk.len() as u64;

        if downloaded - last_emit > 100_000 || downloaded >= total_size {
            last_emit = downloaded;
            let pct = if total_size > 0 {
                (downloaded as f64 / total_size as f64 * 100.0) as u32
            } else {
                0
            };
            emit_progress(&app, "java", downloaded, total_size, pct, "downloading");
        }
    }

    file.flush()
        .await
        .map_err(|e| format!("Flush error: {}", e))?;
    drop(file);

    // ── Extract ───────────────────────────────────────────────────────────
    emit_progress(&app, "java", downloaded, total_size, 100, "extracting");

    // Remove old JRE install
    if jre_dir.exists() {
        fs::remove_dir_all(&jre_dir)
            .map_err(|e| format!("Failed to remove old JRE: {}", e))?;
    }
    fs::create_dir_all(&jre_dir)
        .map_err(|e| format!("Failed to create JRE directory: {}", e))?;

    let archive_clone = archive_path.clone();
    let jre_clone = jre_dir.clone();

    tokio::task::spawn_blocking(move || extract_archive(&archive_clone, &jre_clone))
        .await
        .map_err(|e| format!("Extract thread error: {}", e))??;

    // ── Set executable permissions (Unix) ─────────────────────────────────
    #[cfg(unix)]
    {
        if let Some(ref java) = managed_java_path(&data_dir) {
            use std::os::unix::fs::PermissionsExt;
            // chmod the java binary and the bin directory contents
            if let Some(bin_dir) = java.parent() {
                if let Ok(entries) = fs::read_dir(bin_dir) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        let path = entry.path();
                        if path.is_file() {
                            if let Ok(meta) = fs::metadata(&path) {
                                let mut perms = meta.permissions();
                                perms.set_mode(0o755);
                                let _ = fs::set_permissions(&path, perms);
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Cleanup ───────────────────────────────────────────────────────────
    let _ = fs::remove_file(&archive_path);

    emit_progress(&app, "java", downloaded, total_size, 100, "done");

    match managed_java_path(&data_dir) {
        Some(java) => {
            // Record update timestamp
            let _ = mark_tool_updated(&data_dir, "java");
            Ok(java.to_string_lossy().to_string())
        }
        None => Err("Extraction completed but Java binary not found in the archive.".into()),
    }
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

fn emit_progress(app: &AppHandle, tool: &str, dl: u64, total: u64, pct: u32, status: &str) {
    let _ = app.emit(
        "download-progress",
        DownloadProgress {
            tool: tool.to_string(),
            downloaded: dl,
            total,
            percent: pct,
            status: status.to_string(),
        },
    );
}

/// Extract an archive (tar.gz on macOS/Linux, zip on Windows).
fn extract_archive(archive_path: &PathBuf, dest_dir: &PathBuf) -> Result<(), String> {
    #[cfg(not(target_os = "windows"))]
    {
        let output = std::process::Command::new("tar")
            .args([
                "xzf",
                &archive_path.to_string_lossy(),
                "-C",
                &dest_dir.to_string_lossy(),
            ])
            .output()
            .map_err(|e| format!("Failed to run tar: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("tar extraction failed: {}", stderr));
        }
    }

    #[cfg(target_os = "windows")]
    {
        extract_zip(archive_path, dest_dir)?;
    }

    Ok(())
}

/// Synchronously extract a ZIP file into `dest_dir`.
fn extract_zip(zip_path: &PathBuf, dest_dir: &PathBuf) -> Result<(), String> {
    let file = fs::File::open(zip_path).map_err(|e| format!("Failed to open zip: {}", e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("Invalid zip archive: {}", e))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Zip entry error: {}", e))?;

        // Sanitize to prevent zip-slip attacks
        let name = entry.name().to_string();
        if name.contains("..") {
            continue;
        }

        let outpath = dest_dir.join(&name);

        if entry.is_dir() {
            fs::create_dir_all(&outpath).map_err(|e| format!("mkdir error: {}", e))?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("mkdir error: {}", e))?;
            }
            let mut outfile =
                fs::File::create(&outpath).map_err(|e| format!("create error: {}", e))?;
            std::io::copy(&mut entry, &mut outfile).map_err(|e| format!("copy error: {}", e))?;
        }
    }

    Ok(())
}

// ─── Recent Files ─────────────────────────────────────────────────────────────

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

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "aai_test_{}_{}", std::process::id(), id
        ));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    fn cleanup(dir: &PathBuf) {
        let _ = fs::remove_dir_all(dir);
    }

    // ── STALE_THRESHOLD_SECS ─────────────────────────────────────────────

    #[test]
    fn stale_threshold_is_30_days() {
        assert_eq!(STALE_THRESHOLD_SECS, 30 * 24 * 60 * 60);
    }

    // ── ToolsConfig ──────────────────────────────────────────────────────

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

    // ── Config persistence ───────────────────────────────────────────────

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
        // Should return default on parse error
        assert!(config.last_updated.is_empty());
        cleanup(&dir);
    }

    #[test]
    fn mark_tool_updated_records_timestamp() {
        let dir = temp_dir();
        mark_tool_updated(&dir, "platform-tools").unwrap();
        let config = load_config(&dir);
        let ts = config.last_updated.get("platform-tools").unwrap();
        // Timestamp should be recent (within last 10 seconds)
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

    // ── Path helpers ─────────────────────────────────────────────────────

    #[test]
    fn managed_adb_path_is_under_data_dir() {
        let dir = PathBuf::from("/test/data");
        let path = managed_adb_path(&dir);
        assert!(path.starts_with("/test/data/platform-tools"));
        assert!(path.to_string_lossy().contains(crate::adb_binary()));
    }

    #[test]
    fn managed_bundletool_path_is_under_data_dir() {
        let dir = PathBuf::from("/test/data");
        let path = managed_bundletool_path(&dir);
        assert_eq!(path, PathBuf::from("/test/data/bundletool.jar"));
    }

    #[test]
    fn managed_java_path_returns_none_when_no_jre() {
        let dir = temp_dir();
        let result = managed_java_path(&dir);
        assert!(result.is_none());
        cleanup(&dir);
    }

    // ── unix_now ─────────────────────────────────────────────────────────

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

    // ── RecentFile / RecentFilesConfig ───────────────────────────────────

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

    // ── prune_list ───────────────────────────────────────────────────────

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
        // Use a file that definitely exists
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
        // Should keep the one with latest last_used
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

    // ── extract_zip error handling ───────────────────────────────────────

    #[test]
    fn extract_zip_nonexistent_file() {
        let dest = temp_dir();
        let result = extract_zip(&PathBuf::from("/nonexistent.zip"), &dest);
        assert!(result.is_err());
        cleanup(&dest);
    }

    #[test]
    fn extract_zip_invalid_file() {
        let dir = temp_dir();
        let bad_zip = dir.join("bad.zip");
        fs::write(&bad_zip, "this is not a zip").unwrap();
        let result = extract_zip(&bad_zip, &dir);
        assert!(result.is_err());
        cleanup(&dir);
    }

    // ── ToolsStatus serialization ────────────────────────────────────────

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

    // ── DownloadProgress serialization ───────────────────────────────────

    #[test]
    fn download_progress_serializes() {
        let progress = DownloadProgress {
            tool: "platform-tools".to_string(),
            downloaded: 5000,
            total: 10000,
            percent: 50,
            status: "downloading".to_string(),
        };
        let json = serde_json::to_string(&progress).unwrap();
        assert!(json.contains("\"percent\":50"));
        assert!(json.contains("\"status\":\"downloading\""));
    }

    // ── StaleTool serialization ──────────────────────────────────────────

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

    // ── MAX_RECENT_FILES constant ────────────────────────────────────────

    #[test]
    fn max_recent_files_is_ten() {
        assert_eq!(MAX_RECENT_FILES, 10);
    }

    // ── config_path / recent_files_path ──────────────────────────────────

    #[test]
    fn config_path_ends_with_correct_filename() {
        let dir = PathBuf::from("/test");
        assert_eq!(config_path(&dir), PathBuf::from("/test/tools_config.json"));
    }

    #[test]
    fn recent_files_path_ends_with_correct_filename() {
        let dir = PathBuf::from("/test");
        assert_eq!(recent_files_path(&dir), PathBuf::from("/test/recent_files.json"));
    }

    // ── adoptium_jre_url ─────────────────────────────────────────────────

    #[test]
    fn adoptium_jre_url_is_https() {
        let url = adoptium_jre_url();
        assert!(url.starts_with("https://"));
    }

    #[test]
    fn adoptium_jre_url_contains_jre() {
        let url = adoptium_jre_url();
        assert!(url.contains("jre"));
    }

    #[test]
    fn adoptium_jre_url_targets_version_21() {
        let url = adoptium_jre_url();
        assert!(url.contains("/21/"));
    }
}
