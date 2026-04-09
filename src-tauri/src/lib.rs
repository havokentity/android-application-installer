use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod tools;

// ─── Data Types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub serial: String,
    pub state: String,
    pub model: String,
    pub product: String,
    pub transport_id: String,
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

pub(crate) fn adb_binary() -> &'static str {
    if cfg!(target_os = "windows") {
        "adb.exe"
    } else {
        "adb"
    }
}

pub(crate) fn java_binary() -> &'static str {
    if cfg!(target_os = "windows") {
        "java.exe"
    } else {
        "java"
    }
}

/// Run an external command and return (stdout, stderr).
/// Returns Err if the process fails to start or exits with non-zero status.
fn run_cmd(program: &str, args: &[&str]) -> Result<(String, String), String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run '{}': {}", program, e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok((stdout, stderr))
    } else {
        Err(format!(
            "Command '{}' failed (exit {}):\n{}\n{}",
            program,
            output.status.code().unwrap_or(-1),
            stdout.trim(),
            stderr.trim()
        ))
    }
}

/// Same as run_cmd but doesn't fail on non-zero exit (some tools like aapt2
/// return non-zero but still produce useful output).
fn run_cmd_lenient(program: &str, args: &[&str]) -> Result<(String, String, bool), String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run '{}': {}", program, e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    Ok((stdout, stderr, output.status.success()))
}

// ─── Tauri Commands ───────────────────────────────────────────────────────────

