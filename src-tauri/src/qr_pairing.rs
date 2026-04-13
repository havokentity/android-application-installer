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

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes128Gcm, Key, Nonce};
use curve25519_dalek::edwards::{CompressedEdwardsY, EdwardsPoint};
use curve25519_dalek::scalar::Scalar;
use hkdf::Hkdf;
use rand::Rng;
use serde::Serialize;
use sha2::{Digest, Sha256, Sha512};
use std::env;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::Emitter;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::TlsConnector;

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

// ─── SPAKE2 Implementation (BoringSSL-compatible) ────────────────────────────
//
// In QR pairing the phone is the **server** (Bob) and the desktop is the
// **client** (Alice).
//
//   Alice (desktop): T* = x·B + w·M      K_a = x·(S* − w·N)
//   Bob   (phone):   S* = y·B + w·N      K_b = y·(T* − w·M)
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

/// SPAKE2 Alice (client / desktop) state.
#[allow(dead_code)]
struct Spake2Alice {
    x: Scalar,
    w: Scalar,
    my_msg: [u8; 32], // T* (Alice's public message)
}

#[allow(dead_code)]
impl Spake2Alice {
    /// Create a new SPAKE2 Alice context, returning (state, outbound_message).
    fn new(password: &[u8]) -> Result<(Self, Vec<u8>), String> {
        let w = password_to_scalar(password);
        let m_point = spake2_m();

        // Random scalar x
        let x = Scalar::random(&mut rand::rngs::OsRng);

        // T* = x·B + w·M  (B = Ed25519 basepoint)
        let t_star = EdwardsPoint::mul_base(&x) + w * m_point;
        let my_msg = t_star.compress().to_bytes();

        Ok((Self { x, w, my_msg }, my_msg.to_vec()))
    }

    /// Process Bob's (phone) message and derive the shared key.
    fn finish(self, their_msg: &[u8]) -> Result<Vec<u8>, String> {
        if their_msg.len() != 32 {
            return Err(format!("Invalid SPAKE2 message length: {}", their_msg.len()));
        }

        let n_point = spake2_n();

        let mut msg_bytes = [0u8; 32];
        msg_bytes.copy_from_slice(their_msg);
        let s_star = CompressedEdwardsY(msg_bytes)
            .decompress()
            .ok_or("Failed to decompress peer's SPAKE2 message")?;

        // K = x·(S* − w·N)
        let k = self.x * (s_star - self.w * n_point);
        let k_bytes = k.compress().to_bytes();

        // Transcript hash (BoringSSL-compatible):
        //   kClientName || kServerName || T* || S* || K
        let mut hasher = Sha256::new();
        hash_with_length(&mut hasher, SPAKE2_CLIENT_NAME);
        hash_with_length(&mut hasher, SPAKE2_SERVER_NAME);
        hash_with_length(&mut hasher, &self.my_msg); // T* (Alice's message)
        hash_with_length(&mut hasher, their_msg);    // S* (Bob's message)
        hash_with_length(&mut hasher, &k_bytes);     // K  (shared point)

        Ok(hasher.finalize().to_vec())
    }
}

// Bob kept for tests
#[allow(dead_code)]
/// SPAKE2 Bob (server) state.
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

/// Run the full ADB pairing protocol as the CLIENT (Alice).
///
/// Protocol flow (we are Alice / Client):
///  1. Send our SPAKE2 message (T*)
///  2. Read server's (Bob/phone) SPAKE2 message (S*)
///  3. Derive shared AES key
///  4. Send our encrypted PeerInfo (client sends FIRST in AOSP protocol)
///  5. Read server's encrypted PeerInfo
#[allow(dead_code)]
async fn handle_pairing_as_client<S>(
    stream: &mut S,
    password: &[u8],
    adb_pub_key: &str,
) -> Result<(), String>
where
    S: AsyncReadExt + AsyncWriteExt + Unpin,
{
    // 1. SPAKE2 handshake — generate our message (T* = x·B + w·M)
    let (spake, our_msg) = Spake2Alice::new(password)?;

    // 2. Send our SPAKE2 message
    write_header(stream, MSG_SPAKE2, our_msg.len() as u32).await?;
    stream
        .write_all(&our_msg)
        .await
        .map_err(|e| format!("Send SPAKE2 msg failed: {}", e))?;
    stream
        .flush()
        .await
        .map_err(|e| format!("Flush SPAKE2 msg failed: {}", e))?;

    // 3. Read server's SPAKE2 message
    let (msg_type, payload_len) = read_header(stream).await?;
    if msg_type != MSG_SPAKE2 {
        return Err(format!("Expected SPAKE2 message, got type {}", msg_type));
    }
    let their_msg = read_payload(stream, payload_len as usize).await?;

    // 4. Complete SPAKE2, derive shared key
    let spake2_key = spake.finish(&their_msg)?;
    let aes_key = derive_aes_key(&spake2_key)?;

    // 5. Client sends encrypted PeerInfo FIRST (AOSP protocol order)
    let our_peer_info = build_peer_info(adb_pub_key);
    let encrypted_ours = aes_encrypt(&aes_key, 0, &our_peer_info)?;
    write_header(stream, MSG_PEER_INFO, encrypted_ours.len() as u32).await?;
    stream
        .write_all(&encrypted_ours)
        .await
        .map_err(|e| format!("Send PeerInfo failed: {}", e))?;
    stream
        .flush()
        .await
        .map_err(|e| format!("Flush PeerInfo failed: {}", e))?;

    // 6. Read server's encrypted PeerInfo
    let (peer_type, peer_payload_len) = read_header(stream).await?;
    if peer_type != MSG_PEER_INFO {
        return Err(format!("Expected PeerInfo message, got type {}", peer_type));
    }
    let encrypted_peer = read_payload(stream, peer_payload_len as usize).await?;
    let _peer_info = aes_decrypt(&aes_key, 0, &encrypted_peer)?;

    Ok(())
}

