//! QR Code pairing for ADB wireless debugging (Android 11+).
//!
//! Implements the ADB pairing protocol server:
//!  1. Advertise an mDNS `_adb-tls-pairing._tcp` service
//!  2. Display a QR code with the pairing info
//!  3. Accept a TCP connection from the phone
//!  4. SPAKE2 key exchange (BoringSSL-compatible, Ed25519)
//!  5. AES-128-GCM encrypted PeerInfo exchange
//!  6. Phone stores our RSA key → future `adb connect` works without prompt

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes128Gcm, Key, Nonce};
use curve25519_dalek::edwards::{CompressedEdwardsY, EdwardsPoint};
use curve25519_dalek::scalar::Scalar;
use hkdf::Hkdf;
use rand::Rng;
use serde::Serialize;
use sha2::{Digest, Sha256, Sha512};
use std::env;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::Emitter;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;
use tokio_rustls::TlsAcceptor;

use crate::cmd::run_cmd_async_lenient;

// ─── Protocol Constants ──────────────────────────────────────────────────────

/// Protocol version byte.
const PAIRING_VERSION: u8 = 1;

/// Message types in the pairing packet header.
const MSG_SPAKE2: u8 = 0;
const MSG_PEER_INFO: u8 = 1;

/// Peer info type: ADB RSA public key.
const PEER_TYPE_RSA_PUB_KEY: u8 = 0;

/// Maximum size of the `data` field in PeerInfo (matches AOSP `MAX_PEER_INFO_SIZE`).
const MAX_PEER_INFO_SIZE: usize = 1 << 13; // 8192

/// Total PeerInfo struct size: 1 (type) + 8192 (data).
const PEER_INFO_SIZE: usize = 1 + MAX_PEER_INFO_SIZE;

/// Header size: 1 (version) + 1 (type) + 4 (payload length, big-endian).
const HEADER_SIZE: usize = 6;

/// HKDF info string used by ADB to derive the AES key from the SPAKE2 output.
const HKDF_INFO: &[u8] = b"adb pairing_auth aes-128-gcm key";

/// QR pairing timeout in seconds.
const PAIRING_TIMEOUT_SECS: u64 = 120;

// ─── SPAKE2 M/N Points (RFC 9382) ────────────────────────────────────────────
//
// These are the "nothing up my sleeve" generator points for the Ed25519
// SPAKE2 ciphersuite, taken verbatim from RFC 9382 §4.  BoringSSL (which
// Android's ADB uses internally) uses these exact same values.

/// Compressed Edwards Y for M (RFC 9382, Ed25519).
const SPAKE2_M_COMPRESSED: [u8; 32] = [
    0xd0, 0x48, 0x03, 0x2c, 0x6e, 0xa0, 0xb6, 0xd6,
    0x97, 0xdd, 0xc2, 0xe8, 0x6b, 0xda, 0x85, 0xa3,
    0x3a, 0xda, 0xc9, 0x20, 0xf1, 0xbf, 0x18, 0xe1,
    0xb0, 0xc6, 0xd1, 0x66, 0xa3, 0x70, 0x04, 0xd5,
];

/// Compressed Edwards Y for N (RFC 9382, Ed25519).
const SPAKE2_N_COMPRESSED: [u8; 32] = [
    0xd3, 0xbf, 0xb5, 0x18, 0xf4, 0x4f, 0x34, 0x30,
    0xf2, 0x9d, 0x0c, 0x92, 0xaf, 0x50, 0x38, 0x65,
    0xa1, 0xed, 0x32, 0x81, 0xdc, 0x69, 0xb3, 0x5d,
    0xd8, 0x68, 0xba, 0x85, 0xf8, 0x86, 0xc4, 0xab,
];

/// AOSP identity strings (including null terminator, matching sizeof() in C).
/// Both client and server pass these in the same order to SPAKE2_CTX_new.
const SPAKE2_CLIENT_NAME: &[u8] = b"adb pair client\0";
const SPAKE2_SERVER_NAME: &[u8] = b"adb pair server\0";