/// Auto-detect ADB binary path.
/// Checks the app's managed platform-tools FIRST, then falls back to system locations.
#[tauri::command]
fn find_adb(app: tauri::AppHandle) -> Result<String, String> {
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
fn get_devices(adb_path: String) -> Result<Vec<DeviceInfo>, String> {
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

/// Install an APK onto the specified device.
#[tauri::command]
fn install_apk(adb_path: String, device: String, apk_path: String) -> Result<String, String> {
    let (stdout, stderr) = run_cmd(&adb_path, &["-s", &device, "install", "-r", &apk_path])?;

    // adb install can succeed (exit 0) but still report Failure in stdout
    if stdout.contains("Failure") || stderr.contains("Failure") {
        return Err(format!(
            "APK install failed:\n{}\n{}",
            stdout.trim(),
            stderr.trim()
        ));
    }

    Ok(format!("APK installed successfully.\n{}", stdout.trim()))
}

/// Install an AAB file via bundletool (build-apks → install-apks).
#[tauri::command]
fn install_aab(
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

    let args_ref: Vec<&str> = build_args.iter().map(|s| s.as_str()).collect();
    run_cmd(&java_path, &args_ref)
        .map_err(|e| format!("bundletool build-apks failed:\n{}", e))?;

    // 3. Install the .apks set onto the device
    let install_args: Vec<String> = vec![
        "-jar".into(),
        bundletool_path,
        "install-apks".into(),
        format!("--apks={}", apks_str),
        format!("--device-id={}", device),
        format!("--adb={}", adb_path),
    ];

    let inst_ref: Vec<&str> = install_args.iter().map(|s| s.as_str()).collect();
    let (inst_out, _) = run_cmd(&java_path, &inst_ref)
        .map_err(|e| format!("bundletool install-apks failed:\n{}", e))?;

    // 4. Cleanup
    let _ = fs::remove_file(&apks_path);

    Ok(format!("AAB installed successfully.\n{}", inst_out.trim()))
}

/// Launch an installed app on the device by package name.
#[tauri::command]
fn launch_app(adb_path: String, device: String, package_name: String) -> Result<String, String> {
    let (stdout, stderr) = run_cmd(
        &adb_path,
        &["-s", &device, "shell", "monkey", "-p", &package_name, "1"],
    )?;

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

/// Extract the package name from an APK using aapt2 (falls back to aapt).
#[tauri::command]
fn get_package_name(apk_path: String) -> Result<String, String> {
    for var in &["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Ok(sdk) = env::var(var) {
            let build_tools = PathBuf::from(&sdk).join("build-tools");
            if !build_tools.exists() {
                continue;
            }

            let mut versions: Vec<_> = fs::read_dir(&build_tools)
                .map_err(|e| e.to_string())?
                .filter_map(|e| e.ok())
                .collect();
            versions.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

            for version_dir in versions {
                for tool in &["aapt2", "aapt"] {
                    let tool_path = version_dir.path().join(tool);
                    if !tool_path.exists() {
                        continue;
                    }

                    if let Ok((stdout, _, _)) = run_cmd_lenient(
                        tool_path.to_str().unwrap_or(""),
                        &["dump", "badging", &apk_path],
                    ) {
                        for line in stdout.lines() {
                            if line.starts_with("package:") {
                                if let Some(start) = line.find("name='") {
                                    let rest = &line[start + 6..];
                                    if let Some(end) = rest.find('\'') {
                                        return Ok(rest[..end].to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Err("Could not extract package name. Ensure Android SDK build-tools are installed.".into())
}

/// Check if Java is available and return its path + version info.
/// Checks the app-managed JRE first, then JAVA_HOME, then PATH.
#[tauri::command]
fn check_java(app: tauri::AppHandle) -> Result<String, String> {
    // 0. Check app-managed JRE (highest priority)
    if let Ok(data_dir) = tools::get_data_dir(&app) {
        if let Some(java) = tools::managed_java_path(&data_dir) {
            let java_str = java.to_string_lossy().to_string();
            if let Ok((_, stderr, _)) = run_cmd_lenient(&java_str, &["-version"]) {
                let version = stderr
                    .lines()
                    .next()
                    .unwrap_or("unknown version")
                    .to_string();
                return Ok(format!("{}|{}", java_str, version));
            }
        }
    }

    // 1. Check JAVA_HOME
    if let Ok(java_home) = env::var("JAVA_HOME") {
        let java = PathBuf::from(&java_home).join("bin").join(java_binary());
        if java.exists() {
            let java_str = java.to_string_lossy().to_string();
            if let Ok((_, stderr, _)) = run_cmd_lenient(&java_str, &["-version"]) {
                let version = stderr
                    .lines()
                    .next()
                    .unwrap_or("unknown version")
                    .to_string();
                return Ok(format!("{}|{}", java_str, version));
            }
        }
    }

    // 2. Try java on PATH
    match run_cmd_lenient("java", &["-version"]) {
        Ok((_, stderr, true)) => {
            let version = stderr
                .lines()
                .next()
                .unwrap_or("unknown version")
                .to_string();
            Ok(format!("java|{}", version))
        }
        _ => Err("Java not found. Use the \"Download\" button in the Tools section to install it automatically.".into()),
    }
}

/// Find bundletool.jar.
/// Checks the app's managed location FIRST, then falls back to system locations.
#[tauri::command]
fn find_bundletool(app: tauri::AppHandle) -> Result<String, String> {
    // 0. Check app-managed bundletool (highest priority)
    if let Ok(data_dir) = tools::get_data_dir(&app) {
        let managed = tools::managed_bundletool_path(&data_dir);
        if managed.exists() {
            return Ok(managed.to_string_lossy().into());
        }
    }

    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .unwrap_or_default();

    let mut candidates = vec![
        PathBuf::from(&home).join("bundletool.jar"),
        PathBuf::from(&home).join(".android").join("bundletool.jar"),
        PathBuf::from(&home)
            .join("Library")
            .join("Android")
            .join("bundletool.jar"),
        PathBuf::from(&home)
            .join("Android")
            .join("bundletool.jar"),
    ];

    for var in &["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Ok(sdk) = env::var(var) {
            candidates.push(PathBuf::from(&sdk).join("bundletool.jar"));
        }
    }

    for c in &candidates {
        if c.exists() {
            return Ok(c.to_string_lossy().into());
        }
    }

    Err("bundletool.jar not found. Use the \"Download\" button to install it automatically.".into())
}

/// Uninstall an app from the device by package name.
#[tauri::command]
fn uninstall_app(adb_path: String, device: String, package_name: String) -> Result<String, String> {
    let (stdout, stderr) =
        run_cmd(&adb_path, &["-s", &device, "uninstall", &package_name])?;

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

/// List third-party installed packages on the device.
#[tauri::command]
fn list_packages(adb_path: String, device: String) -> Result<Vec<String>, String> {
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

// ─── App Entry Point ──────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            find_adb,
            get_devices,
            install_apk,
            install_aab,
            launch_app,
            get_package_name,
            check_java,
            find_bundletool,
            uninstall_app,
            list_packages,
            tools::get_tools_status,
            tools::setup_platform_tools,
            tools::setup_bundletool,
            tools::setup_java,
            tools::check_for_stale_tools,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