// Old server-side handler kept for tests
#[allow(dead_code)]
/// Run the full ADB pairing protocol as the SERVER (Bob).
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

// ─── Raw mDNS Fallback ──────────────────────────────────────────────────────
//
// When the mdns-sd daemon can't properly handle port 5353 sharing on Windows
// (common when the Windows DNS Client service or another mDNS daemon holds the
// port), OR when multicast doesn't traverse between wired Ethernet and WiFi
// segments, this fallback sends gratuitous mDNS announcements directly via a
// raw UDP socket on EVERY network interface.  The phone picks up these
// unsolicited responses and discovers our pairing service.

/// Detect all usable IPv4 addresses on this machine.
/// On Windows, parses `ipconfig` output. Falls back to the primary IP.
#[allow(dead_code)]
async fn get_all_ipv4_addrs(primary_ip: &str) -> Vec<Ipv4Addr> {
    let mut addrs = Vec::new();

    #[cfg(target_os = "windows")]
    {
        const CNW: u32 = 0x0800_0000;
        if let Ok(output) = tokio::process::Command::new("ipconfig")
            .creation_flags(CNW)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .await
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let lower = line.to_lowercase();
                // Skip gateway, subnet mask, DNS, DHCP lines
                if lower.contains("gateway") || lower.contains("mask")
                    || lower.contains("dns") || lower.contains("dhcp")
                {
                    continue;
                }
                if let Some(after) = line.rsplit(':').next() {
                    if let Ok(ip) = after.trim().parse::<Ipv4Addr>() {
                        if !ip.is_loopback() && !ip.is_link_local()
                            && !ip.is_unspecified() && ip.octets()[0] < 224
                            && !addrs.contains(&ip)
                        {
                            addrs.push(ip);
                        }
                    }
                }
            }
        }
    }

    // Ensure the primary IP is always included (and first)
    if let Ok(primary) = primary_ip.parse::<Ipv4Addr>() {
        if !addrs.contains(&primary) {
            addrs.insert(0, primary);
        } else {
            // Move primary to front
            addrs.retain(|ip| *ip != primary);
            addrs.insert(0, primary);
        }
    }

    addrs
}

/// Encode a DNS domain name into wire format (length-prefixed labels).
fn dns_encode_name(name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for label in name.trim_end_matches('.').split('.') {
        if !label.is_empty() {
            out.push(label.len() as u8);
            out.extend_from_slice(label.as_bytes());
        }
    }
    out.push(0); // root label
    out
}