fn spake2_m() -> EdwardsPoint {
    CompressedEdwardsY(SPAKE2_M_COMPRESSED)
        .decompress()
        .expect("RFC 9382 M point must decompress")
}

fn spake2_n() -> EdwardsPoint {
    CompressedEdwardsY(SPAKE2_N_COMPRESSED)
        .decompress()
        .expect("RFC 9382 N point must decompress")
}

// ─── SPAKE2 Implementation (BoringSSL-compatible, Bob/Server) ────────────────
//
// In ADB pairing the desktop is the **server** (Bob) and the phone is the
// **client** (Alice).
//
//   Alice (phone):  T* = x·B + w·M      K_a = x·(S* − w·N)
//   Bob   (desktop): S* = y·B + w·N      K_b = y·(T* − w·M)
//
// Transcript (both sides compute the same hash):
//   SHA-256( len(kClientName)||kClientName ||
//            len(kServerName)||kServerName ||
//            len(T*)||T* || len(S*)||S*    ||
//            len(K)||K )

/// Hash the password to a Scalar using SHA-512 + reduce (same as BoringSSL).
fn password_to_scalar(password: &[u8]) -> Scalar {
    let hash = Sha512::digest(password);
    let mut wide = [0u8; 64];
    wide.copy_from_slice(&hash);
    Scalar::from_bytes_mod_order_wide(&wide)
}

/// SPAKE2 Bob (server / desktop) state.
struct Spake2Bob {
    y: Scalar,
    w: Scalar,
    my_msg: [u8; 32], // S* (Bob's public message)
}

impl Spake2Bob {
    /// Create a new SPAKE2 Bob context, returning (state, outbound_message).
    fn new(password: &[u8]) -> Result<(Self, Vec<u8>), String> {
        let w = password_to_scalar(password);
        let n_point = spake2_n();

        // Random scalar y
        let y = Scalar::random(&mut rand::rngs::OsRng);

        // S* = y·B + w·N  (B = Ed25519 basepoint)
        let s_star = EdwardsPoint::mul_base(&y) + w * n_point;
        let my_msg = s_star.compress().to_bytes();

        Ok((Self { y, w, my_msg }, my_msg.to_vec()))
    }

    /// Process Alice's (phone) message and derive the shared key.
    fn finish(self, their_msg: &[u8]) -> Result<Vec<u8>, String> {
        if their_msg.len() != 32 {
            return Err(format!("Invalid SPAKE2 message length: {}", their_msg.len()));
        }

        let m_point = spake2_m();

        let mut msg_bytes = [0u8; 32];
        msg_bytes.copy_from_slice(their_msg);
        let t_star = CompressedEdwardsY(msg_bytes)
            .decompress()
            .ok_or("Failed to decompress peer's SPAKE2 message")?;

        // K = y·(T* − w·M)
        let k = self.y * (t_star - self.w * m_point);
        let k_bytes = k.compress().to_bytes();

        // Transcript hash (BoringSSL-compatible).
        // Bob reorders so that the result is identical to Alice's hash:
        //   my_name || their_name || their_msg(T*) || my_msg(S*) || K
        // which equals: kClientName || kServerName || T* || S* || K
        let mut hasher = Sha256::new();
        hash_with_length(&mut hasher, SPAKE2_CLIENT_NAME); // my_name  (= kClientName)
        hash_with_length(&mut hasher, SPAKE2_SERVER_NAME); // their_name (= kServerName)
        hash_with_length(&mut hasher, their_msg);          // T* (Alice's message)
        hash_with_length(&mut hasher, &self.my_msg);       // S* (Bob's message)
        hash_with_length(&mut hasher, &k_bytes);           // K  (shared point)

        Ok(hasher.finalize().to_vec())
    }
}

/// Append `len(data)` (LE u64) + `data` to the SHA-256 hasher (matches BoringSSL).
fn hash_with_length(hasher: &mut Sha256, data: &[u8]) {
    hasher.update((data.len() as u64).to_le_bytes());
    hasher.update(data);
}

// ─── Key Derivation & Encryption ─────────────────────────────────────────────

