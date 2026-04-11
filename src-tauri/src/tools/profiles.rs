//! Signing profile presets: save named keystore + password + alias configs.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

use super::paths::get_data_dir;

// ─── Types ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SigningProfile {
    pub name: String,
    pub keystore_path: String,
    pub keystore_pass: String,
    pub key_alias: String,
    pub key_pass: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SigningProfilesConfig {
    #[serde(default)]
    pub profiles: Vec<SigningProfile>,
}

// ─── Persistence ─────────────────────────────────────────────────────────────

fn profiles_path(data_dir: &Path) -> PathBuf {
    data_dir.join("signing_profiles.json")
}

fn load_profiles(data_dir: &Path) -> SigningProfilesConfig {
    let path = profiles_path(data_dir);
    if !path.exists() {
        return SigningProfilesConfig::default();
    }
    match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => SigningProfilesConfig::default(),
    }
}

fn save_profiles(data_dir: &Path, config: &SigningProfilesConfig) -> Result<(), String> {
    let path = profiles_path(data_dir);
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize signing profiles: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write signing profiles: {}", e))?;
    Ok(())
}

// ─── Tauri Commands ──────────────────────────────────────────────────────────

/// Get all saved signing profiles.
#[tauri::command]
pub(crate) fn get_signing_profiles(app: AppHandle) -> Result<Vec<SigningProfile>, String> {
    let data_dir = get_data_dir(&app)?;
    Ok(load_profiles(&data_dir).profiles)
}

/// Save a signing profile (upserts by name).
#[tauri::command]
pub(crate) fn save_signing_profile(
    app: AppHandle,
    profile: SigningProfile,
) -> Result<Vec<SigningProfile>, String> {
    let data_dir = get_data_dir(&app)?;
    let mut config = load_profiles(&data_dir);

    // Upsert: replace existing profile with same name, or append
    if let Some(existing) = config.profiles.iter_mut().find(|p| p.name == profile.name) {
        *existing = profile;
    } else {
        config.profiles.push(profile);
    }

    // Sort alphabetically by name
    config.profiles.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    save_profiles(&data_dir, &config)?;
    Ok(config.profiles)
}

/// Delete a signing profile by name.
#[tauri::command]
pub(crate) fn delete_signing_profile(
    app: AppHandle,
    name: String,
) -> Result<Vec<SigningProfile>, String> {
    let data_dir = get_data_dir(&app)?;
    let mut config = load_profiles(&data_dir);
    config.profiles.retain(|p| p.name != name);
    save_profiles(&data_dir, &config)?;
    Ok(config.profiles)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signing_profile_serializes() {
        let p = SigningProfile {
            name: "Debug".into(),
            keystore_path: "/path/to/debug.jks".into(),
            keystore_pass: "pass".into(),
            key_alias: "debug".into(),
            key_pass: "keypass".into(),
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("keystorePath"));
        assert!(json.contains("keyAlias"));
    }

    #[test]
    fn signing_profile_deserializes_camel_case() {
        let json = r#"{"name":"Test","keystorePath":"/a","keystorePass":"b","keyAlias":"c","keyPass":"d"}"#;
        let p: SigningProfile = serde_json::from_str(json).unwrap();
        assert_eq!(p.name, "Test");
        assert_eq!(p.keystore_path, "/a");
    }

    #[test]
    fn profiles_config_default_is_empty() {
        let c = SigningProfilesConfig::default();
        assert!(c.profiles.is_empty());
    }

    #[test]
    fn save_and_load_profiles_round_trip() {
        let tmp = std::env::temp_dir().join("test_profiles_rt");
        let _ = fs::create_dir_all(&tmp);
        let config = SigningProfilesConfig {
            profiles: vec![SigningProfile {
                name: "Release".into(),
                keystore_path: "/ks".into(),
                keystore_pass: "pw".into(),
                key_alias: "alias".into(),
                key_pass: "kp".into(),
            }],
        };
        save_profiles(&tmp, &config).unwrap();
        let loaded = load_profiles(&tmp);
        assert_eq!(loaded.profiles.len(), 1);
        assert_eq!(loaded.profiles[0].name, "Release");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn load_profiles_handles_corrupt_json() {
        let tmp = std::env::temp_dir().join("test_profiles_corrupt");
        let _ = fs::create_dir_all(&tmp);
        fs::write(tmp.join("signing_profiles.json"), "not json!").unwrap();
        let loaded = load_profiles(&tmp);
        assert!(loaded.profiles.is_empty());
        let _ = fs::remove_dir_all(&tmp);
    }
}