/// Build a raw mDNS announcement (unsolicited DNS response) containing
/// PTR, SRV, TXT, and A records for our pairing service.
/// Used for proactive multicast announcements (currently disabled to avoid
/// Windows DNS Client caching conflicts — kept for tests and possible future use).
#[allow(dead_code)]
fn build_mdns_announcement(
    service_name: &str,
    hostname: &str,
    port: u16,
    ip: Ipv4Addr,
) -> Vec<u8> {
    let svc_type = "_adb-tls-pairing._tcp.local.";
    let instance = format!("{}.{}", service_name, svc_type);

    let svc_enc = dns_encode_name(svc_type);
    let inst_enc = dns_encode_name(&instance);
    let host_enc = dns_encode_name(hostname);

    let mut p = Vec::with_capacity(512);

    // ── DNS Header (12 bytes) ──
    p.extend_from_slice(&[0x00, 0x00]); // Transaction ID (0 for mDNS)
    p.extend_from_slice(&[0x84, 0x00]); // Flags: QR=1 AA=1
    p.extend_from_slice(&[0x00, 0x00]); // QDCOUNT
    p.extend_from_slice(&[0x00, 0x04]); // ANCOUNT: PTR + SRV + TXT + A
    p.extend_from_slice(&[0x00, 0x00]); // NSCOUNT
    p.extend_from_slice(&[0x00, 0x00]); // ARCOUNT

    // ── PTR record: _adb-tls-pairing._tcp.local. → instance ──
    p.extend_from_slice(&svc_enc);
    p.extend_from_slice(&[0x00, 0x0C]); // Type PTR
    p.extend_from_slice(&[0x00, 0x01]); // Class IN (shared, no cache-flush)
    p.extend_from_slice(&4500u32.to_be_bytes()); // TTL
    p.extend_from_slice(&(inst_enc.len() as u16).to_be_bytes());
    p.extend_from_slice(&inst_enc);

    // ── SRV record: instance → hostname:port ──
    p.extend_from_slice(&inst_enc);
    p.extend_from_slice(&[0x00, 0x21]); // Type SRV
    p.extend_from_slice(&[0x80, 0x01]); // Class IN + cache-flush
    p.extend_from_slice(&120u32.to_be_bytes()); // TTL
    let srv_rdata_len = (6 + host_enc.len()) as u16;
    p.extend_from_slice(&srv_rdata_len.to_be_bytes());
    p.extend_from_slice(&[0x00, 0x00]); // Priority
    p.extend_from_slice(&[0x00, 0x00]); // Weight
    p.extend_from_slice(&port.to_be_bytes());
    p.extend_from_slice(&host_enc);

    // ── TXT record (empty) ──
    p.extend_from_slice(&inst_enc);
    p.extend_from_slice(&[0x00, 0x10]); // Type TXT
    p.extend_from_slice(&[0x80, 0x01]); // Class IN + cache-flush
    p.extend_from_slice(&4500u32.to_be_bytes()); // TTL
    p.extend_from_slice(&[0x00, 0x01]); // RDLENGTH 1
    p.push(0x00); // empty TXT

    // ── A record: hostname → IP ──
    p.extend_from_slice(&host_enc);
    p.extend_from_slice(&[0x00, 0x01]); // Type A
    p.extend_from_slice(&[0x80, 0x01]); // Class IN + cache-flush
    p.extend_from_slice(&120u32.to_be_bytes()); // TTL
    p.extend_from_slice(&[0x00, 0x04]); // RDLENGTH
    p.extend_from_slice(&ip.octets());

    p
}

#[allow(dead_code)]
/// Build an mDNS response for an ANY/SRV query on the specific instance name.
fn build_instance_response(
    service_name: &str,
    hostname: &str,
    port: u16,
    ip: Ipv4Addr,
) -> Vec<u8> {
    let svc_type = "_adb-tls-pairing._tcp.local.";
    let instance = format!("{}.{}", service_name, svc_type);

    let inst_enc = dns_encode_name(&instance);
    let host_enc = dns_encode_name(hostname);

    let mut p = Vec::with_capacity(256);

    // DNS Header
    p.extend_from_slice(&[0x00, 0x00]); // Transaction ID
    p.extend_from_slice(&[0x84, 0x00]); // Flags: QR=1 AA=1
    p.extend_from_slice(&[0x00, 0x00]); // QDCOUNT
    p.extend_from_slice(&[0x00, 0x02]); // ANCOUNT: SRV + TXT
    p.extend_from_slice(&[0x00, 0x00]); // NSCOUNT
    p.extend_from_slice(&[0x00, 0x01]); // ARCOUNT: A

    // Answer 1: SRV
    p.extend_from_slice(&inst_enc);
    p.extend_from_slice(&[0x00, 0x21]); // Type SRV
    p.extend_from_slice(&[0x80, 0x01]); // Class IN + cache-flush
    p.extend_from_slice(&120u32.to_be_bytes());
    let srv_rdata_len = (6 + host_enc.len()) as u16;
    p.extend_from_slice(&srv_rdata_len.to_be_bytes());
    p.extend_from_slice(&[0x00, 0x00]); // Priority
    p.extend_from_slice(&[0x00, 0x00]); // Weight
    p.extend_from_slice(&port.to_be_bytes());
    p.extend_from_slice(&host_enc);

    // Answer 2: TXT (empty)
    p.extend_from_slice(&inst_enc);
    p.extend_from_slice(&[0x00, 0x10]); // Type TXT
    p.extend_from_slice(&[0x80, 0x01]); // Class IN + cache-flush
    p.extend_from_slice(&4500u32.to_be_bytes());
    p.extend_from_slice(&[0x00, 0x01]);
    p.push(0x00);

    // Additional: A record (so phone can resolve the SRV target hostname)
    p.extend_from_slice(&host_enc);
    p.extend_from_slice(&[0x00, 0x01]); // Type A
    p.extend_from_slice(&[0x80, 0x01]); // Class IN + cache-flush
    p.extend_from_slice(&120u32.to_be_bytes());
    p.extend_from_slice(&[0x00, 0x04]);
    p.extend_from_slice(&ip.octets());

    p
}

