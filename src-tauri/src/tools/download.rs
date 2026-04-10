//! Downloading and extracting managed tools: ADB platform-tools, bundletool,
//! and an embedded JRE (Adoptium Temurin).

use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};

use super::config::mark_tool_updated;
use super::paths::{get_data_dir, managed_adb_path, managed_java_path};

// ─── Data Types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub tool: String,
    pub downloaded: u64,
    pub total: u64,
    pub percent: u32,
    pub status: String, // "downloading" | "extracting" | "done" | "error"
}

// ─── Progress Helper ─────────────────────────────────────────────────────────

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

// ─── Tauri Commands ──────────────────────────────────────────────────────────

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

    let dest = super::paths::managed_bundletool_path(&data_dir);

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

/// Adoptium (Eclipse Temurin) JRE download URL for the current platform.
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

// ─── Extraction Helpers ──────────────────────────────────────────────────────

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
pub(crate) fn extract_zip(zip_path: &PathBuf, dest_dir: &PathBuf) -> Result<(), String> {
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

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn extract_zip_nonexistent_file() {
        let dest = std::env::temp_dir().join("aai_dl_test_nonexist");
        let _ = std::fs::create_dir_all(&dest);
        let result = extract_zip(&PathBuf::from("/nonexistent.zip"), &dest);
        assert!(result.is_err());
        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn extract_zip_invalid_file() {
        let dir = std::env::temp_dir().join("aai_dl_test_invalid");
        let _ = std::fs::create_dir_all(&dir);
        let bad_zip = dir.join("bad.zip");
        std::fs::write(&bad_zip, "this is not a zip").unwrap();
        let result = extract_zip(&bad_zip, &dir);
        assert!(result.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
