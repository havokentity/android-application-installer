//! Windows Firewall rule management for QR pairing.

/// Result of the firewall setup attempt.
#[derive(Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub(super) enum FirewallStatus {
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
pub(super) async fn check_program_rule_exists() -> bool {
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
pub(super) async fn try_add_temp_rules(port: u16) -> bool {
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
pub(super) async fn try_add_program_rule_elevated() -> bool {
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
pub(super) async fn try_remove_temp_rules(port: u16) {
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
pub(super) async fn check_program_rule_exists() -> bool { false }
#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
pub(super) async fn try_add_temp_rules(_port: u16) -> bool { false }
#[cfg(not(target_os = "windows"))]
pub(super) async fn try_add_program_rule_elevated() -> bool { false }
#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
pub(super) async fn try_remove_temp_rules(_port: u16) {}