#[allow(dead_code)]
/// Build an mDNS response containing only an A record for hostname resolution.
fn build_a_response(hostname: &str, ip: Ipv4Addr) -> Vec<u8> {
    let host_enc = dns_encode_name(hostname);
    let mut p = Vec::with_capacity(64);

    p.extend_from_slice(&[0x00, 0x00]); // Transaction ID
    p.extend_from_slice(&[0x84, 0x00]); // Flags: QR=1 AA=1
    p.extend_from_slice(&[0x00, 0x00]); // QDCOUNT
    p.extend_from_slice(&[0x00, 0x01]); // ANCOUNT: 1
    p.extend_from_slice(&[0x00, 0x00]); // NSCOUNT
    p.extend_from_slice(&[0x00, 0x00]); // ARCOUNT

    p.extend_from_slice(&host_enc);
    p.extend_from_slice(&[0x00, 0x01]); // Type A
    p.extend_from_slice(&[0x80, 0x01]); // Class IN + cache-flush
    p.extend_from_slice(&120u32.to_be_bytes());
    p.extend_from_slice(&[0x00, 0x04]);
    p.extend_from_slice(&ip.octets());

    p
}

/// Build an mDNS query packet for a given name and query type.
#[allow(dead_code)]
fn build_mdns_query(name: &str, qtype: u16) -> Vec<u8> {
    let name_enc = dns_encode_name(name);
    let mut pkt = Vec::with_capacity(12 + name_enc.len() + 4);
    // Header
    pkt.extend_from_slice(&[0x00, 0x00]); // ID
    pkt.extend_from_slice(&[0x00, 0x00]); // Flags: standard query
    pkt.extend_from_slice(&[0x00, 0x01]); // QDCOUNT: 1
    pkt.extend_from_slice(&[0x00, 0x00]); // ANCOUNT
    pkt.extend_from_slice(&[0x00, 0x00]); // NSCOUNT
    pkt.extend_from_slice(&[0x00, 0x00]); // ARCOUNT
    // Question
    pkt.extend_from_slice(&name_enc);
    pkt.extend_from_slice(&qtype.to_be_bytes()); // Type
    pkt.extend_from_slice(&[0x00, 0x01]); // Class IN
    pkt
}

/// Skip a DNS name in wire format (handles both inline labels and compression pointers).
#[allow(dead_code)]
fn dns_skip_name(data: &[u8], mut pos: usize) -> Option<usize> {
    loop {
        if pos >= data.len() { return None; }
        let len = data[pos] as usize;
        if len == 0 {
            return Some(pos + 1);
        }
        if len & 0xC0 == 0xC0 {
            // Compression pointer — 2 bytes total
            if pos + 1 >= data.len() { return None; }
            return Some(pos + 2);
        }
        pos += 1 + len;
        if pos > data.len() { return None; }
    }
}

/// Parse an mDNS response to extract SRV (port) and A (IP) records.
/// Returns `Some((ip, port))` if both are found, filtering out loopback/unspecified IPs.
#[allow(dead_code)]
fn parse_mdns_srv_and_a(data: &[u8]) -> Option<(Ipv4Addr, u16)> {
    if data.len() < 12 { return None; }
    if data[2] & 0x80 == 0 { return None; } // Must be response

    let qdcount = u16::from_be_bytes([data[4], data[5]]) as usize;
    let ancount = u16::from_be_bytes([data[6], data[7]]) as usize;
    let nscount = u16::from_be_bytes([data[8], data[9]]) as usize;
    let arcount = u16::from_be_bytes([data[10], data[11]]) as usize;

    let mut pos = 12usize;

    // Skip questions
    for _ in 0..qdcount {
        pos = dns_skip_name(data, pos)?;
        if pos + 4 > data.len() { return None; }
        pos += 4; // type + class
    }

    let mut found_port: Option<u16> = None;
    let mut found_ip: Option<Ipv4Addr> = None;
    let total_records = ancount + nscount + arcount;

    for _ in 0..total_records {
        if pos >= data.len() { break; }
        pos = dns_skip_name(data, pos)?;
        if pos + 10 > data.len() { break; }

        let rtype = u16::from_be_bytes([data[pos], data[pos + 1]]);
        let rdlength = u16::from_be_bytes([data[pos + 8], data[pos + 9]]) as usize;
        let rdata_start = pos + 10;
        let rdata_end = rdata_start + rdlength;
        if rdata_end > data.len() { break; }

        match rtype {
            33 => { // SRV: priority(2) + weight(2) + port(2) + target
                if rdlength >= 6 {
                    let port = u16::from_be_bytes([data[rdata_start + 4], data[rdata_start + 5]]);
                    found_port = Some(port);
                }
            }
            1 => { // A: 4 bytes IP
                if rdlength == 4 {
                    let ip = Ipv4Addr::new(
                        data[rdata_start],
                        data[rdata_start + 1],
                        data[rdata_start + 2],
                        data[rdata_start + 3],
                    );
                    if !ip.is_loopback() && !ip.is_unspecified() {
                        found_ip = Some(ip);
                    }
                }
            }
            _ => {}
        }

        pos = rdata_end;
    }

    match (found_ip, found_port) {
        (Some(ip), Some(port)) => Some((ip, port)),
        _ => None,
    }
}

