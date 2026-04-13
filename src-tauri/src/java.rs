//! Java and bundletool discovery, plus keytool operations.

use std::env;
use std::path::{Path, PathBuf};

use crate::cmd::{java_binary, run_cmd_lenient};
use crate::tools;

// ─── Private Helpers (extracted for testability) ──────────────────────────────

fn keytool_binary() -> &'static str {
    if cfg!(target_os = "windows") {
        "keytool.exe"
    } else {
        "keytool"
    }
}

/// Derives the expected keytool binary path from a java binary path.
/// Returns the sibling `keytool[.exe]` in the same `bin/` directory.
/// Does NOT check whether the path exists — caller is responsible for that.
fn derive_keytool_path(java_path: &str) -> PathBuf {
    Path::new(java_path)
        .parent()
        .map(|bin_dir| bin_dir.join(keytool_binary()))
        .unwrap_or_else(|| PathBuf::from(keytool_binary()))
}

/// Returns the standard candidate locations where `bundletool.jar` might live
/// for a given home directory string.
fn bundletool_candidate_paths(home: &str) -> Vec<PathBuf> {
    vec![
        PathBuf::from(home).join("bundletool.jar"),
        PathBuf::from(home).join(".android").join("bundletool.jar"),
        PathBuf::from(home)
            .join("Library")
            .join("Android")
            .join("bundletool.jar"),
        PathBuf::from(home).join("Android").join("bundletool.jar"),
    ]
}

