//! Package name extraction from APK and AAB files.

use std::env;
use std::fs;
use std::io::Read;
use std::path::PathBuf;

use crate::cmd::run_cmd_lenient;

// ─── Tauri Commands ──────────────────────────────────────────────────────────

/// Return the size of a file in bytes.
#[tauri::command]
pub(crate) fn get_file_size(path: String) -> Result<u64, String> {
    std::fs::metadata(&path)
        .map(|m| m.len())
        .map_err(|e| format!("Failed to read file size: {}", e))
}

/// Extract the package name from an APK file by parsing its binary AndroidManifest.xml.
/// No external tools required — reads the APK as a ZIP and decodes the manifest directly.
/// Falls back to aapt2/aapt if available.
#[tauri::command]
pub(crate) fn get_package_name(apk_path: String) -> Result<String, String> {
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
pub(crate) fn get_aab_package_name(
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

// ─── Parsing Helpers ─────────────────────────────────────────────────────────

/// Parse the AndroidManifest.xml directly from an APK (ZIP) file using axmldecoder.
pub(crate) fn extract_package_from_apk(apk_path: &str) -> Result<String, String> {
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
pub(crate) fn parse_package_from_xml(xml: &str) -> Option<String> {
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
pub(crate) fn parse_package_from_aapt(output: &str) -> Option<String> {
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

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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

    // ── extract_package_from_apk error cases ─────────────────────────────

    #[test]
    fn extract_package_from_nonexistent_apk() {
        let result = extract_package_from_apk("/nonexistent/path/test.apk");
        assert!(result.is_err());
    }

    #[test]
    fn extract_package_from_invalid_file() {
        let tmp = std::env::temp_dir().join("test_not_an_apk.txt");
        std::fs::write(&tmp, "not a zip file").unwrap();
        let result = extract_package_from_apk(tmp.to_str().unwrap());
        assert!(result.is_err());
        let _ = std::fs::remove_file(&tmp);
    }
}