/// Derive the AES-128-GCM key from the SPAKE2 shared key using HKDF-SHA256.
fn derive_aes_key(spake2_key: &[u8]) -> Result<[u8; 16], String> {
    let hkdf = Hkdf::<Sha256>::new(None, spake2_key);
    let mut okm = [0u8; 16];
    hkdf.expand(HKDF_INFO, &mut okm)
        .map_err(|e| format!("HKDF expand failed: {}", e))?;
    Ok(okm)
}

/// Build a 12-byte GCM nonce from a counter (LE u64, padded with zeros).
fn make_nonce(counter: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[..8].copy_from_slice(&counter.to_le_bytes());
    nonce
}

/// Encrypt data with AES-128-GCM.
fn aes_encrypt(key: &[u8; 16], nonce_counter: u64, plaintext: &[u8]) -> Result<Vec<u8>, String> {
    let cipher = Aes128Gcm::new(Key::<Aes128Gcm>::from_slice(key));
    let nonce = make_nonce(nonce_counter);
    cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext)
        .map_err(|e| format!("AES-GCM encrypt failed: {}", e))
}

/// Decrypt data with AES-128-GCM.
fn aes_decrypt(key: &[u8; 16], nonce_counter: u64, ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    let cipher = Aes128Gcm::new(Key::<Aes128Gcm>::from_slice(key));
    let nonce = make_nonce(nonce_counter);
    cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext)
        .map_err(|e| format!("AES-GCM decrypt failed: {}", e))
}

// ─── PeerInfo ────────────────────────────────────────────────────────────────

/// Build the PeerInfo struct (8193 bytes) from the ADB RSA public key string.
fn build_peer_info(adb_pub_key: &str) -> Vec<u8> {
    let mut info = vec![0u8; PEER_INFO_SIZE];
    info[0] = PEER_TYPE_RSA_PUB_KEY;
    let key_bytes = adb_pub_key.as_bytes();
    let copy_len = key_bytes.len().min(MAX_PEER_INFO_SIZE - 1); // leave room for null terminator
    info[1..1 + copy_len].copy_from_slice(&key_bytes[..copy_len]);
    // Rest is zero-padded (already zeroed), null terminator implicit
    info
}

// ─── Wire Protocol ───────────────────────────────────────────────────────────

/// Write a pairing packet header to the stream.
async fn write_header(
    stream: &mut (impl AsyncWriteExt + Unpin),
    msg_type: u8,
    payload_len: u32,
) -> Result<(), String> {
    let mut header = [0u8; HEADER_SIZE];
    header[0] = PAIRING_VERSION;
    header[1] = msg_type;
    header[2..6].copy_from_slice(&payload_len.to_be_bytes());
    stream
        .write_all(&header)
        .await
        .map_err(|e| format!("Write header failed: {}", e))
}

/// Read a pairing packet header from the stream. Returns (msg_type, payload_len).
async fn read_header(
    stream: &mut (impl AsyncReadExt + Unpin),
) -> Result<(u8, u32), String> {
    let mut header = [0u8; HEADER_SIZE];
    stream
        .read_exact(&mut header)
        .await
        .map_err(|e| format!("Read header failed: {}", e))?;

    let version = header[0];
    if version != PAIRING_VERSION {
        return Err(format!("Unsupported pairing version: {}", version));
    }
    let msg_type = header[1];
    let payload_len = u32::from_be_bytes([header[2], header[3], header[4], header[5]]);
    Ok((msg_type, payload_len))
}

/// Read exactly `len` bytes from the stream.
async fn read_payload(
    stream: &mut (impl AsyncReadExt + Unpin),
    len: usize,
) -> Result<Vec<u8>, String> {
    let mut buf = vec![0u8; len];
    stream
        .read_exact(&mut buf)
        .await
        .map_err(|e| format!("Read payload failed: {}", e))?;
    Ok(buf)
}

// ─── Pairing Protocol Handler ────────────────────────────────────────────────

