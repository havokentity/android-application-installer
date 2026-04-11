//! Command execution infrastructure: sync/async process runners, cancellation,
//! and progress-event helpers shared across the application.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use tauri::Emitter;

// ─── Data Types ──────────────────────────────────────────────────────────────

/// Progress event emitted during async operations (install, launch, uninstall).
#[derive(Debug, Clone, Serialize)]
pub struct OperationProgress {
    pub operation: String,
    pub device: String,
    pub status: String, // "running" | "done" | "cancelled" | "error"
    pub message: String,
    pub step: Option<u32>,
    pub total_steps: Option<u32>,
    pub cancellable: bool,
}

/// Global cancellation flag for long-running async operations (legacy, used by wireless ADB).
pub(crate) static OPERATION_CANCEL: AtomicBool = AtomicBool::new(false);

// ─── Per-Operation Cancellation Registry ─────────────────────────────────────

static CANCEL_TOKEN_COUNTER: AtomicU64 = AtomicU64::new(0);
static CANCEL_REGISTRY: OnceLock<std::sync::Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();

fn cancel_registry() -> &'static std::sync::Mutex<HashMap<String, Arc<AtomicBool>>> {
    CANCEL_REGISTRY.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// Get the cancel flag for a given token. Falls back to a non-cancellable flag
/// if the token is missing or invalid.
pub(crate) fn get_cancel_flag(token: &Option<String>) -> Arc<AtomicBool> {
    if let Some(ref t) = token {
        if let Ok(reg) = cancel_registry().lock() {
            if let Some(flag) = reg.get(t) {
                return flag.clone();
            }
        }
    }
    Arc::new(AtomicBool::new(false))
}

// ─── Platform Binaries ───────────────────────────────────────────────────────

pub(crate) fn adb_binary() -> &'static str {
    if cfg!(target_os = "windows") {
        "adb.exe"
    } else {
        "adb"
    }
}

pub(crate) fn java_binary() -> &'static str {
    if cfg!(target_os = "windows") {
        "java.exe"
    } else {
        "java"
    }
}

// ─── Synchronous Command Runners ─────────────────────────────────────────────

/// Run an external command and return (stdout, stderr).
/// Returns Err if the process fails to start or exits with non-zero status.
pub(crate) fn run_cmd(program: &str, args: &[&str]) -> Result<(String, String), String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run '{}': {}", program, e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok((stdout, stderr))
    } else {
        Err(format!(
            "Command '{}' failed (exit {}):\n{}\n{}",
            program,
            output.status.code().unwrap_or(-1),
            stdout.trim(),
            stderr.trim()
        ))
    }
}

/// Same as run_cmd but doesn't fail on non-zero exit (some tools like aapt2
/// return non-zero but still produce useful output).
pub(crate) fn run_cmd_lenient(program: &str, args: &[&str]) -> Result<(String, String, bool), String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run '{}': {}", program, e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    Ok((stdout, stderr, output.status.success()))
}

// ─── Async Operation Helpers ─────────────────────────────────────────────────

/// Emit an operation-progress event to the frontend.
pub(crate) fn emit_op_progress(
    app: &tauri::AppHandle,
    operation: &str,
    device: &str,
    status: &str,
    message: &str,
    step: Option<u32>,
    total_steps: Option<u32>,
    cancellable: bool,
) {
    let _ = app.emit(
        "operation-progress",
        OperationProgress {
            operation: operation.to_string(),
            device: device.to_string(),
            status: status.to_string(),
            message: message.to_string(),
            step,
            total_steps,
            cancellable,
        },
    );
}

/// Async loop that resolves once the given cancel flag is set.
async fn poll_cancel_flag(cancel: &AtomicBool) {
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if cancel.load(Ordering::Relaxed) {
            break;
        }
    }
}