/// Discover the phone's `_adb-tls-pairing._tcp` service via ADB's built-in
/// mDNS daemon.
///
/// Polls `adb mdns services` periodically.  ADB's mDNS implementation handles
/// all the platform-specific port 5353, multicast, and DNS Client Service
/// conflicts that plague raw-socket approaches on Windows.
///
/// Returns `(IP, port)` when the phone's pairing service matching our QR
/// service name is found.
async fn discover_via_adb_mdns(
    service_name: &str,
    adb_path: &str,
    cancel: &Arc<AtomicBool>,
    app: &tauri::AppHandle,
) -> Result<(String, u16), String> {
    let log = |msg: String| { let _ = app.emit("qr-pairing-log", &msg); };

    let start = std::time::Instant::now();
    let mut poll_count = 0u32;

    log(format!("[mDNS] Waiting for phone to register service {}…", service_name));

    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        if start.elapsed().as_secs() >= PAIRING_TIMEOUT_SECS {
            return Err(
                "QR pairing timed out — the phone never registered the pairing service.\n\
                 Make sure you scanned the QR code on the phone \
                 (Settings → Developer Options → Wireless debugging → Pair device with QR code)."
                    .into(),
            );
        }

        poll_count += 1;

        let (stdout, _stderr, _ok) =
            run_cmd_async_lenient(adb_path, &["mdns", "services"]).await.unwrap_or_default();

        // Look for our service name in the output.
        // ADB output format:
        //   adb-SERIAL123\t_adb-tls-pairing._tcp.\t192.168.1.100:37215
        for line in stdout.lines() {
            if !line.contains('\t') {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 3 {
                continue;
            }
            let name = parts[0].trim();
            let svc_type = parts[1].trim();
            let addr = parts[2].trim();

            // Match by service name AND pairing type
            if name == service_name && svc_type.contains("pairing") {
                if let Some((ip, port)) = parse_ip_port(addr) {
                    log(format!("[mDNS] ✓ Discovered phone's pairing service: {}:{}", ip, port));
                    return Ok((ip, port));
                }
            }
        }

        if poll_count <= 5 || poll_count % 10 == 0 {
            log(format!("[mDNS] → Polled ADB mDNS (attempt #{})", poll_count));
        }

        // Poll every 1.5 seconds — fast enough to catch the service promptly,
        // slow enough to not hammer ADB with commands.
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    }
}

/// Extract (IP, port) from an "ip:port" string.
fn parse_ip_port(s: &str) -> Option<(String, u16)> {
    let (ip_str, port_str) = s.rsplit_once(':')?;
    let port = port_str.parse::<u16>().ok()?;
    // Basic sanity: must contain at least one dot for IPv4
    if !ip_str.contains('.') {
        return None;
    }
    Some((ip_str.to_string(), port))
}

