//! Signing profile presets: save named keystore + password + alias configs.
//! Passwords are encrypted at rest using AES-256-GCM with a machine-local key.
//! File-to-profile associations are tracked for auto-restore on re-selection.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    /// Maps file path → profile name for auto-restore.
    #[serde(default)]
    pub file_mappings: HashMap<String, String>,
}

// ─── Encryption ──────────────────────────────────────────────────────────────

/// Path to the machine-local encryption key.
fn key_path(data_dir: &Path) -> PathBuf {
    data_dir.join("signing_key.bin")
}

/// Get or create a 256-bit AES key. Generated once per machine, stored as raw bytes.
fn get_or_create_key(data_dir: &Path) -> Result<[u8; 32], String> {
    let path = key_path(data_dir);
    if path.exists() {
        let bytes = fs::read(&path).map_err(|e| format!("Failed to read encryption key: {}", e))?;
        if bytes.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            return Ok(key);
        }
        // Key file is corrupt — regenerate
    }
    // Generate a new key
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    // Ensure data dir exists
    let _ = fs::create_dir_all(data_dir);
    fs::write(&path, &key).map_err(|e| format!("Failed to write encryption key: {}", e))?;
    Ok(key)
}

/// Encrypt a plaintext string. Returns `hex(nonce):hex(ciphertext)`.
fn encrypt_field(key: &[u8; 32], plaintext: &str) -> Result<String, String> {
    if plaintext.is_empty() {
        return Ok(String::new());
    }
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("Encryption failed: {}", e))?;
    Ok(format!("{}:{}", hex_encode(&nonce_bytes), hex_encode(&ciphertext)))
}

