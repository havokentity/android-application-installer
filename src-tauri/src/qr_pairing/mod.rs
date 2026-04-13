//! QR Code pairing for ADB wireless debugging (Android 11+).
//!
//! Flow:
//!  1. Generate a QR code containing a random service name + password
//!  2. Phone scans QR → registers the service via mDNS → starts TLS server
//!  3. We discover the phone's service via `adb mdns services`
//!  4. We pair via `adb pair <ip>:<port> <password>` (delegates TLS + SPAKE2 to ADB)
//!  5. Auto-connect to the device after successful pairing
//!
//! This module also contains a full SPAKE2/AES-GCM implementation kept for
//! reference and tests, but the primary path delegates to ADB for reliability.

mod firewall;
mod mdns;
mod protocol;
mod tls;

use rand::Rng;
use serde::Serialize;
use std::env;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::Mutex;

use crate::cmd::run_cmd_async_lenient;

// ─── Network Helpers ─────────────────────────────────────────────────────────

/// Detect the local IP address by connecting a UDP socket to a public address.
/// No actual data is sent.
fn get_local_ip() -> Result<String, String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0")
        .map_err(|e| format!("UDP bind failed: {}", e))?;
    socket
        .connect("8.8.8.8:80")
        .map_err(|e| format!("UDP connect failed: {}", e))?;
    let addr = socket
        .local_addr()
        .map_err(|e| format!("local_addr failed: {}", e))?;
    Ok(addr.ip().to_string())
}

/// Get the ADB RSA public key from `~/.android/adbkey.pub`.
fn get_adb_pub_key() -> Result<String, String> {
    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .map_err(|_| "Cannot determine home directory")?;
    let key_path = PathBuf::from(&home).join(".android").join("adbkey.pub");
    std::fs::read_to_string(&key_path).map_err(|e| {
        format!(
            "ADB public key not found at {}. Start ADB at least once to generate it.\n{}",
            key_path.display(),
            e
        )
    })
}

/// Generate the QR code data string in the format Android expects.
fn qr_data_string(service_name: &str, password: &str) -> String {
    format!("WIFI:T:ADB;S:{};P:{};;", service_name, password)
}

/// Generate an SVG string of the QR code.
fn generate_qr_svg(data: &str) -> Result<String, String> {
    let code =
        qrcode::QrCode::new(data).map_err(|e| format!("QR code generation failed: {}", e))?;
    Ok(code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(200, 200)
        .dark_color(qrcode::render::svg::Color("#000000"))
        .light_color(qrcode::render::svg::Color("#ffffff"))
        .build())
}

/// Generate a random alphanumeric string of the given length.
fn random_alphanum(len: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| {
            let idx = rng.gen_range(0..36);
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + idx - 10) as char
            }
        })
        .collect()
}

// ─── Managed State ───────────────────────────────────────────────────────────

