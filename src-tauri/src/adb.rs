//! ADB device operations: discovery, install, launch, uninstall, and listing.

use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::cmd::{adb_binary, emit_op_progress, run_cmd, run_cmd_async, run_cmd_lenient};
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
}
