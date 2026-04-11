//! Command execution infrastructure: sync/async process runners, cancellation,
//! and progress-event helpers shared across the application.

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
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

/// Global cancellation flag for long-running async operations.
pub(crate) static OPERATION_CANCEL: AtomicBool = AtomicBool::new(false);

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

/// Async loop that resolves once the cancel flag is set.
async fn poll_cancel() {
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if OPERATION_CANCEL.load(Ordering::Relaxed) {
            break;
        }
    }
}

/// Run an external command asynchronously with cancellation support.
/// Uses `tokio::process::Command` so the child process is killed on cancel
/// (via `kill_on_drop`).
pub(crate) async fn run_cmd_async(program: &str, args: &[&str]) -> Result<(String, String), String> {
    // Early exit if already cancelled
    if OPERATION_CANCEL.load(Ordering::Relaxed) {
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
        _ = poll_cancel() => {
            Err("Operation cancelled by user.".to_string())
        }
    }
}

/// Same as run_cmd_async but doesn't fail on non-zero exit.
/// Returns (stdout, stderr, success) — useful for tools like `adb pair`
/// that may exit non-zero but still produce useful output.
pub(crate) async fn run_cmd_async_lenient(program: &str, args: &[&str]) -> Result<(String, String, bool), String> {
    if OPERATION_CANCEL.load(Ordering::Relaxed) {
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
        _ = poll_cancel() => {
            Err("Operation cancelled by user.".to_string())
        }
    }
}

// ─── Cancellation Control ────────────────────────────────────────────────────

/// Set or clear the global cancellation flag for async operations.
/// Called from the frontend to cancel or reset before starting a new batch.
#[tauri::command]
pub(crate) fn set_cancel_flag(cancel: bool) {
    OPERATION_CANCEL.store(cancel, Ordering::SeqCst);
}

/// Write text content to a file at the given path.
/// Used by the frontend to save logs and other exported text.
#[tauri::command]
pub(crate) fn save_text_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, &content)
        .map_err(|e| format!("Failed to write file '{}': {}", path, e))
}

/// Send a native OS notification.
/// On macOS uses `osascript` (AppleScript) — the only reliable method since
/// `notify-rust` returns Ok but silently fails to display on modern macOS.
/// On Linux/Windows uses `notify-rust`.
#[tauri::command]
pub(crate) fn send_notification(title: String, body: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let escaped_title = title.replace('\\', "\\\\").replace('"', "\\\"");
        let escaped_body = body.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!(
            r#"display notification "{}" with title "{}""#,
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
}