/// Run an external command asynchronously with an explicit cancellation flag.
/// Uses `tokio::process::Command` so the child process is killed on cancel
/// (via `kill_on_drop`).
pub(crate) async fn run_cmd_async_with_cancel(program: &str, args: &[&str], cancel: &AtomicBool) -> Result<(String, String), String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Operation cancelled by user.".to_string());
    }

    let child = tokio::process::Command::new(program)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Failed to run '{}': {}", program, e))?;

    tokio::select! {
        output = child.wait_with_output() => {
            let output = output.map_err(|e| format!("Process error for '{}': {}", program, e))?;
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            if output.status.success() {
                Ok((stdout, stderr))
            } else {
                Err(format!(
                    "Command '{}' failed (exit {}):\n{}\n{}",
                    program,
                    output.status.code().unwrap_or(-1),
                    stdout.trim(),
                    stderr.trim()
                ))
            }
        }
        _ = poll_cancel_flag(cancel) => {
            Err("Operation cancelled by user.".to_string())
        }
    }
}

/// Backward-compatible wrapper using the global cancel flag.
/// Used by wireless ADB commands that don't need per-operation cancellation.
pub(crate) async fn run_cmd_async(program: &str, args: &[&str]) -> Result<(String, String), String> {
    run_cmd_async_with_cancel(program, args, &OPERATION_CANCEL).await
}

/// Same as run_cmd_async_with_cancel but doesn't fail on non-zero exit.
/// Returns (stdout, stderr, success).
pub(crate) async fn run_cmd_async_lenient_with_cancel(program: &str, args: &[&str], cancel: &AtomicBool) -> Result<(String, String, bool), String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Operation cancelled by user.".to_string());
    }

    let child = tokio::process::Command::new(program)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Failed to run '{}': {}", program, e))?;

    tokio::select! {
        output = child.wait_with_output() => {
            let output = output.map_err(|e| format!("Process error for '{}': {}", program, e))?;
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Ok((stdout, stderr, output.status.success()))
        }
        _ = poll_cancel_flag(cancel) => {
            Err("Operation cancelled by user.".to_string())
        }
    }
}

/// Backward-compatible wrapper using the global cancel flag.
pub(crate) async fn run_cmd_async_lenient(program: &str, args: &[&str]) -> Result<(String, String, bool), String> {
    run_cmd_async_lenient_with_cancel(program, args, &OPERATION_CANCEL).await
}

// ─── Cancellation Control ────────────────────────────────────────────────────

/// Create a new per-operation cancellation token.
/// Returns a unique token string the frontend can use to cancel this specific operation.
#[tauri::command]
pub(crate) fn create_cancel_token() -> String {
    let id = CANCEL_TOKEN_COUNTER.fetch_add(1, Ordering::SeqCst);
    let token = format!("op-{}", id);
    if let Ok(mut reg) = cancel_registry().lock() {
        reg.insert(token.clone(), Arc::new(AtomicBool::new(false)));
    }
    token
}

/// Cancel a specific operation by its token.
#[tauri::command]
pub(crate) fn cancel_operation(token: String) {
    if let Ok(reg) = cancel_registry().lock() {
        if let Some(flag) = reg.get(&token) {
            flag.store(true, Ordering::SeqCst);
        }
    }
}

/// Release a cancellation token (cleanup after operation completes).
#[tauri::command]
pub(crate) fn release_cancel_token(token: String) {
    if let Ok(mut reg) = cancel_registry().lock() {
        reg.remove(&token);
    }
}

/// Set or clear the global cancellation flag for async operations.
/// Also cancels all active per-operation tokens when `cancel` is true (backward compat).
#[tauri::command]
pub(crate) fn set_cancel_flag(cancel: bool) {
    OPERATION_CANCEL.store(cancel, Ordering::SeqCst);
    if cancel {
        if let Ok(reg) = cancel_registry().lock() {
            for flag in reg.values() {
                flag.store(true, Ordering::SeqCst);
            }
        }
    }
}

/// Write text content to a file at the given path.
/// Used by the frontend to save logs and other exported text.
#[tauri::command]
pub(crate) fn save_text_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, &content)
        .map_err(|e| format!("Failed to write file '{}': {}", path, e))
}