/// Decrypt a `hex(nonce):hex(ciphertext)` string. Returns plaintext.
/// If the input doesn't look encrypted (no colon, or decryption fails),
/// returns the input unchanged — gracefully handles legacy plaintext profiles.
fn decrypt_field(key: &[u8; 32], encoded: &str) -> String {
    if encoded.is_empty() {
        return String::new();
    }
    let Some((nonce_hex, ct_hex)) = encoded.split_once(':') else {
        return encoded.to_string(); // Legacy plaintext
    };
    let Ok(nonce_bytes) = hex_decode(nonce_hex) else {
        return encoded.to_string();
    };
    let Ok(ciphertext) = hex_decode(ct_hex) else {
        return encoded.to_string();
    };
    if nonce_bytes.len() != 12 {
        return encoded.to_string();
    }
    let cipher = match Aes256Gcm::new_from_slice(key) {
        Ok(c) => c,
        Err(_) => return encoded.to_string(),
    };
    let nonce = Nonce::from_slice(&nonce_bytes);
    match cipher.decrypt(nonce, ciphertext.as_ref()) {
        Ok(plaintext) => String::from_utf8(plaintext).unwrap_or_else(|_| encoded.to_string()),
        Err(_) => encoded.to_string(), // Decryption failed — return as-is (legacy plaintext)
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_decode(hex: &str) -> Result<Vec<u8>, String> {
    if hex.len() % 2 != 0 {
        return Err("Invalid hex length".into());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

// ─── Persistence ─────────────────────────────────────────────────────────────

fn profiles_path(data_dir: &Path) -> PathBuf {
    data_dir.join("signing_profiles.json")
}

/// Load profiles from disk and decrypt password fields.
fn load_profiles(data_dir: &Path) -> SigningProfilesConfig {
    let path = profiles_path(data_dir);
    if !path.exists() {
        return SigningProfilesConfig::default();
    }
    let mut config: SigningProfilesConfig = match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => return SigningProfilesConfig::default(),
    };

    // Decrypt password fields
    if let Ok(key) = get_or_create_key(data_dir) {
        for p in &mut config.profiles {
            p.keystore_pass = decrypt_field(&key, &p.keystore_pass);
            p.key_pass = decrypt_field(&key, &p.key_pass);
        }
    }

    config
}

/// Encrypt password fields and save profiles to disk.
fn save_profiles(data_dir: &Path, config: &SigningProfilesConfig) -> Result<(), String> {
    let key = get_or_create_key(data_dir)?;

    // Encrypt password fields before serialization
    let mut encrypted = config.clone();
    for p in &mut encrypted.profiles {
        p.keystore_pass = encrypt_field(&key, &p.keystore_pass)?;
        p.key_pass = encrypt_field(&key, &p.key_pass)?;
    }

    let path = profiles_path(data_dir);
    let json = serde_json::to_string_pretty(&encrypted)
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
    // Also remove file mappings that reference the deleted profile
    config.file_mappings.retain(|_, v| v != &name);
    save_profiles(&data_dir, &config)?;
    Ok(config.profiles)
}

/// Get the signing profile name associated with a file path.
#[tauri::command]
pub(crate) fn get_profile_for_file(
    app: AppHandle,
    path: String,
) -> Result<Option<String>, String> {
    let data_dir = get_data_dir(&app)?;
    let config = load_profiles(&data_dir);
    match config.file_mappings.get(&path) {
        Some(name) => {
            // Only return if the profile still exists
            if config.profiles.iter().any(|p| &p.name == name) {
                Ok(Some(name.clone()))
            } else {
                Ok(None)
            }
        }
        None => Ok(None),
    }
}

/// Associate a file path with a signing profile name.
#[tauri::command]
pub(crate) fn set_profile_for_file(
    app: AppHandle,
    path: String,
    profile_name: String,
) -> Result<(), String> {
    let data_dir = get_data_dir(&app)?;
    let mut config = load_profiles(&data_dir);
    config.file_mappings.insert(path, profile_name);
    save_profiles(&data_dir, &config)?;
    Ok(())
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
        assert!(c.file_mappings.is_empty());
    }

    #[test]
    fn save_and_load_profiles_round_trip() {
        let tmp = std::env::temp_dir().join("test_profiles_rt_enc");
        let _ = fs::remove_dir_all(&tmp);
        let _ = fs::create_dir_all(&tmp);
        let config = SigningProfilesConfig {
            profiles: vec![SigningProfile {
                name: "Release".into(),
                keystore_path: "/ks".into(),
                keystore_pass: "my_secret_pw".into(),
                key_alias: "alias".into(),
                key_pass: "key_secret".into(),
            }],
            file_mappings: HashMap::new(),
        };
        save_profiles(&tmp, &config).unwrap();

        // Verify the file on disk has encrypted passwords (not plaintext)
        let raw = fs::read_to_string(profiles_path(&tmp)).unwrap();
        assert!(!raw.contains("my_secret_pw"), "Password should be encrypted on disk");
        assert!(!raw.contains("key_secret"), "Key password should be encrypted on disk");

        // Load and verify decryption
        let loaded = load_profiles(&tmp);
        assert_eq!(loaded.profiles.len(), 1);
        assert_eq!(loaded.profiles[0].name, "Release");
        assert_eq!(loaded.profiles[0].keystore_pass, "my_secret_pw");
        assert_eq!(loaded.profiles[0].key_pass, "key_secret");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn load_profiles_handles_corrupt_json() {
        let tmp = std::env::temp_dir().join("test_profiles_corrupt_enc");
        let _ = fs::remove_dir_all(&tmp);
        let _ = fs::create_dir_all(&tmp);
        fs::write(tmp.join("signing_profiles.json"), "not json!").unwrap();
        let loaded = load_profiles(&tmp);
        assert!(loaded.profiles.is_empty());
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        let key = [42u8; 32];
        let plaintext = "my_secret_password";
        let encrypted = encrypt_field(&key, plaintext).unwrap();
        assert_ne!(encrypted, plaintext);
        assert!(encrypted.contains(':')); // nonce:ciphertext format
        let decrypted = decrypt_field(&key, &encrypted);
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decrypt_handles_plaintext_gracefully() {
        let key = [42u8; 32];
        // Legacy plaintext passwords (no colon) should be returned as-is
        assert_eq!(decrypt_field(&key, "plaintext_password"), "plaintext_password");
    }

    #[test]
    fn encrypt_empty_string() {
        let key = [42u8; 32];
        let encrypted = encrypt_field(&key, "").unwrap();
        assert_eq!(encrypted, "");
        assert_eq!(decrypt_field(&key, ""), "");
    }

    #[test]
    fn file_mappings_included_in_config() {
        let tmp = std::env::temp_dir().join("test_profiles_mappings");
        let _ = fs::remove_dir_all(&tmp);
        let _ = fs::create_dir_all(&tmp);
        let mut config = SigningProfilesConfig::default();
        config.profiles.push(SigningProfile {
            name: "Release".into(),
            keystore_path: "/ks".into(),
            keystore_pass: "pw".into(),
            key_alias: "a".into(),
            key_pass: "kp".into(),
        });
        config.file_mappings.insert("/path/to/app.aab".into(), "Release".into());
        save_profiles(&tmp, &config).unwrap();

        let loaded = load_profiles(&tmp);
        assert_eq!(loaded.file_mappings.get("/path/to/app.aab"), Some(&"Release".to_string()));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn hex_encode_decode_round_trip() {
        let data = vec![0xde, 0xad, 0xbe, 0xef];
        let encoded = hex_encode(&data);
        assert_eq!(encoded, "deadbeef");
        let decoded = hex_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }
}