/// Run the full ADB pairing protocol on an established (TLS) stream.
///
/// Protocol flow (we are Bob / Server):
///  1. Send our SPAKE2 message (S*)
///  2. Read client's (Alice/phone) SPAKE2 message (T*)
///  3. Derive shared AES key
///  4. Read client's encrypted PeerInfo (server reads FIRST)
///  5. Send our encrypted PeerInfo
async fn handle_pairing<S>(
    stream: &mut S,
    password: &[u8],
    adb_pub_key: &str,
) -> Result<(), String>
where
    S: AsyncReadExt + AsyncWriteExt + Unpin,
{
    // 1. SPAKE2 handshake — generate our message (S* = y·B + w·N)
    let (spake, our_msg) = Spake2Bob::new(password)?;

    // 2. Send our SPAKE2 message
    write_header(stream, MSG_SPAKE2, our_msg.len() as u32).await?;
    stream
        .write_all(&our_msg)
        .await
        .map_err(|e| format!("Send SPAKE2 msg failed: {}", e))?;
    // Flush to ensure the message is sent before we wait for the response.
    // Without this, TLS may buffer the record and deadlock (both sides waiting
    // for a message the other hasn't flushed yet).
    stream
        .flush()
        .await
        .map_err(|e| format!("Flush SPAKE2 msg failed: {}", e))?;

    // 3. Read client's SPAKE2 message
    let (msg_type, payload_len) = read_header(stream).await?;
    if msg_type != MSG_SPAKE2 {
        return Err(format!("Expected SPAKE2 message, got type {}", msg_type));
    }
    let their_msg = read_payload(stream, payload_len as usize).await?;

    // 4. Complete SPAKE2, derive shared key
    let spake2_key = spake.finish(&their_msg)?;

    // 5. Derive AES-128-GCM key via HKDF
    let aes_key = derive_aes_key(&spake2_key)?;

    // 6. Server reads client's encrypted PeerInfo FIRST (AOSP protocol order)
    let (peer_type, peer_payload_len) = read_header(stream).await?;
    if peer_type != MSG_PEER_INFO {
        return Err(format!("Expected PeerInfo message, got type {}", peer_type));
    }
    let encrypted_peer = read_payload(stream, peer_payload_len as usize).await?;

    // Decrypt with dec_nonce = 0
    let _peer_info = aes_decrypt(&aes_key, 0, &encrypted_peer)?;
    // We don't strictly need the phone's key — the phone has stored ours,
    // and that's what matters for future `adb connect` to work.

    // 7. Send our encrypted PeerInfo
    let our_peer_info = build_peer_info(adb_pub_key);
    let encrypted_ours = aes_encrypt(&aes_key, 0, &our_peer_info)?;
    write_header(stream, MSG_PEER_INFO, encrypted_ours.len() as u32).await?;
    stream
        .write_all(&encrypted_ours)
        .await
        .map_err(|e| format!("Send PeerInfo failed: {}", e))?;

    // Flush to ensure all data is sent
    stream
        .flush()
        .await
        .map_err(|e| format!("Flush failed: {}", e))?;

    Ok(())
}

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

// ─── TLS ─────────────────────────────────────────────────────────────────────

/// Generate a self-signed TLS certificate and build a [`TlsAcceptor`].
///
/// AOSP's pairing server uses an ephemeral RSA-2048 self-signed cert with
/// `SSL_VERIFY_NONE` — neither side verifies the other's certificate.
/// We use rcgen's default (ECDSA P-256) which BoringSSL also supports.
fn build_tls_acceptor() -> Result<TlsAcceptor, String> {
    let ck = rcgen::generate_simple_self_signed(vec!["adb".into()])
        .map_err(|e| format!("TLS cert generation failed: {e}"))?;

    let cert_der = tokio_rustls::rustls::pki_types::CertificateDer::from(
        ck.cert.der().to_vec(),
    );
    let key_der = tokio_rustls::rustls::pki_types::PrivateKeyDer::from(
        tokio_rustls::rustls::pki_types::PrivatePkcs8KeyDer::from(
            ck.key_pair.serialize_der(),
        ),
    );

    // AOSP mandates TLS 1.3 only for ADB pairing
    // (SSL_CTX_set_min/max_proto_version both set to TLS1_3_VERSION)
    let config = tokio_rustls::rustls::ServerConfig::builder_with_protocol_versions(
            &[&tokio_rustls::rustls::version::TLS13],
        )
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der)
        .map_err(|e| format!("TLS config failed: {e}"))?;

    Ok(TlsAcceptor::from(Arc::new(config)))
}