// Old mDNS responder kept for reference (no longer used — we are now a client/browser)
#[allow(dead_code)]
async fn raw_mdns_announce_loop(
    service_name: String,
    hostname: String,
    port: u16,
    ip_str: String,
    all_ips: Vec<Ipv4Addr>,
    cancel: Arc<AtomicBool>,
    app: tauri::AppHandle,
) {
    let log = |msg: String| { let _ = app.emit("qr-pairing-log", &msg); };

    let primary_ip: Ipv4Addr = match ip_str.parse() {
        Ok(v) => v,
        Err(e) => { log(format!("[raw-mDNS] Bad IP '{}': {}", ip_str, e)); return; }
    };

    let inst_response = build_instance_response(&service_name, &hostname, port, primary_ip);
    let a_response = build_a_response(&hostname, primary_ip);
    let mcast = Ipv4Addr::new(224, 0, 0, 251);

    // DNS wire-format markers for detecting relevant queries
    let svc_marker = dns_encode_name("_adb-tls-pairing._tcp.local.");
    let host_marker = dns_encode_name(&hostname);

    // Build a set of our own IPs so we can ignore queries from ourselves / ADB on our PC
    let own_ips: std::collections::HashSet<Ipv4Addr> = all_ips.iter().copied().collect();

    // Create socket with SO_REUSEADDR so we can share port 5353 with other
    // mDNS daemons (Windows DNS Client, ADB, browsers, etc.)
    let socket = match socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    ) {
        Ok(s) => s,
        Err(e) => { log(format!("[raw-mDNS] Socket creation failed: {}", e)); return; }
    };

    let _ = socket.set_reuse_address(true);
    if let Err(e) = socket.bind(&socket2::SockAddr::from(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 5353))) {
        log(format!("[raw-mDNS] ⚠ Bind to :5353 failed ({}), trying ephemeral…", e));
        if let Err(e2) = socket.bind(&socket2::SockAddr::from(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))) {
            log(format!("[raw-mDNS] Bind failed entirely: {}", e2));
            return;
        }
    }

    let _ = socket.set_nonblocking(true);
    // RFC 6762 §11: ALL mDNS responses (including unicast) MUST have IP TTL=255.
    // Android's mDNSResponder silently drops packets with TTL != 255.
    match socket.set_ttl(255) {
        Ok(()) => {},
        Err(e) => log(format!("[raw-mDNS] ⚠ Failed to set TTL=255: {} — Android may ignore our responses!", e)),
    }

    // Disable multicast loopback — we never send multicast, but this also
    // prevents us from seeing our own packets echoed back by the OS.
    let _ = socket.set_multicast_loop_v4(false);

    // Join multicast group on every interface so we RECEIVE queries from all segments
    for ip in &all_ips {
        let _ = socket.join_multicast_v4(&mcast, ip);
    }

    let iface_list: Vec<String> = all_ips.iter().map(|ip| ip.to_string()).collect();
    log(format!("[raw-mDNS] ✓ Listening on {} interface(s): {}", all_ips.len(), iface_list.join(", ")));
    log(format!("[raw-mDNS] Service: {} → {}:{} (unicast-only, no multicast)", service_name, ip_str, port));

    let mut query_count = 0u32;

    while !cancel.load(Ordering::Relaxed) {
        // ── Drain incoming mDNS packets — respond to queries for our service or hostname ──
        loop {
            let mut buf = [std::mem::MaybeUninit::<u8>::uninit(); 1500];
            match socket.recv_from(&mut buf) {
                Ok((len, from_addr)) => {
                    let data = unsafe {
                        std::slice::from_raw_parts(buf.as_ptr() as *const u8, len)
                    };
                    // Only process DNS queries (QR bit = 0)
                    if len <= 12 || data[2] & 0x80 != 0 {
                        continue;
                    }

                    // Ignore queries from our own PC (Windows DNS Client, ADB, etc.)
                    // Only respond to queries from OTHER hosts (i.e. the phone).
                    if let Some(sock_addr) = from_addr.as_socket_ipv4() {
                        if own_ips.contains(sock_addr.ip()) {
                            continue;
                        }
                    }

                    let is_svc_query = data.windows(svc_marker.len()).any(|w| w == svc_marker.as_slice());
                    let is_host_query = data.windows(host_marker.len()).any(|w| w == host_marker.as_slice());

                    if is_svc_query || is_host_query {
                        query_count += 1;
                        let addr_str = from_addr.as_socket()
                            .map(|a| a.to_string())
                            .unwrap_or_else(|| "?".into());

                        // Parse query name + type for diagnostics
                        let qinfo = {
                            let mut pos = 12usize;
                            let mut labels: Vec<String> = Vec::new();
                            while pos < len {
                                let ll = data[pos] as usize;
                                if ll == 0 { pos += 1; break; }
                                pos += 1;
                                if pos + ll <= len {
                                    labels.push(String::from_utf8_lossy(&data[pos..pos+ll]).to_string());
                                    pos += ll;
                                } else { break; }
                            }
                            let qtype = if pos + 2 <= len { u16::from_be_bytes([data[pos], data[pos+1]]) } else { 0 };
                            let qclass = if pos + 4 <= len { u16::from_be_bytes([data[pos+2], data[pos+3]]) } else { 0 };
                            format!("{} type={} class=0x{:04x}", labels.join("."), qtype, qclass)
                        };

                        // Choose the right response based on query type
                        let (response, kind) = if is_svc_query {
                            // Instance query (ANY/SRV for service) → SRV+TXT in answers, A in additional
                            (&inst_response, "instance(SRV+TXT+A)")
                        } else {
                            // Hostname query (A for yeshua1.local.) → A record only
                            (&a_response, "hostname(A)")
                        };

                        log(format!("[raw-mDNS] ← Query #{} from {}: {} → responding with {}", query_count, addr_str, qinfo, kind));

                        // Send UNICAST response directly to querier (TTL=255 set on socket).
                        // We intentionally do NOT multicast — see doc comment above.
                        match socket.send_to(response, &from_addr) {
                            Ok(n) => log(format!("[raw-mDNS]   → Sent {} bytes unicast to {}", n, addr_str)),
                            Err(e) => log(format!("[raw-mDNS]   → Unicast send FAILED: {}", e)),
                        }
                        // Brief pause, then send again for reliability (UDP is unreliable)
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        let _ = socket.send_to(response, &from_addr);
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }

        // Brief sleep — poll ~10×/sec for responsive query handling
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

// ─── TLS ─────────────────────────────────────────────────────────────────────

/// TLS certificate verifier that accepts any certificate (for ADB pairing).
/// ADB pairing uses self-signed certificates with `SSL_VERIFY_NONE`.
#[derive(Debug)]
#[allow(dead_code)]
struct NoVerifier;

impl tokio_rustls::rustls::client::danger::ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[tokio_rustls::rustls::pki_types::CertificateDer<'_>],
        _server_name: &tokio_rustls::rustls::pki_types::ServerName<'_>,
        _ocsp: &[u8],
        _now: tokio_rustls::rustls::pki_types::UnixTime,
    ) -> Result<
        tokio_rustls::rustls::client::danger::ServerCertVerified,
        tokio_rustls::rustls::Error,
    > {
        Ok(tokio_rustls::rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _dss: &tokio_rustls::rustls::DigitallySignedStruct,
    ) -> Result<
        tokio_rustls::rustls::client::danger::HandshakeSignatureValid,
        tokio_rustls::rustls::Error,
    > {
        Ok(tokio_rustls::rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _dss: &tokio_rustls::rustls::DigitallySignedStruct,
    ) -> Result<
        tokio_rustls::rustls::client::danger::HandshakeSignatureValid,
        tokio_rustls::rustls::Error,
    > {
        Ok(tokio_rustls::rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<tokio_rustls::rustls::SignatureScheme> {
        use tokio_rustls::rustls::SignatureScheme;
        vec![
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
        ]
    }
}

/// Build a [`TlsConnector`] for connecting to the phone's pairing server.
/// Uses TLS 1.3 only with no certificate verification (matching AOSP).
#[allow(dead_code)]
fn build_tls_connector() -> Result<TlsConnector, String> {
    let config = tokio_rustls::rustls::ClientConfig::builder_with_protocol_versions(
        &[&tokio_rustls::rustls::version::TLS13],
    )
    .dangerous()
    .with_custom_certificate_verifier(Arc::new(NoVerifier))
    .with_no_client_auth();

    Ok(TlsConnector::from(Arc::new(config)))
}

#[allow(dead_code)]
/// Generate a self-signed TLS certificate and build a [`TlsAcceptor`].
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

/// Result of the firewall setup attempt.
#[derive(Clone, Copy, PartialEq)]
#[allow(dead_code)]
enum FirewallStatus {
    /// Temporary port-specific rules were added (should be cleaned up).
    TempRulesAdded,
    /// A persistent program-level rule is active (don't clean up).
    ProgramRuleOk,
    /// No rules could be added.
    Failed,
}

/// Persistent program-level rule name (survives across sessions).
const PROGRAM_RULE_NAME: &str = "Android Application Installer";

/// Check if our persistent program-level firewall rule already exists.
/// `netsh show rule` doesn't require admin.
#[cfg(target_os = "windows")]
async fn check_program_rule_exists() -> bool {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let result = tokio::process::Command::new("netsh")
        .args([
            "advfirewall", "firewall", "show", "rule",
            &format!("name={}", PROGRAM_RULE_NAME),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await;

    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            output.status.success() && stdout.contains(PROGRAM_RULE_NAME)
        }
        Err(_) => false,
    }
}

/// Try to add temporary port-specific inbound rules (TCP for pairing +
/// UDP 5353 for mDNS).  Requires admin — will fail silently if not elevated.
#[cfg(target_os = "windows")]
#[allow(dead_code)]
async fn try_add_temp_rules(port: u16) -> bool {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let tcp_name = format!("ADB QR Pairing TCP (port {})", port);
    let tcp_ok = tokio::process::Command::new("netsh")
        .args([
            "advfirewall", "firewall", "add", "rule",
            &format!("name={}", tcp_name),
            "dir=in", "action=allow", "protocol=tcp",
            &format!("localport={}", port),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;

    if !matches!(tcp_ok, Ok(s) if s.success()) {
        return false;
    }

    // Also add UDP 5353 for mDNS so the phone's multicast queries reach us
    let _ = tokio::process::Command::new("netsh")
        .args([
            "advfirewall", "firewall", "add", "rule",
            "name=ADB QR Pairing mDNS",
            "dir=in", "action=allow", "protocol=udp",
            "localport=5353",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;

    true
}

/// Try to add a persistent program-level firewall rule with UAC elevation.
/// Shows a UAC prompt to the user — if accepted the rule persists across sessions.
#[cfg(target_os = "windows")]
async fn try_add_program_rule_elevated() -> bool {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let exe_path = match std::env::current_exe() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(_) => return false,
    };

    // Use PowerShell Start-Process -Verb RunAs to trigger UAC elevation.
    // Single-quoted ArgumentList preserves the inner double-quotes for netsh.
    let netsh_args = format!(
        "advfirewall firewall add rule name=\"{}\" dir=in action=allow program=\"{}\" enable=yes",
        PROGRAM_RULE_NAME, exe_path
    );
    let ps_command = format!(
        "Start-Process netsh -ArgumentList '{}' -Verb RunAs -Wait -WindowStyle Hidden",
        netsh_args.replace('\'', "''")
    );

    let result = tokio::process::Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &ps_command])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;

    matches!(result, Ok(s) if s.success())
}

/// Remove the temporary port-specific firewall rules added by [`try_add_temp_rules`].
#[cfg(target_os = "windows")]
#[allow(dead_code)]
async fn try_remove_temp_rules(port: u16) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let tcp_name = format!("ADB QR Pairing TCP (port {})", port);
    let _ = tokio::process::Command::new("netsh")
        .args(["advfirewall", "firewall", "delete", "rule", &format!("name={}", tcp_name)])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;

    let _ = tokio::process::Command::new("netsh")
        .args(["advfirewall", "firewall", "delete", "rule", "name=ADB QR Pairing mDNS"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;
}

/// No-op on non-Windows platforms.
#[cfg(not(target_os = "windows"))]
async fn check_program_rule_exists() -> bool { false }
#[cfg(not(target_os = "windows"))]
async fn try_add_temp_rules(_port: u16) -> bool { false }
#[cfg(not(target_os = "windows"))]
async fn try_add_program_rule_elevated() -> bool { false }
#[cfg(not(target_os = "windows"))]
async fn try_remove_temp_rules(_port: u16) {}

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
        if check_program_rule_exists().await {
            log("✓ Firewall: program rule active".into());
        } else {
            log("Requesting firewall access (you may see a UAC prompt)…".into());
            if try_add_program_rule_elevated().await {
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
    let (phone_ip, phone_port) = discover_via_adb_mdns(
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

    #[test]
    fn dns_encode_name_basic() {
        let enc = dns_encode_name("local.");
        assert_eq!(enc, vec![5, b'l', b'o', b'c', b'a', b'l', 0]);
    }

    #[test]
    fn dns_encode_name_multi_label() {
        let enc = dns_encode_name("_tcp.local.");
        assert_eq!(enc, vec![4, b'_', b't', b'c', b'p', 5, b'l', b'o', b'c', b'a', b'l', 0]);
    }

    #[test]
    fn dns_encode_name_no_trailing_dot() {
        // Should produce same result with or without trailing dot
        assert_eq!(dns_encode_name("local."), dns_encode_name("local"));
    }

    #[test]
    fn build_mdns_announcement_creates_valid_packet() {
        let pkt = build_mdns_announcement(
            "adb-test1234",
            "myhost.local.",
            12345,
            Ipv4Addr::new(192, 168, 0, 100),
        );
        // DNS header: 12 bytes, then records
        assert!(pkt.len() > 12);
        // Flags: 0x8400 (QR=1, AA=1)
        assert_eq!(pkt[2], 0x84);
        assert_eq!(pkt[3], 0x00);
        // ANCOUNT: 4
        assert_eq!(pkt[6], 0x00);
        assert_eq!(pkt[7], 0x04);
        // Packet should contain the service name and IP
        let pkt_str = String::from_utf8_lossy(&pkt);
        assert!(pkt_str.contains("adb-test1234"));
        // IP bytes should be present: 192.168.0.100
        assert!(pkt.windows(4).any(|w| w == [192, 168, 0, 100]));
    }

    #[test]
    fn parse_ip_port_valid() {
        assert_eq!(
            parse_ip_port("192.168.0.23:37215"),
            Some(("192.168.0.23".to_string(), 37215))
        );
    }

    #[test]
    fn parse_ip_port_no_port() {
        assert_eq!(parse_ip_port("192.168.0.23"), None);
    }

    #[test]
    fn parse_ip_port_bad_port() {
        assert_eq!(parse_ip_port("192.168.0.23:abc"), None);
    }

    #[test]
    fn parse_ip_port_no_dot() {
        assert_eq!(parse_ip_port("localhost:5555"), None);
    }
}

