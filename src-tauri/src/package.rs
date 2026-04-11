//! Package name and metadata extraction from APK and AAB files.

use serde::Serialize;
use std::env;
use std::fs;
use std::io::Read;
use std::path::PathBuf;

use crate::cmd::run_cmd_lenient;

// ─── Data Types ──────────────────────────────────────────────────────────────

/// Metadata extracted from an APK or AAB file.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PackageMetadata {
    pub package_name: Option<String>,
    pub version_name: Option<String>,
    pub version_code: Option<String>,
    pub min_sdk: Option<String>,
    pub target_sdk: Option<String>,
    pub permissions: Vec<String>,
    pub file_size: u64,
}

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

/// Extract metadata (version, SDK levels, permissions) from an APK file.
/// Primary: parse binary AndroidManifest.xml from the ZIP.
/// Fallback: use aapt2 `dump badging` for richer data.
#[tauri::command]
pub(crate) fn get_apk_metadata(apk_path: String) -> Result<PackageMetadata, String> {
    let file_size = fs::metadata(&apk_path).map(|m| m.len()).unwrap_or(0);

    // Try aapt2/aapt first — gives the richest metadata
    if let Some(meta) = try_aapt_metadata(&apk_path, file_size) {
        return Ok(meta);
    }

    // Fallback: parse binary manifest from APK directly
    let mut meta = PackageMetadata { file_size, ..Default::default() };
    if let Ok(info) = extract_manifest_from_apk(&apk_path) {
        meta.package_name = info.package_name;
        meta.version_name = info.version_name;
        meta.version_code = info.version_code;
        meta.min_sdk = info.min_sdk;
        meta.target_sdk = info.target_sdk;
        meta.permissions = info.permissions;
    }
    Ok(meta)
}

