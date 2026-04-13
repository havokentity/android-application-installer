//! mDNS service discovery and raw DNS packet building/parsing.
//!
//! Contains both the primary discovery path (polling `adb mdns services`) and
//! a raw mDNS responder fallback for environments where ADB's built-in mDNS
//! daemon cannot reach the phone (e.g. cross-segment multicast issues).

use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::Emitter;

use crate::cmd::run_cmd_async_lenient;

/// QR pairing timeout in seconds.
const PAIRING_TIMEOUT_SECS: u64 = 120;

// ─── Network Helpers ─────────────────────────────────────────────────────────

/// Detect all usable IPv4 addresses on this machine.
/// On Windows, parses `ipconfig` output. Falls back to the primary IP.
#[allow(dead_code)]
pub(super) async fn get_all_ipv4_addrs(primary_ip: &str) -> Vec<Ipv4Addr> {
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

// ─── DNS Wire Format ─────────────────────────────────────────────────────────

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

// ─── DNS Packet Parsing ──────────────────────────────────────────────────────

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

// ─── Service Discovery ───────────────────────────────────────────────────────

/// Discover the phone's `_adb-tls-pairing._tcp` service via ADB's built-in
/// mDNS daemon.
///
/// Polls `adb mdns services` periodically.  ADB's mDNS implementation handles
/// all the platform-specific port 5353, multicast, and DNS Client Service
/// conflicts that plague raw-socket approaches on Windows.
///
/// Returns `(IP, port)` when the phone's pairing service matching our QR
/// service name is found.
pub(super) async fn discover_via_adb_mdns(
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

// ─── Raw mDNS Responder (fallback) ──────────────────────────────────────────
//
// When the mdns-sd daemon can't properly handle port 5353 sharing on Windows
// (common when the Windows DNS Client service or another mDNS daemon holds the
// port), OR when multicast doesn't traverse between wired Ethernet and WiFi
// segments, this fallback sends gratuitous mDNS announcements directly via a
// raw UDP socket on EVERY network interface.  The phone picks up these
// unsolicited responses and discovers our pairing service.

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

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
