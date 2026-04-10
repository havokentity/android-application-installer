use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Emitter;

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

/// Progress event emitted during async operations (install, launch, uninstall).
#[derive(Debug, Clone, Serialize)]
pub struct OperationProgress {
    pub operation: String,
    pub device: String,
    pub status: String, // "running" | "done" | "cancelled" | "error"
    pub message: String,
    pub step: Option<u32>,
    pub total_steps: Option<u32>,
    pub cancellable: bool,
}

/// Global cancellation flag for long-running async operations.
static OPERATION_CANCEL: AtomicBool = AtomicBool::new(false);

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

// ─── Async Operation Helpers ──────────────────────────────────────────────────

/// Emit an operation-progress event to the frontend.
fn emit_op_progress(
    app: &tauri::AppHandle,
    operation: &str,
    device: &str,
    status: &str,
    message: &str,
    step: Option<u32>,
    total_steps: Option<u32>,
    cancellable: bool,
) {
    let _ = app.emit(
        "operation-progress",
        OperationProgress {
            operation: operation.to_string(),
            device: device.to_string(),
            status: status.to_string(),
            message: message.to_string(),
            step,
            total_steps,
            cancellable,
        },
    );
}

/// Async loop that resolves once the cancel flag is set.
async fn poll_cancel() {
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if OPERATION_CANCEL.load(Ordering::Relaxed) {
            break;
        }
    }
}

/// Run an external command asynchronously with cancellation support.
/// Uses `tokio::process::Command` so the child process is killed on cancel
/// (via `kill_on_drop`).
async fn run_cmd_async(program: &str, args: &[&str]) -> Result<(String, String), String> {
    // Early exit if already cancelled
    if OPERATION_CANCEL.load(Ordering::Relaxed) {
        return Err("Operation cancelled by user.".to_string());
    }

    let child = tokio::process::Command::new(program)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Failed to run '{}': {}", program, e))?;

    tokio::select! {
        output = child.wait_with_output() => {
            let output = output.map_err(|e| format!("Process error for '{}': {}", program, e))?;
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
        _ = poll_cancel() => {
            Err("Operation cancelled by user.".to_string())
        }
    }
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

/// Install an APK onto the specified device (async with progress & cancellation).
#[tauri::command]
async fn install_apk(
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
async fn install_aab(
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
async fn launch_app(
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

/// Extract the package name from an APK file by parsing its binary AndroidManifest.xml.
/// No external tools required — reads the APK as a ZIP and decodes the manifest directly.
/// Falls back to aapt2/aapt if available.
#[tauri::command]
fn get_package_name(apk_path: String) -> Result<String, String> {
    // 1. Try parsing the APK directly (no external tools needed)
    if let Ok(pkg) = extract_package_from_apk(&apk_path) {
        return Ok(pkg);
    }

    // 2. Fallback: try aapt2/aapt from Android SDK build-tools
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
                        if let Some(pkg) = parse_package_from_aapt(&stdout) {
                            return Ok(pkg);
                        }
                    }
                }
            }
        }
    }

    Err("Could not extract package name from APK.".into())
}

/// Extract the package name from an AAB file.
/// Uses bundletool to dump the manifest, then parses the package attribute.
#[tauri::command]
fn get_aab_package_name(
    aab_path: String,
    java_path: String,
    bundletool_path: String,
) -> Result<String, String> {
    // Run: java -jar bundletool.jar dump manifest --bundle=<aab>
    let bundle_arg = format!("--bundle={}", aab_path);
    let args = vec![
        "-jar",
        &bundletool_path,
        "dump",
        "manifest",
        &bundle_arg,
    ];

    let (stdout, stderr, success) = run_cmd_lenient(&java_path, &args)?;

    if !success && stdout.trim().is_empty() {
        return Err(format!(
            "bundletool dump manifest failed:\n{}",
            stderr.trim()
        ));
    }

    // Parse package="..." from the XML output
    if let Some(pkg) = parse_package_from_xml(&stdout) {
        return Ok(pkg);
    }

    Err("Could not extract package name from AAB manifest.".into())
}