/// Extract metadata from an AAB file using `bundletool dump manifest`.
#[tauri::command]
pub(crate) fn get_aab_metadata(
    aab_path: String,
    java_path: String,
    bundletool_path: String,
) -> Result<PackageMetadata, String> {
    let file_size = fs::metadata(&aab_path).map(|m| m.len()).unwrap_or(0);
    let bundle_arg = format!("--bundle={}", aab_path);
    let args = vec!["-jar", &bundletool_path, "dump", "manifest", &bundle_arg];

    let (stdout, stderr, success) = run_cmd_lenient(&java_path, &args)?;
    if !success && stdout.trim().is_empty() {
        return Err(format!("bundletool dump manifest failed:\n{}", stderr.trim()));
    }

    let mut meta = PackageMetadata { file_size, ..Default::default() };
    meta.package_name = parse_package_from_xml(&stdout);
    meta.version_name = parse_xml_attr(&stdout, "versionName");
    meta.version_code = parse_xml_attr(&stdout, "versionCode");
    meta.min_sdk = parse_xml_attr(&stdout, "minSdkVersion");
    meta.target_sdk = parse_xml_attr(&stdout, "targetSdkVersion");
    meta.permissions = parse_xml_permissions(&stdout);
    Ok(meta)
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

// ─── Metadata Parsing Helpers ────────────────────────────────────────────────

/// Internal struct for raw manifest fields.
struct ManifestInfo {
    package_name: Option<String>,
    version_name: Option<String>,
    version_code: Option<String>,
    min_sdk: Option<String>,
    target_sdk: Option<String>,
    permissions: Vec<String>,
}

/// Look up an attribute by local name, handling optional namespace prefixes.
/// e.g. `find_attr(attrs, "versionName")` matches `"versionName"` or `"android:versionName"`.
fn find_attr(attrs: &indexmap::IndexMap<String, String>, name: &str) -> Option<String> {
    // Exact match first
    if let Some(v) = attrs.get(name) {
        if !v.is_empty() { return Some(v.clone()); }
    }
    // Then check for namespace-prefixed key (e.g. "android:versionName")
    let suffix = format!(":{}", name);
    for (k, v) in attrs.iter() {
        if k.ends_with(&suffix) && !v.is_empty() {
            return Some(v.clone());
        }
    }
    None
}

/// Parse binary AndroidManifest.xml from an APK and extract all metadata fields.
/// Uses the axmldecoder structured DOM API to traverse elements and attributes.
fn extract_manifest_from_apk(apk_path: &str) -> Result<ManifestInfo, String> {
    let file = fs::File::open(apk_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut manifest = archive.by_name("AndroidManifest.xml").map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    manifest.read_to_end(&mut buf).map_err(|e| e.to_string())?;

    let doc = axmldecoder::parse(&buf).map_err(|e| e.to_string())?;

    let mut info = ManifestInfo {
        package_name: None,
        version_name: None,
        version_code: None,
        min_sdk: None,
        target_sdk: None,
        permissions: Vec::new(),
    };

    if let Some(axmldecoder::Node::Element(root)) = doc.get_root() {
        let attrs = root.get_attributes();

        // Root <manifest> attributes
        info.package_name = find_attr(attrs, "package");
        info.version_name = find_attr(attrs, "versionName");
        info.version_code = find_attr(attrs, "versionCode");

        // Traverse children for <uses-sdk> and <uses-permission>
        for child in root.get_children() {
            if let axmldecoder::Node::Element(el) = child {
                let tag = el.get_tag();
                let child_attrs = el.get_attributes();

                if tag == "uses-sdk" {
                    if info.min_sdk.is_none() {
                        info.min_sdk = find_attr(child_attrs, "minSdkVersion");
                    }
                    if info.target_sdk.is_none() {
                        info.target_sdk = find_attr(child_attrs, "targetSdkVersion");
                    }
                } else if tag == "uses-permission" {
                    if let Some(perm) = find_attr(child_attrs, "name") {
                        if !info.permissions.contains(&perm) {
                            info.permissions.push(perm);
                        }
                    }
                }
            }
        }
    }

    Ok(info)
}

/// Try aapt2/aapt to extract full metadata from an APK.
fn try_aapt_metadata(apk_path: &str, file_size: u64) -> Option<PackageMetadata> {
    for var in &["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Ok(sdk) = env::var(var) {
            let build_tools = PathBuf::from(&sdk).join("build-tools");
            if !build_tools.exists() { continue; }

            let mut versions: Vec<_> = fs::read_dir(&build_tools).ok()?
                .filter_map(|e| e.ok()).collect();
            versions.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

            for version_dir in versions {
                for tool in &["aapt2", "aapt"] {
                    let tool_path = version_dir.path().join(tool);
                    if !tool_path.exists() { continue; }

                    if let Ok((stdout, _, _)) = run_cmd_lenient(
                        tool_path.to_str().unwrap_or(""), &["dump", "badging", apk_path],
                    ) {
                        return Some(parse_aapt_metadata(&stdout, file_size));
                    }
                }
            }
        }
    }
    None
}

/// Parse full metadata from aapt/aapt2 `dump badging` output.
fn parse_aapt_metadata(output: &str, file_size: u64) -> PackageMetadata {
    let mut meta = PackageMetadata { file_size, ..Default::default() };

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("package:") {
            meta.package_name = extract_aapt_value(trimmed, "name");
            meta.version_code = extract_aapt_value(trimmed, "versionCode");
            meta.version_name = extract_aapt_value(trimmed, "versionName");
        } else if trimmed.starts_with("sdkVersion:") {
            meta.min_sdk = extract_aapt_quoted(trimmed);
        } else if trimmed.starts_with("targetSdkVersion:") {
            meta.target_sdk = extract_aapt_quoted(trimmed);
        } else if trimmed.starts_with("uses-permission:") {
            if let Some(perm) = extract_aapt_value(trimmed, "name") {
                meta.permissions.push(perm);
            }
        }
    }
    meta
}

/// Extract a named value like `name='com.example'` from an aapt line.
fn extract_aapt_value(line: &str, key: &str) -> Option<String> {
    let pattern = format!("{}='", key);
    if let Some(start) = line.find(&pattern) {
        let rest = &line[start + pattern.len()..];
        if let Some(end) = rest.find('\'') {
            let val = rest[..end].to_string();
            if !val.is_empty() { return Some(val); }
        }
    }
    None
}

/// Extract the first single-quoted value from an aapt line (e.g. `sdkVersion:'21'`).
fn extract_aapt_quoted(line: &str) -> Option<String> {
    if let Some(start) = line.find('\'') {
        let rest = &line[start + 1..];
        if let Some(end) = rest.find('\'') {
            let val = rest[..end].to_string();
            if !val.is_empty() { return Some(val); }
        }
    }
    None
}

