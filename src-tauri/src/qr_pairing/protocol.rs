//! SPAKE2 key exchange, AES-128-GCM encryption, wire protocol, and pairing handlers.
//!
//! This implements the full ADB pairing protocol (SPAKE2 + encrypted PeerInfo
//! exchange) for reference and tests. The primary code path delegates to
//! `adb pair` instead, but this module is kept so that the crypto can be
//! unit-tested independently.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes128Gcm, Key, Nonce};
use curve25519_dalek::edwards::{CompressedEdwardsY, EdwardsPoint};
use curve25519_dalek::scalar::Scalar;
use hkdf::Hkdf;
use sha2::{Digest, Sha256, Sha512};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
}