/// Parses key alias names from `keytool -list` stdout/stderr output.
/// Lines that mention a known entry type (`PrivateKeyEntry`, `trustedCertEntry`,
/// `SecretKeyEntry`, `keyEntry`) are expected to start with `<alias>, <date>, <type>,`.
fn parse_keytool_aliases(stdout: &str, stderr: &str) -> Vec<String> {
    const ENTRY_TYPES: &[&str] = &[
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
        if ENTRY_TYPES.iter().any(|et| trimmed.contains(et)) {
            if let Some(comma_pos) = trimmed.find(',') {
                let alias = trimmed[..comma_pos].trim().to_string();
                if !alias.is_empty() {
                    aliases.push(alias);
                }
            }
        }
    }
    aliases
}

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

    let mut candidates = bundletool_candidate_paths(&home);

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
    let derived = derive_keytool_path(&java_path);
    let keytool_path = if derived.exists() {
        derived.to_string_lossy().to_string()
    } else {
        // Fallback: try keytool on PATH
        keytool_binary().to_string()
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
    let aliases = parse_keytool_aliases(&stdout, &stderr);

    if aliases.is_empty() {
        return Err("No key aliases found in the keystore.".into());
    }

    Ok(aliases)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_keytool_aliases ─────────────────────────────────────────────────

    #[test]
    fn parse_aliases_single_private_key_entry() {
        let stdout = "Keystore type: PKCS12\n\
                      myalias, Apr 10, 2026, PrivateKeyEntry,\n\
                      Certificate fingerprint: SHA-256: AA:BB\n";
        let aliases = parse_keytool_aliases(stdout, "");
        assert_eq!(aliases, vec!["myalias"]);
    }

    #[test]
    fn parse_aliases_multiple_entry_types() {
        let stdout = "androiddebugkey, Jan 1, 2024, PrivateKeyEntry,\n\
                      rootca, Mar 5, 2025, trustedCertEntry,\n\
                      secretkey, Dec 1, 2023, SecretKeyEntry,\n";
        let aliases = parse_keytool_aliases(stdout, "");
        assert_eq!(aliases, vec!["androiddebugkey", "rootca", "secretkey"]);
    }

    #[test]
    fn parse_aliases_from_stderr_only() {
        // Some JDK versions print the alias table to stderr
        let stderr = "release, Jan 1, 2025, PrivateKeyEntry,\n";
        let aliases = parse_keytool_aliases("", stderr);
        assert_eq!(aliases, vec!["release"]);
    }

    #[test]
    fn parse_aliases_both_stdout_and_stderr() {
        let stdout = "keyA, Jan 1, 2024, PrivateKeyEntry,\n";
        let stderr = "keyB, Jan 2, 2024, trustedCertEntry,\n";
        let aliases = parse_keytool_aliases(stdout, stderr);
        assert_eq!(aliases, vec!["keyA", "keyB"]);
    }

    #[test]
    fn parse_aliases_strips_whitespace_from_alias() {
        let stdout = "  spaced alias  , Apr 1, 2026, PrivateKeyEntry,\n";
        let aliases = parse_keytool_aliases(stdout, "");
        assert_eq!(aliases, vec!["spaced alias"]);
    }

    #[test]
    fn parse_aliases_no_matching_lines_returns_empty() {
        let stdout = "Keystore type: JKS\nKeystore provider: SUN\nYour keystore contains 0 entries\n";
        let aliases = parse_keytool_aliases(stdout, "");
        assert!(aliases.is_empty());
    }

    #[test]
    fn parse_aliases_empty_input_returns_empty() {
        assert!(parse_keytool_aliases("", "").is_empty());
    }

    #[test]
    fn parse_aliases_entry_type_line_without_comma_is_skipped() {
        // No comma → can't extract an alias; should not panic or produce garbage
        let stdout = "PrivateKeyEntry\n";
        let aliases = parse_keytool_aliases(stdout, "");
        assert!(aliases.is_empty());
    }

    #[test]
    fn parse_aliases_empty_alias_before_comma_is_skipped() {
        let stdout = ", Apr 1, 2026, PrivateKeyEntry,\n";
        let aliases = parse_keytool_aliases(stdout, "");
        assert!(aliases.is_empty());
    }

    #[test]
    fn parse_aliases_keyentry_type_is_recognised() {
        let stdout = "legacykey, May 5, 2020, keyEntry,\n";
        let aliases = parse_keytool_aliases(stdout, "");
        assert_eq!(aliases, vec!["legacykey"]);
    }

    // ── derive_keytool_path ───────────────────────────────────────────────────

    #[test]
    fn derive_keytool_path_uses_same_bin_dir() {
        // Given a java binary in a bin/ directory, keytool should be a sibling.
        let java = if cfg!(target_os = "windows") {
            r"C:\Program Files\Java\jdk-21\bin\java.exe"
        } else {
            "/usr/lib/jvm/java-21/bin/java"
        };
        let result = derive_keytool_path(java);
        let expected_name = keytool_binary();
        assert_eq!(result.file_name().unwrap().to_str().unwrap(), expected_name);

        // Parent directory must be the same bin/ dir as java
        let java_parent = Path::new(java).parent().unwrap();
        assert_eq!(result.parent().unwrap(), java_parent);
    }

    #[test]
    fn derive_keytool_path_bare_java_name_falls_back_to_bare_keytool() {
        // When java_path has no parent component the result is just the binary name.
        let result = derive_keytool_path("java");
        assert_eq!(
            result,
            PathBuf::from(keytool_binary()),
            "should fall back to bare keytool binary name"
        );
    }

    #[test]
    fn derive_keytool_path_returns_correct_extension_for_platform() {
        let result = derive_keytool_path("/some/bin/java");
        let name = result.file_name().unwrap().to_str().unwrap();
        if cfg!(target_os = "windows") {
            assert!(name.ends_with(".exe"), "expected .exe on Windows, got {name}");
        } else {
            assert!(!name.ends_with(".exe"), "unexpected .exe on non-Windows, got {name}");
        }
    }

    // ── bundletool_candidate_paths ────────────────────────────────────────────

    #[test]
    fn bundletool_candidates_contains_four_paths() {
        let candidates = bundletool_candidate_paths("/home/user");
        assert_eq!(candidates.len(), 4);
    }

    #[test]
    fn bundletool_candidates_home_root() {
        let candidates = bundletool_candidate_paths("/home/user");
        assert!(
            candidates.contains(&PathBuf::from("/home/user/bundletool.jar")),
            "expected home-root candidate"
        );
    }

    #[test]
    fn bundletool_candidates_android_hidden_dir() {
        let candidates = bundletool_candidate_paths("/home/user");
        assert!(
            candidates.contains(&PathBuf::from("/home/user/.android/bundletool.jar")),
            "expected ~/.android candidate"
        );
    }

    #[test]
    fn bundletool_candidates_library_android_dir() {
        let candidates = bundletool_candidate_paths("/home/user");
        assert!(
            candidates.contains(&PathBuf::from("/home/user/Library/Android/bundletool.jar")),
            "expected ~/Library/Android candidate (macOS)"
        );
    }

    #[test]
    fn bundletool_candidates_android_dir() {
        let candidates = bundletool_candidate_paths("/home/user");
        assert!(
            candidates.contains(&PathBuf::from("/home/user/Android/bundletool.jar")),
            "expected ~/Android candidate"
        );
    }

    #[test]
    fn bundletool_candidates_all_end_with_bundletool_jar() {
        for path in bundletool_candidate_paths("/home/user") {
            assert_eq!(
                path.file_name().unwrap().to_str().unwrap(),
                "bundletool.jar"
            );
        }
    }

    #[test]
    fn bundletool_candidates_empty_home_still_returns_four() {
        // Gracefully handles an empty home string
        let candidates = bundletool_candidate_paths("");
        assert_eq!(candidates.len(), 4);
    }
}