// ─── Windows Firewall ────────────────────────────────────────────────────────

/// Try to add a temporary inbound TCP firewall rule for the pairing port.
/// Returns `true` if the rule was added, `false` if it failed (no admin, etc.).
/// This is best-effort — the pairing may still work without it if the user has
/// already allowed the application through Windows Firewall.
#[cfg(target_os = "windows")]
async fn try_add_firewall_rule(port: u16) -> bool {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let rule_name = format!("ADB QR Pairing (port {})", port);
    let port_str = port.to_string();
    let result = tokio::process::Command::new("netsh")
        .args([
            "advfirewall", "firewall", "add", "rule",
            &format!("name={}", rule_name),
            "dir=in", "action=allow", "protocol=tcp",
            &format!("localport={}", port_str),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;

    matches!(result, Ok(status) if status.success())
}

/// Remove the temporary firewall rule added by [`try_add_firewall_rule`].
#[cfg(target_os = "windows")]
async fn try_remove_firewall_rule(port: u16) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let rule_name = format!("ADB QR Pairing (port {})", port);
    let _ = tokio::process::Command::new("netsh")
        .args([
            "advfirewall", "firewall", "delete", "rule",
            &format!("name={}", rule_name),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;
}

/// No-op on non-Windows platforms.
#[cfg(not(target_os = "windows"))]
async fn try_add_firewall_rule(_port: u16) -> bool { false }
#[cfg(not(target_os = "windows"))]
async fn try_remove_firewall_rule(_port: u16) {}

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

/// Start QR code pairing: advertise mDNS, show QR, wait for phone to connect.
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

    // Bind TCP listener on an ephemeral port
    let listener = tokio::net::TcpListener::bind("0.0.0.0:0")
        .await
        .map_err(|e| format!("TCP bind failed: {}", e))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("local_addr failed: {}", e))?
        .port();

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
        port,
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
            run_pairing_server(listener, &svc_name, &pw, &ip, port, &adb_pub_key, &cancel_clone, &adb, &app_handle)
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

/// Run the pairing server: firewall → TLS cert → mDNS advertisement → TCP accept → TLS → SPAKE2.
async fn run_pairing_server(
    listener: tokio::net::TcpListener,
    service_name: &str,
    password: &str,
    local_ip: &str,
    port: u16,
    adb_pub_key: &str,
    cancel: &Arc<AtomicBool>,
    adb_path: &str,
    app: &tauri::AppHandle,
) -> Result<String, String> {
    // Helper to emit progress strings the frontend can show in its log panel
    let log = |msg: String| {
        let _ = app.emit("qr-pairing-log", &msg);
    };

    // 0. Try to add a Windows Firewall exception for the pairing port.
    //    The phone makes an INBOUND TCP connection to our port — Windows Firewall
    //    blocks inbound by default unless the app is explicitly allowed.
    let firewall_added = try_add_firewall_rule(port).await;
    if firewall_added {
        log(format!("✓ Firewall rule added for TCP port {}", port));
    } else if cfg!(target_os = "windows") {
        log("⚠ Could not add firewall rule (may need admin). If pairing hangs, allow this app in Windows Firewall.".into());
    }

    // 1. Build ephemeral TLS acceptor (self-signed cert, TLS 1.3 only, no client-cert verify)
    let tls_acceptor = build_tls_acceptor()?;
    log("✓ TLS 1.3 certificate generated".into());

    // 2. Start mDNS advertisement
    //    The phone discovers our pairing server via mDNS after scanning the QR.
    //    If multicast doesn't traverse WiFi↔Ethernet on the router, the phone
    //    will never find us — there is no fallback in the Android QR pairing flow.
    let mdns = mdns_sd::ServiceDaemon::new()
        .map_err(|e| format!("mDNS daemon failed to start: {}", e))?;

    let hostname = format!("{}.local.", service_name);
    let service_info = mdns_sd::ServiceInfo::new(
        "_adb-tls-pairing._tcp.local.",
        service_name,
        &hostname,
        local_ip,
        port,
        None::<std::collections::HashMap<String, String>>,
    )
    .map_err(|e| format!("mDNS service creation failed: {}", e))?;

    let fullname = service_info.get_fullname().to_string();
    mdns.register(service_info.clone())
        .map_err(|e| format!("mDNS registration failed: {}", e))?;

    log(format!("✓ mDNS service registered: {} at {}:{}", service_name, local_ip, port));
    log("Waiting for phone to scan QR and connect…".into());

    // Give the initial announcement a moment to propagate
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // 3. Wait for the phone to connect (with periodic diagnostics)
    let accept_future = listener.accept();
    tokio::pin!(accept_future);

    let start = std::time::Instant::now();
    let mut warned_firewall = false;
    let mut last_reannounce = std::time::Instant::now();

    let result = loop {
        tokio::select! {
            accept_result = &mut accept_future => {
                match accept_result {
                    Ok((tcp_stream, addr)) => {
                        let device_ip = addr.ip().to_string();
                        log(format!("✓ TCP connection from {}", addr));

                        // 4. TLS 1.3 handshake
                        log("Starting TLS 1.3 handshake…".into());
                        let mut tls_stream = tls_acceptor.accept(tcp_stream).await
                            .map_err(|e| format!("TLS handshake failed: {}", e))?;
                        log("✓ TLS handshake complete".into());

                        // 5. Run SPAKE2 + PeerInfo exchange over TLS
                        log("Running SPAKE2 key exchange…".into());
                        handle_pairing(&mut tls_stream, password.as_bytes(), adb_pub_key).await?;
                        log("✓ Pairing protocol complete!".into());

                        break Ok(device_ip);
                    }
                    Err(e) => break Err(format!("TCP accept failed: {}", e)),
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                // ── Periodic check: cancel, diagnostics, timeout ──
                if cancel.load(Ordering::Relaxed) {
                    break Err("cancelled".to_string());
                }

                let elapsed = start.elapsed().as_secs();

                // Re-announce mDNS every ~30 s in case the initial advertisement was missed
                if last_reannounce.elapsed().as_secs() >= 30 {
                    last_reannounce = std::time::Instant::now();
                    let re_info = mdns_sd::ServiceInfo::new(
                        "_adb-tls-pairing._tcp.local.",
                        service_name,
                        &hostname,
                        local_ip,
                        port,
                        None::<std::collections::HashMap<String, String>>,
                    );
                    if let Ok(ri) = re_info {
                        let _ = mdns.register(ri);
                    }
                    log("(re-announced mDNS service)".into());
                }

                // Diagnostic warning after 15 s with no connection
                if elapsed >= 15 && !warned_firewall {
                    warned_firewall = true;
                    log("⚠ No connection received after 15 s — possible causes:".into());
                    if cfg!(target_os = "windows") {
                        log("  • Windows Firewall may be blocking inbound TCP connections".into());
                        log("    → Open Windows Firewall settings and allow this app".into());
                    }
                    log("  • mDNS multicast may not traverse between WiFi and wired LAN".into());
                    log("    → Some routers block multicast between wireless and ethernet".into());
                    log("    → If your PC has WiFi, try enabling it temporarily".into());
                    log(format!("  • Server listening at {}:{}", local_ip, port));
                }

                // Timeout
                if elapsed >= PAIRING_TIMEOUT_SECS {
                    break Err(
                        "QR pairing timed out — the phone never connected. \
                         Check Windows Firewall and ensure mDNS multicast works between \
                         your phone's WiFi and your PC's network."
                            .to_string(),
                    );
                }
            }
        }
    };

    // Clean up mDNS regardless of result
    let _ = mdns.unregister(&fullname);
    let _ = mdns.shutdown();

    // Clean up temporary firewall rule
    if firewall_added {
        try_remove_firewall_rule(port).await;
    }

    // If pairing succeeded, try to auto-connect via mDNS discovery
    if let Ok(ref device_ip) = result {
        log("Pairing succeeded — waiting for device to advertise connect service…".into());
        // Give the phone a moment to start its connect service
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Try to find the device's connect port via mDNS and auto-connect
        let _ = try_auto_connect(adb_path, device_ip).await;
    }

    result
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
    fn password_to_scalar_deterministic() {
        let s1 = password_to_scalar(b"test123");
        let s2 = password_to_scalar(b"test123");
        assert_eq!(s1, s2);
    }

    #[test]
    fn password_to_scalar_different() {
        let s1 = password_to_scalar(b"abc");
        let s2 = password_to_scalar(b"xyz");
        assert_ne!(s1, s2);
    }

    #[test]
    fn spake2_m_decompresses() {
        let m = CompressedEdwardsY(SPAKE2_M_COMPRESSED)
            .decompress()
            .expect("RFC 9382 M point must decompress");
        // Verify round-trip
        assert_eq!(m.compress().to_bytes(), SPAKE2_M_COMPRESSED);
    }

    #[test]
    fn spake2_n_decompresses() {
        let n = CompressedEdwardsY(SPAKE2_N_COMPRESSED)
            .decompress()
            .expect("RFC 9382 N point must decompress");
        // Verify round-trip
        assert_eq!(n.compress().to_bytes(), SPAKE2_N_COMPRESSED);
    }

    #[test]
    fn spake2_m_n_are_distinct() {
        assert_ne!(SPAKE2_M_COMPRESSED, SPAKE2_N_COMPRESSED);
        assert_ne!(spake2_m(), spake2_n());
    }

    #[test]
    fn spake2_bob_creates() {
        let result = Spake2Bob::new(b"123456");
        assert!(result.is_ok());
        let (_, msg) = result.unwrap();
        assert_eq!(msg.len(), 32);
    }

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
    fn peer_info_size() {
        let info = build_peer_info("some_key_data");
        assert_eq!(info.len(), PEER_INFO_SIZE);
        assert_eq!(info[0], PEER_TYPE_RSA_PUB_KEY);
        assert_eq!(&info[1..14], b"some_key_data");
        assert_eq!(info[14], 0); // null terminator
    }

    #[test]
    fn peer_info_truncates_long_key() {
        let long_key = "A".repeat(MAX_PEER_INFO_SIZE + 100);
        let info = build_peer_info(&long_key);
        assert_eq!(info.len(), PEER_INFO_SIZE);
        // Data should be truncated to MAX_PEER_INFO_SIZE - 1 (for null terminator)
        assert_eq!(info[MAX_PEER_INFO_SIZE], 0);
    }

    #[test]
    fn aes_roundtrip() {
        let key = [0x42u8; 16];
        let plaintext = b"hello world";
        let encrypted = aes_encrypt(&key, 0, plaintext).unwrap();
        let decrypted = aes_decrypt(&key, 0, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn aes_different_nonces_produce_different_ciphertext() {
        let key = [0x42u8; 16];
        let plaintext = b"hello";
        let enc1 = aes_encrypt(&key, 0, plaintext).unwrap();
        let enc2 = aes_encrypt(&key, 1, plaintext).unwrap();
        assert_ne!(enc1, enc2);
    }

    #[test]
    fn hkdf_derives_key() {
        let spake_key = [0x55u8; 32];
        let result = derive_aes_key(&spake_key);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 16);
    }

    #[test]
    fn make_nonce_format() {
        let n = make_nonce(0);
        assert_eq!(n, [0u8; 12]);

        let n = make_nonce(1);
        assert_eq!(n[0], 1);
        assert_eq!(n[1..], [0u8; 11]);

        let n = make_nonce(256);
        assert_eq!(n[0], 0);
        assert_eq!(n[1], 1);
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

