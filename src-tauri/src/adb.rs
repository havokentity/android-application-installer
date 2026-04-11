//! ADB device operations: discovery, install, launch, uninstall, and listing.

use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::Emitter;

use crate::cmd::{adb_binary, emit_op_progress, run_cmd, run_cmd_async, run_cmd_async_lenient, run_cmd_lenient};
use crate::tools;

// ─── Data Types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub serial: String,
    pub state: String,
    pub model: String,
    pub product: String,
    pub transport_id: String,
}

// ─── Tauri Commands ──────────────────────────────────────────────────────────

/// Auto-detect ADB binary path.
/// Checks the app's managed platform-tools FIRST, then falls back to system locations.
#[tauri::command]
pub(crate) fn find_adb(app: tauri::AppHandle) -> Result<String, String> {
    // 0. Check app-managed platform-tools (highest priority)
    if let Ok(data_dir) = tools::get_data_dir(&app) {
        let managed = tools::managed_adb_path(&data_dir);
        if managed.exists() {
            return Ok(managed.to_string_lossy().into());
        }
    }

    // 1. Check ANDROID_HOME / ANDROID_SDK_ROOT environment variables
    for var in &["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Ok(sdk) = env::var(var) {
            let adb = PathBuf::from(&sdk)
                .join("platform-tools")
                .join(adb_binary());
            if adb.exists() {
                return Ok(adb.to_string_lossy().into());
            }
        }
    }

    // 2. Check common default SDK locations
    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .unwrap_or_default();

    let mut candidates = vec![
        PathBuf::from(&home)
            .join("Library/Android/sdk/platform-tools")
            .join(adb_binary()), // macOS
        PathBuf::from(&home)
            .join("Android/Sdk/platform-tools")
            .join(adb_binary()), // Linux
    ];

    // Windows: check LOCALAPPDATA
    if cfg!(target_os = "windows") {
        if let Ok(local) = env::var("LOCALAPPDATA") {
            candidates.push(
                PathBuf::from(&local)
                    .join("Android/Sdk/platform-tools")
                    .join(adb_binary()),
            );
        }
    }

    for c in &candidates {
        if c.exists() {
            return Ok(c.to_string_lossy().into());
        }
    }

    // 3. Try which / where (PATH lookup)
    let which = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };
    if let Ok((stdout, _, _)) = run_cmd_lenient(which, &["adb"]) {
        let path = stdout.trim().lines().next().unwrap_or("").trim().to_string();
        if !path.is_empty() && Path::new(&path).exists() {
            return Ok(path);
        }
    }

    Err("ADB not found. Use the \"Download ADB\" button above to install it automatically.".into())
}

/// List connected Android devices via `adb devices -l`.
#[tauri::command]
pub(crate) fn get_devices(adb_path: String) -> Result<Vec<DeviceInfo>, String> {
    // Start the server first (idempotent if already running)
    let _ = run_cmd_lenient(&adb_path, &["start-server"]);

    let (stdout, _) = run_cmd(&adb_path, &["devices", "-l"])?;

    let mut devices = Vec::new();
    for line in stdout.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, char::is_whitespace).collect();
        if parts.len() < 2 {
            continue;
        }

        let serial = parts[0].to_string();
        let rest = parts[1].trim();

        // First token is the state (device, offline, unauthorized, etc.)
        let state_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let state = rest[..state_end].to_string();

        // Parse key:value properties (model, product, transport_id, etc.)
        let mut model = String::new();
        let mut product = String::new();
        let mut transport_id = String::new();

        if state_end < rest.len() {
            for part in rest[state_end..].split_whitespace() {
                if let Some((key, value)) = part.split_once(':') {
                    match key {
                        "model" => model = value.replace('_', " "),
                        "product" => product = value.to_string(),
                        "transport_id" => transport_id = value.to_string(),
                        _ => {}
                    }
                }
            }
        }

        devices.push(DeviceInfo {
            serial,
            state,
            model,
            product,
            transport_id,
        });
    }

    Ok(devices)
}

// ─── Device Tracking (push-based) ────────────────────────────────────────────