/// Send a native OS notification.
/// On macOS uses `osascript` (AppleScript) with sound — the only method that
/// reliably shows banners. `notify-rust` delivers silently to Notification Center
/// without banners and doesn't work in dev mode at all.
/// On Linux/Windows uses `notify-rust`.
#[tauri::command]
pub(crate) fn send_notification(title: String, body: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let escaped_title = title.replace('\\', "\\\\").replace('"', "\\\"");
        let escaped_body = body.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!(
            r#"display notification "{}" with title "{}" sound name "default""#,
            escaped_body, escaped_title
        );
        std::process::Command::new("osascript")
            .args(["-e", &script])
            .output()
            .map_err(|e| format!("Notification failed: {}", e))?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        notify_rust::Notification::new()
            .summary(&title)
            .body(&body)
            .sound_name("default")
            .show()
            .map_err(|e| format!("Notification failed: {}", e))?;
        Ok(())
    }
}


// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adb_binary_returns_correct_name() {
        let name = adb_binary();
        if cfg!(target_os = "windows") {
            assert_eq!(name, "adb.exe");
        } else {
            assert_eq!(name, "adb");
        }
    }

    #[test]
    fn java_binary_returns_correct_name() {
        let name = java_binary();
        if cfg!(target_os = "windows") {
            assert_eq!(name, "java.exe");
        } else {
            assert_eq!(name, "java");
        }
    }

    #[test]
    fn run_cmd_echo_succeeds() {
        let result = run_cmd("echo", &["hello"]);
        assert!(result.is_ok());
        let (stdout, _) = result.unwrap();
        assert!(stdout.trim().contains("hello"));
    }

    #[test]
    fn run_cmd_nonexistent_program_fails() {
        let result = run_cmd("nonexistent_program_12345", &[]);
        assert!(result.is_err());
    }

    #[test]
    fn run_cmd_lenient_false_exit_returns_ok() {
        // `false` exits with code 1 on Unix
        if !cfg!(target_os = "windows") {
            let result = run_cmd_lenient("false", &[]);
            assert!(result.is_ok());
            let (_, _, success) = result.unwrap();
            assert!(!success);
        }
    }

    #[test]
    fn run_cmd_lenient_true_exit_returns_ok() {
        let result = run_cmd_lenient("true", &[]);
        assert!(result.is_ok());
        let (_, _, success) = result.unwrap();
        assert!(success);
    }

    #[test]
    fn create_cancel_token_returns_unique_ids() {
        let t1 = create_cancel_token();
        let t2 = create_cancel_token();
        assert_ne!(t1, t2);
        assert!(t1.starts_with("op-"));
        assert!(t2.starts_with("op-"));
        // Cleanup
        release_cancel_token(t1);
        release_cancel_token(t2);
    }

    #[test]
    fn cancel_operation_sets_flag_for_specific_token() {
        let t1 = create_cancel_token();
        let t2 = create_cancel_token();

        let flag1 = get_cancel_flag(&Some(t1.clone()));
        let flag2 = get_cancel_flag(&Some(t2.clone()));

        assert!(!flag1.load(Ordering::Relaxed));
        assert!(!flag2.load(Ordering::Relaxed));

        cancel_operation(t1.clone());

        assert!(flag1.load(Ordering::Relaxed));
        assert!(!flag2.load(Ordering::Relaxed)); // other token unaffected

        // Cleanup
        release_cancel_token(t1);
        release_cancel_token(t2);
    }

    #[test]
    fn release_cancel_token_removes_from_registry() {
        let token = create_cancel_token();
        let flag = get_cancel_flag(&Some(token.clone()));
        assert!(!flag.load(Ordering::Relaxed));

        release_cancel_token(token.clone());

        // After release, get_cancel_flag should return a new non-cancellable flag
        let flag2 = get_cancel_flag(&Some(token));
        assert!(!flag2.load(Ordering::Relaxed));
    }

    #[test]
    fn get_cancel_flag_returns_noncancellable_for_none() {
        let flag = get_cancel_flag(&None);
        assert!(!flag.load(Ordering::Relaxed));
    }

    #[test]
    fn set_cancel_flag_cancels_all_tokens() {
        let t1 = create_cancel_token();
        let t2 = create_cancel_token();

        let flag1 = get_cancel_flag(&Some(t1.clone()));
        let flag2 = get_cancel_flag(&Some(t2.clone()));

        set_cancel_flag(true);

        assert!(flag1.load(Ordering::Relaxed));
        assert!(flag2.load(Ordering::Relaxed));

        // Reset global
        set_cancel_flag(false);

        // Cleanup
        release_cancel_token(t1);
        release_cancel_token(t2);
    }
}
