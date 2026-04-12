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
use std::net::Ipv4Addr;
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

// ─── Raw mDNS Fallback ──────────────────────────────────────────────────────
//
// When the mdns-sd daemon can't properly handle port 5353 sharing on Windows
// (common when the Windows DNS Client service or another mDNS daemon holds the
// port), this fallback sends gratuitous mDNS announcements directly via a raw
// UDP socket.  The phone picks up these unsolicited responses and discovers
// our pairing service without needing the mdns-sd daemon to work at all.

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

/// Periodically send gratuitous mDNS announcements via a raw UDP socket,
/// bypassing the mdns-sd daemon entirely.
async fn raw_mdns_announce_loop(
    service_name: String,
    hostname: String,
    port: u16,
    ip_str: String,
    cancel: Arc<AtomicBool>,
    app: tauri::AppHandle,
) {
    let log = |msg: String| { let _ = app.emit("qr-pairing-log", &msg); };

    let ip: Ipv4Addr = match ip_str.parse() {
        Ok(v) => v,
        Err(e) => { log(format!("[raw-mDNS] Bad IP '{}': {}", ip_str, e)); return; }
    };

    let packet = build_mdns_announcement(&service_name, &hostname, port, ip);

    // Try binding to port 5353 (RFC 6762 requires source port 5353 for mDNS
    // responses).  Fall back to ephemeral if 5353 is taken.
    let socket = match std::net::UdpSocket::bind("0.0.0.0:5353") {
        Ok(s) => { log("[raw-mDNS] ✓ Bound to source port 5353".into()); s }
        Err(_) => match std::net::UdpSocket::bind("0.0.0.0:0") {
            Ok(s) => {
                let p = s.local_addr().map(|a| a.port()).unwrap_or(0);
                log(format!("[raw-mDNS] ⚠ Port 5353 taken, using ephemeral port {} (some devices may ignore)", p));
                s
            }
            Err(e) => { log(format!("[raw-mDNS] Socket bind failed: {}", e)); return; }
        }
    };

    let _ = socket.join_multicast_v4(&Ipv4Addr::new(224, 0, 0, 251), &ip);
    let _ = socket.set_multicast_ttl_v4(255);
    let _ = socket.set_nonblocking(true);

    let dest: std::net::SocketAddr = (Ipv4Addr::new(224, 0, 0, 251), 5353).into();
    log(format!("[raw-mDNS] Announcing {} → {}:{} ({} bytes/pkt)", service_name, ip_str, port, packet.len()));

    while !cancel.load(Ordering::Relaxed) {
        match socket.send_to(&packet, dest) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => { log(format!("[raw-mDNS] send error: {}", e)); }
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
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

/// Result of the firewall setup attempt.
#[derive(Clone, Copy, PartialEq)]
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

    // 0. Ensure Windows Firewall allows inbound connections (TCP for pairing,
    //    UDP 5353 for mDNS).  The phone makes an INBOUND TCP connection to our
    //    port AND discovers us via mDNS multicast — both are blocked by default.
    let fw_status = if cfg!(target_os = "windows") {
        if check_program_rule_exists().await {
            log("✓ Firewall: program rule active".into());
            FirewallStatus::ProgramRuleOk
        } else if try_add_temp_rules(port).await {
            log(format!("✓ Firewall rules added for TCP port {} and mDNS", port));
            FirewallStatus::TempRulesAdded
        } else {
            log("Requesting firewall access (you may see a UAC prompt)…".into());
            if try_add_program_rule_elevated().await {
                log("✓ Firewall: program rule added (persists for future sessions)".into());
                FirewallStatus::ProgramRuleOk
            } else {
                let exe_hint = std::env::current_exe()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| "<app path>".into());
                log("⚠ Could not add firewall rule. Pairing will likely fail.".into());
                log("  → Run this command in an admin terminal to fix permanently:".into());
                log(format!(
                    "    netsh advfirewall firewall add rule name=\"{}\" dir=in action=allow program=\"{}\" enable=yes",
                    PROGRAM_RULE_NAME, exe_hint
                ));
                FirewallStatus::Failed
            }
        }
    } else {
        FirewallStatus::Failed // non-Windows, no firewall handling needed
    };

    // 1. Kill ADB server to free mDNS port 5353.
    //    On Windows, ADB's built-in mDNS daemon and our mdns-sd daemon both
    //    bind to UDP 5353.  Windows delivers incoming multicast packets to
    //    only ONE of the competing sockets, so the phone's mDNS query for
    //    our pairing service gets swallowed by ADB's daemon (which ignores
    //    it).  Killing ADB temporarily gives our daemon exclusive access.
    log("Stopping ADB server temporarily (freeing mDNS port)…".into());
    #[cfg(target_os = "windows")]
    {
        const CNW: u32 = 0x0800_0000;
        // Graceful kill-server (don't use run_cmd_async_lenient — it checks
        // the global OPERATION_CANCEL flag which may be set)
        let _ = tokio::process::Command::new(adb_path)
            .args(["kill-server"])
            .creation_flags(CNW)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await;
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        // Force-kill any remaining adb.exe processes (belt-and-suspenders)
        let taskkill = tokio::process::Command::new("taskkill")
            .args(["/F", "/IM", "adb.exe"])
            .creation_flags(CNW)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await;
        if matches!(taskkill, Ok(ref s) if s.success()) {
            log("  (force-killed remaining adb.exe processes)".into());
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = tokio::process::Command::new(adb_path)
            .args(["kill-server"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await;
    }
    // Wait for OS to fully release the UDP 5353 socket
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Diagnostic: check what's still using UDP port 5353
    #[cfg(target_os = "windows")]
    {
        const CNW: u32 = 0x0800_0000;
        if let Ok(output) = tokio::process::Command::new("cmd")
            .args(["/C", "netstat -anop udp | findstr :5353"])
            .creation_flags(CNW)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .await
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.trim().is_empty() {
                log("✓ Port 5353 is free — no competing mDNS daemons".into());
            } else {
                for line in stdout.lines().filter(|l| !l.trim().is_empty()) {
                    log(format!("⚠ Port 5353 still in use: {}", line.trim()));
                }
                // Identify the process(es) holding the port
                for line in stdout.lines().filter(|l| !l.trim().is_empty()) {
                    if let Some(pid) = line.split_whitespace().last() {
                        if let Ok(task_out) = tokio::process::Command::new("cmd")
                            .args(["/C", &format!("tasklist /FI \"PID eq {}\" /FO CSV /NH", pid)])
                            .creation_flags(CNW)
                            .stdout(std::process::Stdio::piped())
                            .stderr(std::process::Stdio::null())
                            .output()
                            .await
                        {
                            let name = String::from_utf8_lossy(&task_out.stdout);
                            if !name.trim().is_empty() {
                                log(format!("  → Process: {}", name.lines().next().unwrap_or("").trim()));
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Build ephemeral TLS acceptor (self-signed cert, TLS 1.3 only, no client-cert verify)
    let tls_acceptor = build_tls_acceptor()?;
    log("✓ TLS 1.3 certificate generated".into());

    // 3. Start mDNS advertisement
    //    The phone discovers our pairing server via mDNS after scanning the QR.
    //    Passing an empty IP lets the library auto-detect ALL host addresses,
    //    so both WiFi and Ethernet IPs appear in the A/AAAA records.
    let mdns = mdns_sd::ServiceDaemon::new()
        .map_err(|e| format!("mDNS daemon failed to start: {}", e))?;

    // Enable multicast loopback so our self-test can verify discovery
    match mdns.set_multicast_loop_v4(true) {
        Ok(()) => {}
        Err(e) => log(format!("  ⚠ multicast loopback setting failed: {:?}", e)),
    }

    let hostname = {
        let machine = std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| service_name.to_string())
            .to_lowercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect::<String>();
        if machine.len() < 2 { service_name.to_string() } else { format!("{}.local.", machine) }
    };
    let service_info = mdns_sd::ServiceInfo::new(
        "_adb-tls-pairing._tcp.local.",
        service_name,
        &hostname,
        "",   // empty → register() auto-fills from ALL host interfaces
        port,
        None::<std::collections::HashMap<String, String>>,
    )
    .map_err(|e| format!("mDNS service creation failed: {}", e))?;

    let fullname = service_info.get_fullname().to_string();
    mdns.register(service_info.clone())
        .map_err(|e| format!("mDNS registration failed: {}", e))?;

    log(format!("✓ mDNS service registered: {} (host {}, port {})", service_name, &hostname, port));
    log(format!("  (IPs auto-detected from all interfaces; TCP listener at 0.0.0.0:{})", port));

    // 2b. Start mDNS event monitor — forward daemon events as diagnostic logs
    //     so we can see if queries arrive and responses are sent.
    let monitor_app = app.clone();
    let monitor_cancel = cancel.clone();
    if let Ok(monitor_rx) = mdns.monitor() {
        tokio::task::spawn_blocking(move || {
            while !monitor_cancel.load(Ordering::Relaxed) {
                match monitor_rx.recv_timeout(std::time::Duration::from_secs(2)) {
                    Ok(event) => {
                        let msg = format!("[mDNS] {:?}", event);
                        // Truncate very long messages (e.g. full DNS packets)
                        let msg = if msg.len() > 300 { format!("{}…", &msg[..300]) } else { msg };
                        let _ = monitor_app.emit("qr-pairing-log", &msg);
                    }
                    Err(_) => {
                        // Timeout or disconnected — sleep briefly to avoid spinning
                        // if the channel was closed, then check cancel flag next iteration
                        std::thread::sleep(std::time::Duration::from_millis(200));
                    }
                }
            }
        });
    }

    // 2c. Self-test: browse for our own service to verify it's discoverable
    let self_test_passed = if let Ok(browse_rx) = mdns.browse("_adb-tls-pairing._tcp.local.") {
        let found = tokio::task::spawn_blocking({
            let target = fullname.clone();
            move || {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
                while std::time::Instant::now() < deadline {
                    match browse_rx.recv_timeout(std::time::Duration::from_millis(500)) {
                        Ok(mdns_sd::ServiceEvent::ServiceResolved(info)) => {
                            if info.get_fullname() == target {
                                return true;
                            }
                        }
                        Ok(_) => continue,
                        Err(_) => continue,
                    }
                }
                false
            }
        })
        .await
        .unwrap_or(false);
        let _ = mdns.stop_browse("_adb-tls-pairing._tcp.local.");
        if found {
            log("✓ mDNS self-test passed — service is discoverable".into());
        } else {
            log("⚠ mDNS self-test: could not verify discovery".into());
        }
        found
    } else {
        log("⚠ mDNS self-test: browse failed to start".into());
        false
    };

    // 2d. If mdns-sd daemon can't discover its own service, start a raw mDNS
    //     announcer as fallback.  This sends gratuitous DNS response packets
    //     directly to the multicast group, bypassing the daemon's socket issues.
    let raw_announcer = if !self_test_passed {
        log("Starting raw mDNS announcer as fallback…".into());
        Some(tokio::spawn(raw_mdns_announce_loop(
            service_name.to_string(),
            hostname.clone(),
            port,
            local_ip.to_string(),
            cancel.clone(),
            app.clone(),
        )))
    } else {
        None
    };

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

                // Re-announce mDNS more aggressively initially:
                // every 5 s for the first 30 s, then every 15 s
                let reannounce_interval = if elapsed < 30 { 5 } else { 15 };
                if last_reannounce.elapsed().as_secs() >= reannounce_interval {
                    last_reannounce = std::time::Instant::now();
                    let re_info = mdns_sd::ServiceInfo::new(
                        "_adb-tls-pairing._tcp.local.",
                        service_name,
                        &hostname,
                        "",   // auto-detect IPs
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
                    if cfg!(target_os = "windows") && fw_status == FirewallStatus::Failed {
                        log("  • Windows Firewall is blocking inbound connections (most likely cause)".into());
                        log("    → Accept the UAC prompt if it appears, or add a firewall rule manually".into());
                    }
                    log("  • mDNS multicast may not traverse between WiFi and wired LAN".into());
                    log("    → Most routers bridge multicast fine; if yours doesn't, try connecting your PC via WiFi".into());
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

    // Clean up raw announcer
    if let Some(h) = raw_announcer { h.abort(); }

    // Clean up mDNS regardless of result
    let _ = mdns.unregister(&fullname);
    let _ = mdns.shutdown();

    // Clean up temporary firewall rules (program-level rules persist intentionally)
    if fw_status == FirewallStatus::TempRulesAdded {
        try_remove_temp_rules(port).await;
    }

    // Restart ADB server (was killed earlier to free mDNS port 5353)
    log("Restarting ADB server…".into());
    let _ = run_cmd_async_lenient(adb_path, &["start-server"]).await;

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
}