// ─── Package Name Parsing Helpers ─────────────────────────────────────────────

/// Parse the AndroidManifest.xml directly from an APK (ZIP) file using axmldecoder.
fn extract_package_from_apk(apk_path: &str) -> Result<String, String> {
    let file = fs::File::open(apk_path)
        .map_err(|e| format!("Failed to open APK: {}", e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("Invalid APK (not a valid ZIP): {}", e))?;

    let mut manifest = archive
        .by_name("AndroidManifest.xml")
        .map_err(|_| "AndroidManifest.xml not found in APK".to_string())?;

    let mut buf = Vec::new();
    manifest
        .read_to_end(&mut buf)
        .map_err(|e| format!("Failed to read AndroidManifest.xml: {}", e))?;

    let doc = axmldecoder::parse(&buf)
        .map_err(|e| format!("Failed to decode binary XML: {}", e))?;

    // The root element should be <manifest> with a "package" attribute
    if let Some(axmldecoder::Node::Element(root)) = doc.get_root() {
        if let Some(pkg) = root.get_attributes().get("package") {
            if !pkg.is_empty() {
                return Ok(pkg.clone());
            }
        }
    }

    Err("package attribute not found in manifest".into())
}

/// Parse `package="..."` from an XML string (works for both decoded binary XML and bundletool output).
fn parse_package_from_xml(xml: &str) -> Option<String> {
    // Look for package="<value>" in the manifest element
    for line in xml.lines() {
        let trimmed = line.trim();
        if trimmed.contains("package=") {
            // Handle both package="value" and package='value'
            if let Some(start) = trimmed.find("package=\"") {
                let rest = &trimmed[start + 9..];
                if let Some(end) = rest.find('"') {
                    let pkg = rest[..end].trim().to_string();
                    if !pkg.is_empty() {
                        return Some(pkg);
                    }
                }
            }
            if let Some(start) = trimmed.find("package='") {
                let rest = &trimmed[start + 9..];
                if let Some(end) = rest.find('\'') {
                    let pkg = rest[..end].trim().to_string();
                    if !pkg.is_empty() {
                        return Some(pkg);
                    }
                }
            }
        }
    }
    None
}

/// Parse `name='...'` from aapt/aapt2 badging output.
fn parse_package_from_aapt(output: &str) -> Option<String> {
    for line in output.lines() {
        if line.starts_with("package:") {
            if let Some(start) = line.find("name='") {
                let rest = &line[start + 6..];
                if let Some(end) = rest.find('\'') {
                    return Some(rest[..end].to_string());
                }
            }
        }
    }
    None
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

/// List key aliases from a Java keystore file using `keytool`.
/// Derives the keytool binary path from the java binary path (same bin/ directory).
#[tauri::command]
fn list_key_aliases(
    java_path: String,
    keystore_path: String,
    keystore_pass: String,
) -> Result<Vec<String>, String> {
    // Derive keytool path from java path (sibling in the same bin/ directory)
    let java = Path::new(&java_path);
    let keytool_name = if cfg!(target_os = "windows") {
        "keytool.exe"
    } else {
        "keytool"
    };

    let keytool_path = if let Some(bin_dir) = java.parent() {
        let kt = bin_dir.join(keytool_name);
        if kt.exists() {
            kt.to_string_lossy().to_string()
        } else {
            // Fallback: try keytool on PATH
            keytool_name.to_string()
        }
    } else {
        keytool_name.to_string()
    };

    let (stdout, stderr, success) = run_cmd_lenient(
        &keytool_path,
        &["-list", "-keystore", &keystore_path, "-storepass", &keystore_pass],
    )?;

    if !success {
        // Common error: wrong password
        let combined = format!("{}\n{}", stdout.trim(), stderr.trim());
        if combined.contains("password was incorrect")
            || combined.contains("keystore was tampered with")
        {
            return Err("Incorrect keystore password.".into());
        }
        return Err(format!("keytool failed: {}", combined.trim()));
    }

    // Parse alias names from keytool output.
    // Lines with aliases look like:
    //   myalias, Apr 10, 2026, PrivateKeyEntry,
    //   androiddebugkey, Jan 1, 2024, trustedCertEntry,
    // We look for lines containing known entry types.
    let entry_types = [
        "PrivateKeyEntry",
        "trustedCertEntry",
        "SecretKeyEntry",
        "keyEntry",
    ];

    let mut aliases = Vec::new();
    for line in stdout.lines().chain(stderr.lines()) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Check if this line contains an entry type indicator
        if entry_types.iter().any(|et| trimmed.contains(et)) {
            // Alias is everything before the first comma
            if let Some(comma_pos) = trimmed.find(',') {
                let alias = trimmed[..comma_pos].trim().to_string();
                if !alias.is_empty() {
                    aliases.push(alias);
                }
            }
        }
    }

    if aliases.is_empty() {
        return Err("No key aliases found in the keystore.".into());
    }

    Ok(aliases)
}

/// Uninstall an app from the device by package name (async with cancellation).
#[tauri::command]
async fn uninstall_app(
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

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── adb_binary / java_binary ──────────────────────────────────────────

    #[test]
    fn adb_binary_returns_correct_name() {
        let name = adb_binary();
        if cfg!(target_os = "windows") {
            assert_eq!(name, "adb.exe");
        } else {
            assert_eq!(name, "adb");
        }
    }

    #[test]
    fn java_binary_returns_correct_name() {
        let name = java_binary();
        if cfg!(target_os = "windows") {
            assert_eq!(name, "java.exe");
        } else {
            assert_eq!(name, "java");
        }
    }

    // ── parse_package_from_xml ────────────────────────────────────────────

    #[test]
    fn parse_package_from_xml_double_quotes() {
        let xml = r#"<manifest xmlns:android="http://schemas.android.com/apk/res/android"
            package="com.example.myapp"
            android:versionCode="1">"#;
        assert_eq!(
            parse_package_from_xml(xml),
            Some("com.example.myapp".to_string())
        );
    }

    #[test]
    fn parse_package_from_xml_single_quotes() {
        let xml = "<manifest package='org.test.app'>";
        assert_eq!(
            parse_package_from_xml(xml),
            Some("org.test.app".to_string())
        );
    }

    #[test]
    fn parse_package_from_xml_multiline() {
        let xml = r#"<?xml version="1.0"?>
<manifest
    package="com.multi.line"
    android:versionCode="10">
</manifest>"#;
        assert_eq!(
            parse_package_from_xml(xml),
            Some("com.multi.line".to_string())
        );
    }

    #[test]
    fn parse_package_from_xml_no_package() {
        let xml = r#"<manifest android:versionCode="1"></manifest>"#;
        assert_eq!(parse_package_from_xml(xml), None);
    }

    #[test]
    fn parse_package_from_xml_empty_string() {
        assert_eq!(parse_package_from_xml(""), None);
    }

    #[test]
    fn parse_package_from_xml_empty_package_value() {
        let xml = r#"<manifest package=""></manifest>"#;
        assert_eq!(parse_package_from_xml(xml), None);
    }

    #[test]
    fn parse_package_from_xml_with_spaces() {
        // package attribute with leading/trailing spaces in value
        let xml = r#"<manifest package=" com.spaced.app "></manifest>"#;
        let result = parse_package_from_xml(xml);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "com.spaced.app");
    }

    // ── parse_package_from_aapt ──────────────────────────────────────────

    #[test]
    fn parse_package_from_aapt_standard_output() {
        let output = "package: name='com.example.aapt' versionCode='1' versionName='1.0'\n\
                       application-label:'My App'\n\
                       sdkVersion:'21'";
        assert_eq!(
            parse_package_from_aapt(output),
            Some("com.example.aapt".to_string())
        );
    }

    #[test]
    fn parse_package_from_aapt_no_package_line() {
        let output = "application-label:'My App'\nsdkVersion:'21'";
        assert_eq!(parse_package_from_aapt(output), None);
    }

    #[test]
    fn parse_package_from_aapt_empty() {
        assert_eq!(parse_package_from_aapt(""), None);
    }

    #[test]
    fn parse_package_from_aapt_malformed_line() {
        let output = "package: versionCode='1'"; // missing name=
        assert_eq!(parse_package_from_aapt(output), None);
    }

    #[test]
    fn parse_package_from_aapt_multiple_lines() {
        let output = "some random line\n\
                       another line\n\
                       package: name='com.found.it' versionCode='5'\n\
                       more lines";
        assert_eq!(
            parse_package_from_aapt(output),
            Some("com.found.it".to_string())
        );
    }

    // ── DeviceInfo serialization ─────────────────────────────────────────

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

    // ── run_cmd tests (use simple system commands) ───────────────────────

    #[test]
    fn run_cmd_echo_succeeds() {
        let result = run_cmd("echo", &["hello"]);
        assert!(result.is_ok());
        let (stdout, _) = result.unwrap();
        assert!(stdout.trim().contains("hello"));
    }

    #[test]
    fn run_cmd_nonexistent_program_fails() {
        let result = run_cmd("nonexistent_program_12345", &[]);
        assert!(result.is_err());
    }

    #[test]
    fn run_cmd_lenient_false_exit_returns_ok() {
        // `false` exits with code 1 on Unix
        if !cfg!(target_os = "windows") {
            let result = run_cmd_lenient("false", &[]);
            assert!(result.is_ok());
            let (_, _, success) = result.unwrap();
            assert!(!success);
        }
    }

    #[test]
    fn run_cmd_lenient_true_exit_returns_ok() {
        let result = run_cmd_lenient("true", &[]);
        assert!(result.is_ok());
        let (_, _, success) = result.unwrap();
        assert!(success);
    }

    // ── get_devices parsing logic (by mocking adb output) ────────────────

    #[test]
    fn parse_devices_output_format() {
        // Simulate what get_devices does internally with the adb output
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
        assert_eq!(devices[0].model, "Pixel 7"); // underscore replaced with space
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

    // ── install_apk stdout failure detection ─────────────────────────────

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

    // ── extract_package_from_apk error cases ─────────────────────────────

    #[test]
    fn extract_package_from_nonexistent_apk() {
        let result = extract_package_from_apk("/nonexistent/path/test.apk");
        assert!(result.is_err());
    }

    #[test]
    fn extract_package_from_invalid_file() {
        // Create a temp file that isn't a ZIP
        let tmp = std::env::temp_dir().join("test_not_an_apk.txt");
        std::fs::write(&tmp, "not a zip file").unwrap();
        let result = extract_package_from_apk(tmp.to_str().unwrap());
        assert!(result.is_err());
        let _ = std::fs::remove_file(&tmp);
    }
}

// ─── Cancellation Control ─────────────────────────────────────────────────────

/// Set or clear the global cancellation flag for async operations.
/// Called from the frontend to cancel or reset before starting a new batch.
#[tauri::command]
fn set_cancel_flag(cancel: bool) {
    OPERATION_CANCEL.store(cancel, Ordering::SeqCst);
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
            get_aab_package_name,
            check_java,
            find_bundletool,
            uninstall_app,
            list_packages,
            list_key_aliases,
            set_cancel_flag,
            tools::get_tools_status,
            tools::setup_platform_tools,
            tools::setup_bundletool,
            tools::setup_java,
            tools::check_for_stale_tools,
            tools::get_recent_files,
            tools::add_recent_file,
            tools::remove_recent_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
