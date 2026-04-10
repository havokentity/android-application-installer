//! Java and bundletool discovery, plus keytool operations.

use std::env;
use std::path::{Path, PathBuf};

use crate::cmd::{java_binary, run_cmd_lenient};
use crate::tools;

// ─── Tauri Commands ──────────────────────────────────────────────────────────

/// Check if Java is available and return its path + version info.
/// Checks the app-managed JRE first, then JAVA_HOME, then PATH.
#[tauri::command]
pub(crate) fn check_java(app: tauri::AppHandle) -> Result<String, String> {
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
pub(crate) fn find_bundletool(app: tauri::AppHandle) -> Result<String, String> {
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
pub(crate) fn list_key_aliases(
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