/// Managed state for the `adb track-devices` background task.
pub struct DeviceTracker {
    stop_flag: Arc<AtomicBool>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl Default for DeviceTracker {
    fn default() -> Self {
        Self {
            stop_flag: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }
}

/// Parse `adb devices -l` style output (used by both `get_devices` and tracker).
pub(crate) fn parse_device_list(output: &str) -> Vec<DeviceInfo> {
    let mut devices = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("List of") {
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, char::is_whitespace).collect();
        if parts.len() < 2 {
            continue;
        }

        let serial = parts[0].to_string();
        let rest = parts[1].trim();
        let state_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let state = rest[..state_end].to_string();

        let mut model = String::new();
        let mut product = String::new();
        let mut transport_id = String::new();

        if state_end < rest.len() {
            for part in rest[state_end..].split_whitespace() {
                if let Some((key, value)) = part.split_once(':') {
                    match key {
                        "model" => model = value.replace('_', " "),
                        "product" => product = value.to_string(),
                        "transport_id" => transport_id = value.to_string(),
                        _ => {}
                    }
                }
            }
        }

        devices.push(DeviceInfo { serial, state, model, product, transport_id });
    }
    devices
}

/// Start push-based device tracking via `adb track-devices -l`.
/// Emits `device-list-changed` events when the device list changes.
#[tauri::command]
pub(crate) async fn start_device_tracking(
    app: tauri::AppHandle,
    tracker: tauri::State<'_, Mutex<DeviceTracker>>,
    adb_path: String,
) -> Result<(), String> {
    let mut guard = tracker.lock().await;

    // Stop any existing tracker
    if let Some(handle) = guard.handle.take() {
        guard.stop_flag.store(true, Ordering::SeqCst);
        handle.abort();
    }

    let stop_flag = Arc::new(AtomicBool::new(false));
    guard.stop_flag = stop_flag.clone();

    let app_handle = app.clone();
    let adb = adb_path.clone();

    let handle = tokio::spawn(async move {
        // Ensure ADB server is running
        let _ = tokio::process::Command::new(&adb)
            .args(["start-server"])
            .output()
            .await;

        loop {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }

            // Spawn `adb track-devices -l`
            let child = tokio::process::Command::new(&adb)
                .args(["track-devices", "-l"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true)
                .spawn();

            let mut child = match child {
                Ok(c) => c,
                Err(_) => {
                    // If we can't spawn, wait and retry
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }
            };

            let stdout = match child.stdout.take() {
                Some(s) => s,
                None => break,
            };

            let mut reader = tokio::io::BufReader::new(stdout);
            let mut prev_serials = String::new();

            loop {
                if stop_flag.load(Ordering::Relaxed) {
                    let _ = child.kill().await;
                    return;
                }

                // Read the 4-byte hex length prefix
                let mut len_buf = [0u8; 4];
                match tokio::io::AsyncReadExt::read_exact(&mut reader, &mut len_buf).await {
                    Ok(_) => {}
                    Err(_) => break, // process died, restart
                }

                let len = match u32::from_str_radix(
                    &String::from_utf8_lossy(&len_buf),
                    16,
                ) {
                    Ok(l) => l as usize,
                    Err(_) => break,
                };

                // Read the payload
                let mut payload = vec![0u8; len];
                if len > 0 {
                    match tokio::io::AsyncReadExt::read_exact(&mut reader, &mut payload).await {
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }

                let output = String::from_utf8_lossy(&payload).to_string();
                let devices = parse_device_list(&output);
                let new_serials = {
                    let mut s: Vec<String> = devices.iter().map(|d| d.serial.clone()).collect();
                    s.sort();
                    s.join(",")
                };

                // Only emit if the device list actually changed
                if new_serials != prev_serials {
                    prev_serials = new_serials;
                    let _ = app_handle.emit("device-list-changed", &devices);
                }
            }

            // Process exited — wait a bit and restart
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
    });

    guard.handle = Some(handle);
    Ok(())
}

/// Stop push-based device tracking.
#[tauri::command]
pub(crate) async fn stop_device_tracking(
    tracker: tauri::State<'_, Mutex<DeviceTracker>>,
) -> Result<(), String> {
    let mut guard = tracker.lock().await;
    guard.stop_flag.store(true, Ordering::SeqCst);
    if let Some(handle) = guard.handle.take() {
        handle.abort();
    }
    Ok(())
}

// ─── Wireless ADB (WiFi) ────────────────────────────────────────────────────

/// A device discovered via `adb mdns services`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MdnsService {
    pub name: String,
    pub service_type: String,
    pub ip_port: String,
}

/// Parse the output of `adb mdns services`.
///
/// Typical output:
/// ```text
/// List of discovered mdns services
/// adb-SERIAL123	_adb-tls-connect._tcp.	192.168.1.100:43567
/// adb-SERIAL456	_adb-tls-pairing._tcp.	192.168.1.101:37215
/// ```
pub(crate) fn parse_mdns_services(stdout: &str) -> Vec<MdnsService> {
    stdout
        .lines()
        .filter(|l| l.contains('\t'))
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                Some(MdnsService {
                    name: parts[0].trim().to_string(),
                    service_type: parts[1].trim().to_string(),
                    ip_port: parts[2].trim().to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}

/// Check if mDNS discovery is available (`adb mdns check`).
#[tauri::command]
pub(crate) async fn adb_mdns_check(adb_path: String) -> Result<bool, String> {
    let (stdout, stderr, _) = run_cmd_async_lenient(&adb_path, &["mdns", "check"]).await?;
    let combined = format!("{}\n{}", stdout, stderr);
    Ok(combined.contains("mdns daemon version") || combined.contains("Openscreen"))
}

/// Discover devices on the local network via `adb mdns services`.
/// Async with cancellation support. Deduplicates by (name, service_type, ip_port).
#[tauri::command]
pub(crate) async fn adb_mdns_services(adb_path: String) -> Result<Vec<MdnsService>, String> {
    let (stdout, stderr, _) = run_cmd_async_lenient(&adb_path, &["mdns", "services"]).await?;
    if stderr.contains("unknown host service") || stderr.contains("mdns") && stderr.contains("not") {
        return Err("mDNS discovery is not supported by this ADB version. Update platform-tools to 31+.".into());
    }
    let mut services = parse_mdns_services(&stdout);
    // Deduplicate — adb mdns services can return the same entry multiple times.
    // Sort first so identical entries are adjacent, then dedup.
    services.sort_by(|a, b| (&a.name, &a.service_type, &a.ip_port).cmp(&(&b.name, &b.service_type, &b.ip_port)));
    services.dedup_by(|a, b| a.name == b.name && a.service_type == b.service_type && a.ip_port == b.ip_port);
    Ok(services)
}

/// Parse the result of `adb pair <ip:port> <code>`.
pub(crate) fn parse_pair_result(stdout: &str, stderr: &str) -> Result<String, String> {
    let combined = format!("{}\n{}", stdout, stderr);
    if stdout.contains("Successfully paired") {
        Ok(stdout.trim().to_string())
    } else if combined.contains("Failed") || combined.contains("error") || combined.contains("refused") {
        Err(format!("Pairing failed: {}", combined.trim()))
    } else if combined.contains("timed out") || combined.contains("timeout") {
        Err("Pairing timed out. Make sure the pairing code is correct and the device is on the same network.".into())
    } else if stdout.trim().is_empty() && stderr.trim().is_empty() {
        Err("Pairing failed: no response from ADB.".into())
    } else {
        Err(format!("Unexpected pairing response: {}", combined.trim()))
    }
}

/// Parse the result of `adb connect <ip:port>`.
pub(crate) fn parse_connect_result(stdout: &str, stderr: &str) -> Result<String, String> {
    let combined = format!("{}\n{}", stdout, stderr);
    if stdout.contains("connected to") {
        Ok(stdout.trim().to_string())
    } else if stdout.contains("already connected") {
        Ok(stdout.trim().to_string())
    } else if combined.contains("refused") || combined.contains("Connection refused") {
        Err("Connection refused. Make sure wireless debugging is enabled and the port is correct.".into())
    } else if combined.contains("timed out") || combined.contains("timeout") {
        Err("Connection timed out. Check the IP address and that the device is on the same network.".into())
    } else if combined.contains("failed") || combined.contains("error") {
        Err(format!("Connection failed: {}", combined.trim()))
    } else if stdout.trim().is_empty() && stderr.trim().is_empty() {
        Err("Connection failed: no response from ADB.".into())
    } else {
        Err(format!("Unexpected connection response: {}", combined.trim()))
    }
}

/// Parse the result of `adb disconnect <ip:port>`.
pub(crate) fn parse_disconnect_result(stdout: &str, _stderr: &str) -> Result<String, String> {
    if stdout.contains("disconnected") {
        Ok(stdout.trim().to_string())
    } else if stdout.contains("error") || stdout.contains("no such device") {
        Err(format!("Disconnect failed: {}", stdout.trim()))
    } else {
        Ok(format!("Disconnected: {}", stdout.trim()))
    }
}

/// Pair with a device over WiFi using `adb pair <ip:port> <pairing_code>`.
/// Requires Android 11+ with wireless debugging enabled.
/// Async with cancellation support — pairing can hang if the code is wrong.
#[tauri::command]
pub(crate) async fn adb_pair(
    adb_path: String,
    ip_port: String,
    pairing_code: String,
) -> Result<String, String> {
    let (stdout, stderr, _) = run_cmd_async_lenient(&adb_path, &["pair", &ip_port, &pairing_code]).await?;
    parse_pair_result(&stdout, &stderr)
}

/// Connect to a device over WiFi using `adb connect <ip:port>`.
/// Async with cancellation support.
#[tauri::command]
pub(crate) async fn adb_connect(
    adb_path: String,
    ip_port: String,
) -> Result<String, String> {
    let (stdout, stderr, _) = run_cmd_async_lenient(&adb_path, &["connect", &ip_port]).await?;
    parse_connect_result(&stdout, &stderr)
}

/// Disconnect a wireless device using `adb disconnect <ip:port>`.
/// Async with cancellation support.
#[tauri::command]
pub(crate) async fn adb_disconnect(
    adb_path: String,
    ip_port: String,
) -> Result<String, String> {
    let (stdout, stderr, _) = run_cmd_async_lenient(&adb_path, &["disconnect", &ip_port]).await?;
    parse_disconnect_result(&stdout, &stderr)
}

/// Install an APK onto the specified device (async with progress & cancellation).
#[tauri::command]
pub(crate) async fn install_apk(
    app: tauri::AppHandle,
    adb_path: String,
    device: String,
    apk_path: String,
) -> Result<String, String> {
    emit_op_progress(
        &app, "install_apk", &device, "running",
        "Installing APK...", Some(1), Some(1), true,
    );

    let (stdout, stderr) = match run_cmd_async(
        &adb_path,
        &["-s", &device, "install", "-r", &apk_path],
    ).await {
        Ok(v) => v,
        Err(e) => {
            let status = if e.contains("cancelled") { "cancelled" } else { "error" };
            emit_op_progress(&app, "install_apk", &device, status, &e, None, None, false);
            return Err(e);
        }
    };

    // adb install can succeed (exit 0) but still report Failure in stdout
    if stdout.contains("Failure") || stderr.contains("Failure") {
        let msg = format!(
            "APK install failed:\n{}\n{}",
            stdout.trim(),
            stderr.trim()
        );
        emit_op_progress(&app, "install_apk", &device, "error", &msg, None, None, false);
        return Err(msg);
    }

    emit_op_progress(
        &app, "install_apk", &device, "done",
        "APK installed successfully", Some(1), Some(1), false,
    );
    Ok(format!("APK installed successfully.\n{}", stdout.trim()))
}

/// Install an AAB file via bundletool (async with multi-step progress & cancellation).
#[tauri::command]
pub(crate) async fn install_aab(
    app: tauri::AppHandle,
    adb_path: String,
    device: String,
    aab_path: String,
    java_path: String,
    bundletool_path: String,
    keystore_path: Option<String>,
    keystore_pass: Option<String>,
    key_alias: Option<String>,
    key_pass: Option<String>,
) -> Result<String, String> {
    // 1. Prepare temp .apks output path
    let stem = Path::new(&aab_path)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let apks_path = env::temp_dir().join(format!("{}.apks", stem));
    let _ = fs::remove_file(&apks_path);
    let apks_str = apks_path.to_string_lossy().to_string();

    // 2. Build args for `java -jar bundletool.jar build-apks ...`
    let mut build_args: Vec<String> = vec![
        "-jar".into(),
        bundletool_path.clone(),
        "build-apks".into(),
        format!("--bundle={}", aab_path),
        format!("--output={}", apks_str),
        "--connected-device".into(),
        format!("--device-id={}", device),
        format!("--adb={}", adb_path),
    ];

    // Add keystore args if provided
    if let Some(ref ks) = keystore_path {
        if !ks.is_empty() {
            build_args.push(format!("--ks={}", ks));
            if let Some(ref pass) = keystore_pass {
                if !pass.is_empty() {
                    build_args.push(format!("--ks-pass=pass:{}", pass));
                }
            }
            if let Some(ref alias) = key_alias {
                if !alias.is_empty() {
                    build_args.push(format!("--ks-key-alias={}", alias));
                }
            }
            if let Some(ref pass) = key_pass {
                if !pass.is_empty() {
                    build_args.push(format!("--key-pass=pass:{}", pass));
                }
            }
        }
    }

    // ── Step 1/2: Build APKs ──────────────────────────────────────────────
    emit_op_progress(
        &app, "install_aab", &device, "running",
        "Building APKs from AAB...", Some(1), Some(2), true,
    );

    let args_ref: Vec<&str> = build_args.iter().map(|s| s.as_str()).collect();
    if let Err(e) = run_cmd_async(&java_path, &args_ref).await {
        let status = if e.contains("cancelled") { "cancelled" } else { "error" };
        let msg = if e.contains("cancelled") {
            e.clone()
        } else {
            format!("bundletool build-apks failed:\n{}", e)
        };
        emit_op_progress(&app, "install_aab", &device, status, &msg, None, None, false);
        return Err(msg);
    }

    // ── Step 2/2: Install the .apks set onto the device ───────────────────
    emit_op_progress(
        &app, "install_aab", &device, "running",
        "Installing APKs to device...", Some(2), Some(2), true,
    );

    let install_args: Vec<String> = vec![
        "-jar".into(),
        bundletool_path,
        "install-apks".into(),
        format!("--apks={}", apks_str),
        format!("--device-id={}", device),
        format!("--adb={}", adb_path),
    ];

    let inst_ref: Vec<&str> = install_args.iter().map(|s| s.as_str()).collect();
    let (inst_out, _) = match run_cmd_async(&java_path, &inst_ref).await {
        Ok(v) => v,
        Err(e) => {
            let status = if e.contains("cancelled") { "cancelled" } else { "error" };
            let msg = if e.contains("cancelled") {
                e.clone()
            } else {
                format!("bundletool install-apks failed:\n{}", e)
            };
            emit_op_progress(&app, "install_aab", &device, status, &msg, None, None, false);
            let _ = fs::remove_file(&apks_path);
            return Err(msg);
        }
    };

    // 4. Cleanup
    let _ = fs::remove_file(&apks_path);

    emit_op_progress(
        &app, "install_aab", &device, "done",
        "AAB installed successfully", Some(2), Some(2), false,
    );
    Ok(format!("AAB installed successfully.\n{}", inst_out.trim()))
}

/// Extract a universal APK from an AAB file using bundletool (no device required).
#[tauri::command]
pub(crate) async fn extract_apk_from_aab(
    app: tauri::AppHandle,
    aab_path: String,
    output_path: String,
    java_path: String,
    bundletool_path: String,
    keystore_path: Option<String>,
    keystore_pass: Option<String>,
    key_alias: Option<String>,
    key_pass: Option<String>,
) -> Result<String, String> {
    // 1. Prepare temp .apks output path
    let stem = Path::new(&aab_path)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let apks_path = env::temp_dir().join(format!("{}_universal.apks", stem));
    let _ = fs::remove_file(&apks_path);
    let apks_str = apks_path.to_string_lossy().to_string();

    // 2. Build args for `java -jar bundletool.jar build-apks --mode=universal ...`
    let mut build_args: Vec<String> = vec![
        "-jar".into(),
        bundletool_path,
        "build-apks".into(),
        format!("--bundle={}", aab_path),
        format!("--output={}", apks_str),
        "--mode=universal".into(),
    ];

    // Add keystore args if provided
    if let Some(ref ks) = keystore_path {
        if !ks.is_empty() {
            build_args.push(format!("--ks={}", ks));
            if let Some(ref pass) = keystore_pass {
                if !pass.is_empty() {
                    build_args.push(format!("--ks-pass=pass:{}", pass));
                }
            }
            if let Some(ref alias) = key_alias {
                if !alias.is_empty() {
                    build_args.push(format!("--ks-key-alias={}", alias));
                }
            }
            if let Some(ref pass) = key_pass {
                if !pass.is_empty() {
                    build_args.push(format!("--key-pass=pass:{}", pass));
                }
            }
        }
    }

    // ── Step 1/2: Build universal APKs from AAB ─────────────────────────
    emit_op_progress(
        &app, "extract_apk", "", "running",
        "Building universal APK from AAB...", Some(1), Some(2), true,
    );

    let args_ref: Vec<&str> = build_args.iter().map(|s| s.as_str()).collect();
    if let Err(e) = run_cmd_async(&java_path, &args_ref).await {
        let status = if e.contains("cancelled") { "cancelled" } else { "error" };
        let msg = if e.contains("cancelled") {
            e.clone()
        } else {
            format!("bundletool build-apks failed:\n{}", e)
        };
        emit_op_progress(&app, "extract_apk", "", status, &msg, None, None, false);
        return Err(msg);
    }

    // ── Step 2/2: Extract universal.apk from the .apks ZIP ──────────────
    emit_op_progress(
        &app, "extract_apk", "", "running",
        "Extracting universal APK...", Some(2), Some(2), true,
    );

    let apks_file = fs::File::open(&apks_path)
        .map_err(|e| format!("Failed to open .apks file: {}", e))?;
    let mut archive = zip::ZipArchive::new(apks_file)
        .map_err(|e| format!("Failed to read .apks archive: {}", e))?;

    // Find the universal.apk entry
    let apk_entry_name = (0..archive.len())
        .filter_map(|i| {
            let entry = archive.by_index(i).ok()?;
            let name = entry.name().to_string();
            if name.ends_with(".apk") { Some(name) } else { None }
        })
        .next()
        .ok_or_else(|| "No APK found inside the .apks archive".to_string())?;

    let mut apk_entry = archive.by_name(&apk_entry_name)
        .map_err(|e| format!("Failed to read APK from archive: {}", e))?;

    let mut out_file = fs::File::create(&output_path)
        .map_err(|e| format!("Failed to create output file: {}", e))?;

    std::io::copy(&mut apk_entry, &mut out_file)
        .map_err(|e| format!("Failed to write APK: {}", e))?;

    // Cleanup temp file
    let _ = fs::remove_file(&apks_path);

    emit_op_progress(
        &app, "extract_apk", "", "done",
        "APK extracted successfully", Some(2), Some(2), false,
    );
    Ok(format!("Universal APK extracted to:\n{}", output_path))
}

/// Launch an installed app on the device by package name (async with cancellation).
#[tauri::command]
pub(crate) async fn launch_app(
    app: tauri::AppHandle,
    adb_path: String,
    device: String,
    package_name: String,
) -> Result<String, String> {
    emit_op_progress(
        &app, "launch", &device, "running",
        &format!("Launching {}...", package_name), Some(1), Some(1), true,
    );

    let (stdout, stderr) = match run_cmd_async(
        &adb_path,
        &["-s", &device, "shell", "monkey", "-p", &package_name, "1"],
    ).await {
        Ok(v) => v,
        Err(e) => {
            let status = if e.contains("cancelled") { "cancelled" } else { "error" };
            emit_op_progress(&app, "launch", &device, status, &e, None, None, false);
            return Err(e);
        }
    };

    emit_op_progress(
        &app, "launch", &device, "done",
        &format!("Launched {}", package_name), Some(1), Some(1), false,
    );

    Ok(format!(
        "Launched {}\n{}{}",
        package_name,
        stdout.trim(),
        if stderr.trim().is_empty() {
            String::new()
        } else {
            format!("\n{}", stderr.trim())
        }
    ))
}

/// Uninstall an app from the device by package name (async with cancellation).
#[tauri::command]
pub(crate) async fn uninstall_app(
    app: tauri::AppHandle,
    adb_path: String,
    device: String,
    package_name: String,
) -> Result<String, String> {
    emit_op_progress(
        &app, "uninstall", &device, "running",
        &format!("Uninstalling {}...", package_name), Some(1), Some(1), true,
    );

    let (stdout, stderr) = match run_cmd_async(
        &adb_path,
        &["-s", &device, "uninstall", &package_name],
    ).await {
        Ok(v) => v,
        Err(e) => {
            let status = if e.contains("cancelled") { "cancelled" } else { "error" };
            emit_op_progress(&app, "uninstall", &device, status, &e, None, None, false);
            return Err(e);
        }
    };

    emit_op_progress(
        &app, "uninstall", &device, "done",
        &format!("Uninstalled {}", package_name), Some(1), Some(1), false,
    );

    Ok(format!(
        "Uninstalled {}\n{}{}",
        package_name,
        stdout.trim(),
        if stderr.trim().is_empty() {
            String::new()
        } else {
            format!("\n{}", stderr.trim())
        }
    ))
}

/// Force-stop a running app on the device by package name (async with cancellation).
#[tauri::command]
pub(crate) async fn stop_app(
    app: tauri::AppHandle,
    adb_path: String,
    device: String,
    package_name: String,
) -> Result<String, String> {
    emit_op_progress(
        &app, "stop", &device, "running",
        &format!("Stopping {}...", package_name), Some(1), Some(1), true,
    );

    if let Err(e) = run_cmd_async(
        &adb_path,
        &["-s", &device, "shell", "am", "force-stop", &package_name],
    ).await {
        let status = if e.contains("cancelled") { "cancelled" } else { "error" };
        emit_op_progress(&app, "stop", &device, status, &e, None, None, false);
        return Err(e);
    }

    emit_op_progress(
        &app, "stop", &device, "done",
        &format!("Stopped {}", package_name), Some(1), Some(1), false,
    );

    Ok(format!("Stopped {}", package_name))
}

/// List third-party installed packages on the device.
#[tauri::command]
pub(crate) fn list_packages(adb_path: String, device: String) -> Result<Vec<String>, String> {
    let (stdout, _) = run_cmd(
        &adb_path,
        &["-s", &device, "shell", "pm", "list", "packages", "-3"],
    )?;

    let packages: Vec<String> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("package:"))
        .map(|s| s.trim().to_string())
        .collect();

    Ok(packages)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_info_serializes_correctly() {
        let device = DeviceInfo {
            serial: "ABC123".to_string(),
            state: "device".to_string(),
            model: "Pixel 7".to_string(),
            product: "panther".to_string(),
            transport_id: "1".to_string(),
        };
        let json = serde_json::to_string(&device).unwrap();
        assert!(json.contains("\"serial\":\"ABC123\""));
        assert!(json.contains("\"state\":\"device\""));
        assert!(json.contains("\"model\":\"Pixel 7\""));
    }

    #[test]
    fn device_info_deserializes_correctly() {
        let json = r#"{"serial":"XYZ","state":"offline","model":"Test","product":"test","transport_id":"2"}"#;
        let device: DeviceInfo = serde_json::from_str(json).unwrap();
        assert_eq!(device.serial, "XYZ");
        assert_eq!(device.state, "offline");
        assert_eq!(device.model, "Test");
    }

    #[test]
    fn device_info_clone() {
        let device = DeviceInfo {
            serial: "S1".to_string(),
            state: "device".to_string(),
            model: "M".to_string(),
            product: "P".to_string(),
            transport_id: "T".to_string(),
        };
        let cloned = device.clone();
        assert_eq!(device.serial, cloned.serial);
    }

    #[test]
    fn parse_devices_output_format() {
        let adb_output = "List of devices attached\n\
                          ABC123\tdevice usb:1234 product:panther model:Pixel_7 transport_id:1\n\
                          DEF456\tunauthorized usb:5678 transport_id:2\n\
                          \n";

        let mut devices = Vec::new();
        for line in adb_output.lines().skip(1) {
            let line = line.trim();
            if line.is_empty() { continue; }

            let parts: Vec<&str> = line.splitn(2, char::is_whitespace).collect();
            if parts.len() < 2 { continue; }

            let serial = parts[0].to_string();
            let rest = parts[1].trim();
            let state_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let state = rest[..state_end].to_string();

            let mut model = String::new();
            let mut product = String::new();
            let mut transport_id = String::new();

            if state_end < rest.len() {
                for part in rest[state_end..].split_whitespace() {
                    if let Some((key, value)) = part.split_once(':') {
                        match key {
                            "model" => model = value.replace('_', " "),
                            "product" => product = value.to_string(),
                            "transport_id" => transport_id = value.to_string(),
                            _ => {}
                        }
                    }
                }
            }

            devices.push(DeviceInfo { serial, state, model, product, transport_id });
        }

        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].serial, "ABC123");
        assert_eq!(devices[0].state, "device");
        assert_eq!(devices[0].model, "Pixel 7");
        assert_eq!(devices[0].product, "panther");
        assert_eq!(devices[0].transport_id, "1");

        assert_eq!(devices[1].serial, "DEF456");
        assert_eq!(devices[1].state, "unauthorized");
        assert_eq!(devices[1].transport_id, "2");
        assert!(devices[1].model.is_empty());
    }

    #[test]
    fn parse_empty_devices_output() {
        let output = "List of devices attached\n\n";
        let count = output.lines().skip(1).filter(|l| !l.trim().is_empty()).count();
        assert_eq!(count, 0);
    }

    #[test]
    fn detect_failure_in_stdout() {
        let stdout = "Performing Streamed Install\nFailure [INSTALL_FAILED_ALREADY_EXISTS: ...]";
        assert!(stdout.contains("Failure"));
    }

    #[test]
    fn detect_success_in_stdout() {
        let stdout = "Performing Streamed Install\nSuccess";
        assert!(!stdout.contains("Failure"));
        assert!(stdout.contains("Success"));
    }

    #[test]
    fn parse_device_list_standard() {
        let output = "ABC123\tdevice usb:1234 product:panther model:Pixel_7 transport_id:1\n\
                       DEF456\tunauthorized usb:5678 transport_id:2\n";
        let devices = parse_device_list(output);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].serial, "ABC123");
        assert_eq!(devices[0].state, "device");
        assert_eq!(devices[0].model, "Pixel 7");
        assert_eq!(devices[1].serial, "DEF456");
        assert_eq!(devices[1].state, "unauthorized");
    }

    #[test]
    fn parse_device_list_empty() {
        let devices = parse_device_list("");
        assert!(devices.is_empty());
    }

    #[test]
    fn parse_device_list_skips_header() {
        let output = "List of devices attached\nABC123\tdevice\n";
        let devices = parse_device_list(output);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].serial, "ABC123");
    }

    #[test]
    fn parse_device_list_blank_lines() {
        let output = "\n\nABC123\tdevice\n\n";
        let devices = parse_device_list(output);
        assert_eq!(devices.len(), 1);
    }

    // ── Wireless ADB parse tests ─────────────────────────────────────────

    #[test]
    fn parse_pair_success() {
        let stdout = "Successfully paired to 192.168.1.100:37123 [guid=adb-ABC123-def456]";
        let result = parse_pair_result(stdout, "");
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Successfully paired"));
    }

    #[test]
    fn parse_pair_failed() {
        let result = parse_pair_result("", "error: Failed to pair");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("failed"));
    }

    #[test]
    fn parse_pair_timeout() {
        let result = parse_pair_result("", "error: connection timed out");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("timed out"));
    }

    #[test]
    fn parse_pair_empty_response() {
        let result = parse_pair_result("", "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no response"));
    }

    #[test]
    fn parse_pair_refused() {
        let result = parse_pair_result("", "error: connection refused");
        assert!(result.is_err());
    }

    #[test]
    fn parse_connect_success() {
        let stdout = "connected to 192.168.1.100:5555";
        let result = parse_connect_result(stdout, "");
        assert!(result.is_ok());
        assert!(result.unwrap().contains("connected to"));
    }

    #[test]
    fn parse_connect_already_connected() {
        let stdout = "already connected to 192.168.1.100:5555";
        let result = parse_connect_result(stdout, "");
        assert!(result.is_ok());
    }

    #[test]
    fn parse_connect_refused() {
        let result = parse_connect_result("", "Connection refused");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("refused"));
    }

    #[test]
    fn parse_connect_timeout() {
        let result = parse_connect_result("", "connection timed out");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("timed out"));
    }

    #[test]
    fn parse_connect_failed_generic() {
        let result = parse_connect_result("failed to connect", "");
        assert!(result.is_err());
    }

    #[test]
    fn parse_connect_empty_response() {
        let result = parse_connect_result("", "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no response"));
    }

    #[test]
    fn parse_disconnect_success() {
        let stdout = "disconnected 192.168.1.100:5555";
        let result = parse_disconnect_result(stdout, "");
        assert!(result.is_ok());
    }

    #[test]
    fn parse_disconnect_error() {
        let stdout = "error: no such device '192.168.1.100:5555'";
        let result = parse_disconnect_result(stdout, "");
        assert!(result.is_err());
    }

    #[test]
    fn parse_disconnect_unknown_output() {
        let result = parse_disconnect_result("some other output", "");
        assert!(result.is_ok()); // graceful fallback
    }

    // ── mDNS discovery tests ──────────────────────────────────────────────

    #[test]
    fn parse_mdns_services_typical_output() {
        let stdout = "List of discovered mdns services\n\
                       adb-ABC123\t_adb-tls-connect._tcp.\t192.168.1.100:43567\n\
                       adb-DEF456\t_adb-tls-pairing._tcp.\t192.168.1.101:37215\n";
        let services = parse_mdns_services(stdout);
        assert_eq!(services.len(), 2);
        assert_eq!(services[0].name, "adb-ABC123");
        assert_eq!(services[0].service_type, "_adb-tls-connect._tcp.");
        assert_eq!(services[0].ip_port, "192.168.1.100:43567");
        assert_eq!(services[1].name, "adb-DEF456");
        assert!(services[1].service_type.contains("pairing"));
    }

    #[test]
    fn parse_mdns_services_empty() {
        let stdout = "List of discovered mdns services\n";
        let services = parse_mdns_services(stdout);
        assert!(services.is_empty());
    }

    #[test]
    fn parse_mdns_services_no_tabs() {
        let stdout = "some random output with no tabs\n";
        let services = parse_mdns_services(stdout);
        assert!(services.is_empty());
    }

    #[test]
    fn parse_mdns_services_single_connect() {
        let stdout = "List of discovered mdns services\n\
                       adb-PIXEL7\t_adb-tls-connect._tcp.\t192.168.0.42:5555\n";
        let services = parse_mdns_services(stdout);
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].name, "adb-PIXEL7");
        assert_eq!(services[0].ip_port, "192.168.0.42:5555");
    }
}