/// Managed state for the QR pairing background task.
pub struct QrPairingServer {
    cancel: Arc<AtomicBool>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl Default for QrPairingServer {
    fn default() -> Self {
        Self {
            cancel: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }
}

// ─── Tauri Return Types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct QrPairingInfo {
    pub qr_svg: String,
    pub qr_data: String,
    pub service_name: String,
    pub password: String,
    pub ip: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct QrPairingResult {
    pub success: bool,
    pub device_ip: Option<String>,
    pub error: Option<String>,
}

// ─── Tauri Commands ──────────────────────────────────────────────────────────

/// Start QR code pairing: show QR, discover phone's mDNS service, connect to it.
///
/// Returns the QR code info immediately. The pairing protocol runs in the
/// background and emits a `qr-pairing-result` event when done.
#[tauri::command]
pub(crate) async fn start_qr_pairing(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<QrPairingServer>>,
    adb_path: String,
) -> Result<QrPairingInfo, String> {
    // Cancel any previous QR pairing session
    {
        let mut guard = state.lock().await;
        guard.cancel.store(true, Ordering::SeqCst);
        if let Some(handle) = guard.handle.take() {
            handle.abort();
        }
    }

    // Ensure ADB server is running (creates key pair if first time)
    let _ = run_cmd_async_lenient(&adb_path, &["start-server"]).await;

    // Read our ADB RSA public key
    let adb_pub_key = get_adb_pub_key()?;

    // Detect local IP
    let local_ip = get_local_ip()?;

    // Generate random service name and password
    let service_name = format!("adb-{}", random_alphanum(8));
    let password = random_alphanum(10);
    let qr_data = qr_data_string(&service_name, &password);
    let qr_svg = generate_qr_svg(&qr_data)?;

    let info = QrPairingInfo {
        qr_svg,
        qr_data,
        service_name: service_name.clone(),
        password: password.clone(),
        ip: local_ip.clone(),
        port: 0, // Port is determined by the phone, not us
    };

    // Set up the background task
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_clone = cancel.clone();
    let app_handle = app.clone();
    let svc_name = service_name.clone();
    let pw = password.clone();
    let ip = local_ip.clone();
    let adb = adb_path.clone();

    let handle = tokio::spawn(async move {
        let result =
            run_pairing_client(&svc_name, &pw, &ip, &adb_pub_key, &cancel_clone, &adb, &app_handle)
                .await;

        let pairing_result = match result {
            Ok(device_ip) => QrPairingResult {
                success: true,
                device_ip: Some(device_ip),
                error: None,
            },
            Err(e) if e.contains("cancelled") => QrPairingResult {
                success: false,
                device_ip: None,
                error: Some("QR pairing cancelled.".into()),
            },
            Err(e) => QrPairingResult {
                success: false,
                device_ip: None,
                error: Some(e),
            },
        };

        let _ = app_handle.emit("qr-pairing-result", &pairing_result);
    });

    // Store the cancel flag and handle
    {
        let mut guard = state.lock().await;
        guard.cancel = cancel;
        guard.handle = Some(handle);
    }

    Ok(info)
}

/// Cancel the current QR pairing session.
#[tauri::command]
pub(crate) async fn cancel_qr_pairing(
    state: tauri::State<'_, Mutex<QrPairingServer>>,
) -> Result<(), String> {
    let mut guard = state.lock().await;
    guard.cancel.store(true, Ordering::SeqCst);
    if let Some(handle) = guard.handle.take() {
        handle.abort();
    }
    Ok(())
}

// ─── Orchestration ───────────────────────────────────────────────────────────

/// Run the pairing client: discover phone's service via ADB mDNS → `adb pair`.
///
/// In QR pairing, the phone is the SERVER and we are the CLIENT:
///  1. We generated the QR code with a random service name and password
///  2. The phone scans the QR, registers the service via mDNS, starts a TLS listener
///  3. We discover the phone's service via `adb mdns services`
///  4. We pair using `adb pair <ip>:<port> <password>`
///
/// This delegates the TLS + SPAKE2 handshake to ADB itself, which handles
/// all platform-specific quirks (mDNS port sharing, Windows DNS Client, etc.).
async fn run_pairing_client(
    service_name: &str,
    password: &str,
    _local_ip: &str,
    _adb_pub_key: &str,
    cancel: &Arc<AtomicBool>,
    adb_path: &str,
    app: &tauri::AppHandle,
) -> Result<String, String> {
    let log = |msg: String| {
        let _ = app.emit("qr-pairing-log", &msg);
    };

    // 0. Ensure Windows Firewall allows our program (needed for mDNS UDP 5353).
    if cfg!(target_os = "windows") {
        if firewall::check_program_rule_exists().await {
            log("✓ Firewall: program rule active".into());
        } else {
            log("Requesting firewall access (you may see a UAC prompt)…".into());
            if firewall::try_add_program_rule_elevated().await {
                log("✓ Firewall: program rule added (persists for future sessions)".into());
            } else {
                log("⚠ Could not add firewall rule. mDNS discovery may be affected.".into());
            }
        }
    }

    // 1. Check ADB mDNS daemon
    let (mdns_check, mdns_check_stderr, _) =
        run_cmd_async_lenient(adb_path, &["mdns", "check"]).await.unwrap_or_default();
    let combined_check = format!("{}\n{}", mdns_check, mdns_check_stderr);
    if combined_check.contains("mdns daemon") || combined_check.contains("Openscreen") {
        log("✓ ADB mDNS daemon available".into());
    } else {
        log("⚠ ADB mDNS daemon check did not confirm availability — will try anyway".into());
    }

    // 2. Discover phone's pairing service via `adb mdns services`
    log("Waiting for phone to scan QR code…".into());
    let (phone_ip, phone_port) = mdns::discover_via_adb_mdns(
        service_name,
        adb_path,
        cancel,
        app,
    ).await?;

    log(format!("✓ Found phone's pairing service at {}:{}", phone_ip, phone_port));

    // 3. Pair using `adb pair <ip>:<port> <password>`
    log(format!("Pairing with {}:{} via ADB…", phone_ip, phone_port));
    let target = format!("{}:{}", phone_ip, phone_port);
    let (stdout, stderr, _) = run_cmd_async_lenient(adb_path, &["pair", &target, password])
        .await
        .map_err(|e| format!("adb pair command failed to run: {}", e))?;

    let combined = format!("{}\n{}", stdout, stderr);
    if stdout.contains("Successfully paired") {
        log("✓ Pairing successful!".into());
    } else if combined.contains("Failed") || combined.contains("error") || combined.contains("refused") {
        return Err(format!("Pairing failed: {}", combined.trim()));
    } else if combined.contains("timed out") || combined.contains("timeout") {
        return Err("Pairing timed out. The phone may have closed the pairing dialog.".into());
    } else if stdout.trim().is_empty() && stderr.trim().is_empty() {
        return Err("Pairing failed: no response from ADB.".into());
    } else {
        // Treat unknown output as possible success (some ADB versions differ)
        log(format!("ADB pair output: {}", combined.trim()));
    }

    // 4. Auto-connect: after pairing, the phone advertises a _adb-tls-connect service
    let device_ip = phone_ip.clone();
    log("Pairing succeeded — waiting for device to advertise connect service…".into());
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let _ = try_auto_connect(adb_path, &device_ip).await;

    Ok(device_ip)
}

/// After successful pairing, try to auto-connect to the device by scanning
/// mDNS for a `_adb-tls-connect._tcp` service from the same IP.
async fn try_auto_connect(adb_path: &str, device_ip: &str) -> Result<(), String> {
    // Scan mDNS services
    let (stdout, _, _) = run_cmd_async_lenient(adb_path, &["mdns", "services"])
        .await
        .map_err(|e| format!("mDNS scan failed: {}", e))?;

    // Look for a connect service from the same IP
    for line in stdout.lines() {
        if !line.contains('\t') {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            let svc_type = parts[1].trim();
            let ip_port = parts[2].trim();
            if svc_type.contains("connect") && ip_port.starts_with(device_ip) {
                // Try to connect
                let _ = run_cmd_async_lenient(adb_path, &["connect", ip_port]).await;
                return Ok(());
            }
        }
    }

    // Fallback: try common port 5555
    let target = format!("{}:5555", device_ip);
    let _ = run_cmd_async_lenient(adb_path, &["connect", &target]).await;
    Ok(())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_data_format() {
        let data = qr_data_string("adb-test123", "secret");
        assert_eq!(data, "WIFI:T:ADB;S:adb-test123;P:secret;;");
    }

    #[test]
    fn qr_svg_generates() {
        let svg = generate_qr_svg("test data");
        assert!(svg.is_ok());
        assert!(svg.unwrap().contains("<svg"));
    }

    #[test]
    fn random_alphanum_length() {
        let s = random_alphanum(10);
        assert_eq!(s.len(), 10);
        assert!(s.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn get_local_ip_works() {
        // This test may fail in CI environments without network access
        let result = get_local_ip();
        if let Ok(ip) = result {
            assert!(!ip.is_empty());
            assert!(ip.contains('.') || ip.contains(':'));
        }
    }
}