/// Parse `android:name="..."` from `<uses-permission>` elements in XML.
fn parse_xml_permissions(xml: &str) -> Vec<String> {
    let mut perms = Vec::new();
    for line in xml.lines() {
        let trimmed = line.trim();
        if trimmed.contains("uses-permission") {
            // Try android:name="value" and android:name='value'
            for sep in &['"', '\''] {
                let pattern = format!("android:name={}", sep);
                if let Some(start) = trimmed.find(&pattern) {
                    let rest = &trimmed[start + pattern.len()..];
                    if let Some(end) = rest.find(*sep) {
                        let perm = rest[..end].to_string();
                        if !perm.is_empty() && !perms.contains(&perm) {
                            perms.push(perm);
                        }
                    }
                }
            }
        }
    }
    perms
}

/// Parse `android:attrName="value"` from XML text.
fn parse_xml_attr(xml: &str, attr: &str) -> Option<String> {
    let patterns = [
        format!("android:{}=\"", attr),
        format!("android:{}='", attr),
        format!("{}=\"", attr),
        format!("{}='", attr),
    ];
    for line in xml.lines() {
        let trimmed = line.trim();
        for pat in &patterns {
            if let Some(start) = trimmed.find(pat.as_str()) {
                let sep = pat.chars().last().unwrap();
                let rest = &trimmed[start + pat.len()..];
                if let Some(end) = rest.find(sep) {
                    let val = rest[..end].trim().to_string();
                    if !val.is_empty() {
                        return Some(val);
                    }
                }
            }
        }
    }
    None
}

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

    // ── aapt metadata parsing ─────────────────────────────────────────────

    #[test]
    fn parse_aapt_metadata_full() {
        let output = "package: name='com.example.app' versionCode='42' versionName='2.1.0'\n\
                       sdkVersion:'21'\n\
                       targetSdkVersion:'34'\n\
                       uses-permission: name='android.permission.INTERNET'\n\
                       uses-permission: name='android.permission.CAMERA'\n\
                       application-label:'My App'";
        let meta = parse_aapt_metadata(output, 1024);
        assert_eq!(meta.package_name, Some("com.example.app".into()));
        assert_eq!(meta.version_code, Some("42".into()));
        assert_eq!(meta.version_name, Some("2.1.0".into()));
        assert_eq!(meta.min_sdk, Some("21".into()));
        assert_eq!(meta.target_sdk, Some("34".into()));
        assert_eq!(meta.permissions.len(), 2);
        assert!(meta.permissions.contains(&"android.permission.INTERNET".into()));
        assert_eq!(meta.file_size, 1024);
    }

    #[test]
    fn parse_aapt_metadata_minimal() {
        let output = "package: name='com.minimal' versionCode='1' versionName='1.0'";
        let meta = parse_aapt_metadata(output, 0);
        assert_eq!(meta.package_name, Some("com.minimal".into()));
        assert!(meta.permissions.is_empty());
        assert_eq!(meta.min_sdk, None);
    }

    // ── XML attribute & permission parsing ─────────────────────────────────

    #[test]
    fn parse_xml_attr_double_quote() {
        let xml = r#"<manifest android:versionName="3.0" android:versionCode="10">"#;
        assert_eq!(parse_xml_attr(xml, "versionName"), Some("3.0".into()));
        assert_eq!(parse_xml_attr(xml, "versionCode"), Some("10".into()));
    }

    #[test]
    fn parse_xml_attr_not_found() {
        let xml = "<manifest package=\"com.test\">";
        assert_eq!(parse_xml_attr(xml, "versionName"), None);
    }

    #[test]
    fn parse_xml_permissions_extracts_all() {
        let xml = "<uses-permission android:name=\"android.permission.INTERNET\"/>\n\
                    <uses-permission android:name=\"android.permission.WRITE_EXTERNAL_STORAGE\"/>";
        let perms = parse_xml_permissions(xml);
        assert_eq!(perms.len(), 2);
        assert!(perms.contains(&"android.permission.INTERNET".into()));
    }

    #[test]
    fn parse_xml_permissions_empty() {
        let perms = parse_xml_permissions("<manifest></manifest>");
        assert!(perms.is_empty());
    }

    #[test]
    fn extract_aapt_value_finds_name() {
        let line = "package: name='com.test.app' versionCode='1'";
        assert_eq!(extract_aapt_value(line, "name"), Some("com.test.app".into()));
    }

    #[test]
    fn extract_aapt_quoted_finds_value() {
        assert_eq!(extract_aapt_quoted("sdkVersion:'21'"), Some("21".into()));
    }
}
