//! Firewall rule management for QR pairing (Windows + macOS).

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
#[cfg(target_os = "windows")]
const PROGRAM_RULE_NAME: &str = "Android Application Installer";

// ─── Cross-platform Entry Point ──────────────────────────────────────────────

/// Ensure firewall access for QR pairing. Returns diagnostic log messages.
/// Handles Windows Firewall and macOS Application Firewall automatically.
pub(super) async fn ensure_firewall_access(adb_path: &str) -> Vec<String> {
    let _ = adb_path; // used on macOS only; suppress warning on other platforms
    let mut logs = Vec::new();

    #[cfg(target_os = "windows")]
    {
        if check_program_rule_exists().await {
            logs.push("✓ Firewall: program rule active".into());
        } else {
            logs.push("Requesting firewall access (you may see a UAC prompt)…".into());
            if try_add_program_rule_elevated().await {
                logs.push("✓ Firewall: program rule added (persists for future sessions)".into());
            } else {
                logs.push("⚠ Could not add firewall rule. mDNS discovery may be affected.".into());
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        ensure_macos_firewall_inner(adb_path, &mut logs).await;
    }

    logs
}

// ─── Windows Firewall ────────────────────────────────────────────────────────

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

// ─── macOS Application Firewall ──────────────────────────────────────────────

/// Path to the macOS Application Firewall CLI.
#[cfg(target_os = "macos")]
const SOCKETFILTERFW: &str = "/usr/libexec/ApplicationFirewall/socketfilterfw";

/// Orchestrate macOS firewall checks for both our app and the ADB binary.
#[cfg(target_os = "macos")]
async fn ensure_macos_firewall_inner(adb_path: &str, logs: &mut Vec<String>) {
    if !is_macos_firewall_enabled().await {
        logs.push("✓ macOS firewall is disabled — no rules needed".into());
        return;
    }

    logs.push("macOS Application Firewall is enabled, checking access…".into());

    // Check/allow our app binary
    if let Ok(exe) = std::env::current_exe() {
        check_and_allow_app(&exe.to_string_lossy(), "App", logs).await;
    }

    // Check/allow the ADB binary (resolve to full path if needed)
    let adb_full = if adb_path.contains('/') {
        Some(adb_path.to_string())
    } else {
        resolve_binary_path(adb_path).await
    };
    if let Some(ref path) = adb_full {
        check_and_allow_app(path, "ADB", logs).await;
    }
}

/// Check if the macOS Application Firewall is enabled.
#[cfg(target_os = "macos")]
async fn is_macos_firewall_enabled() -> bool {
    let result = tokio::process::Command::new(SOCKETFILTERFW)
        .args(["--getglobalstate"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await;

    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.contains("State = 1") || stdout.to_lowercase().contains("enabled")
        }
        Err(_) => false,
    }
}

/// Check if a specific application is allowed through the macOS firewall.
/// Returns `Some(true)` if permitted, `Some(false)` if blocked, `None` if not listed.
#[cfg(target_os = "macos")]
async fn is_app_allowed(app_path: &str) -> Option<bool> {
    let result = tokio::process::Command::new(SOCKETFILTERFW)
        .args(["--getappblocked", app_path])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await;

    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_lowercase();
            if stdout.contains("permitted") || stdout.contains("allow") {
                Some(true)
            } else if stdout.contains("blocked") {
                Some(false)
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

/// Try to allow an application through the macOS firewall.
/// Uses `osascript` to request admin privileges (shows a password dialog).
#[cfg(target_os = "macos")]
async fn try_allow_app(app_path: &str) -> bool {
    let escaped = app_path.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        r#"do shell script "{fw} --add \"{path}\" && {fw} --unblockapp \"{path}\"" with administrator privileges"#,
        fw = SOCKETFILTERFW,
        path = escaped,
    );

    let result = tokio::process::Command::new("osascript")
        .args(["-e", &script])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;

    matches!(result, Ok(s) if s.success())
}

/// Resolve a command name (e.g. `"adb"`) to its full path via `which`.
#[cfg(target_os = "macos")]
async fn resolve_binary_path(cmd: &str) -> Option<String> {
    let result = tokio::process::Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await;

    match result {
        Ok(output) if output.status.success() => {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if path.is_empty() { None } else { Some(path) }
        }
        _ => None,
    }
}

/// Check if an app is allowed through the firewall; if blocked, try to allow it.
#[cfg(target_os = "macos")]
async fn check_and_allow_app(path: &str, label: &str, logs: &mut Vec<String>) {
    match is_app_allowed(path).await {
        Some(true) => logs.push(format!("✓ {} is allowed through firewall", label)),
        Some(false) => {
            logs.push(format!("⚠ {} is blocked by firewall — requesting access…", label));
            if try_allow_app(path).await {
                logs.push(format!("✓ {} added to firewall exceptions", label));
            } else {
                logs.push(format!(
                    "⚠ Could not add {} to firewall exceptions (admin access may be needed)",
                    label
                ));
            }
        }
        None => logs.push(format!("{} not in firewall list (should be allowed)", label)),
    }
}

// ─── Stubs for other platforms ───────────────────────────────────────────────

/// No-op on non-Windows/non-macOS platforms.
#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
pub(super) async fn check_program_rule_exists() -> bool { false }
#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
pub(super) async fn try_add_temp_rules(_port: u16) -> bool { false }
#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
pub(super) async fn try_add_program_rule_elevated() -> bool { false }
#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
pub(super) async fn try_remove_temp_rules(_port: u16) {}
