// ─── Utility Helpers ──────────────────────────────────────────────────────────

/** Global auto-incrementing log entry ID. */
let logId = 0;
export function nextLogId(): number {
  return ++logId;
}

/** Extract the file name from a full path. */
export function getFileName(path: string): string {
  return path.split(/[/\\]/).pop() || path;
}

/** Determine Android package type from a file path. */
export function getFileType(path: string): "apk" | "aab" | null {
  const lower = path.toLowerCase();
  if (lower.endsWith(".apk")) return "apk";
  if (lower.endsWith(".aab")) return "aab";
  return null;
}

/** Current time formatted as HH:MM:SS. */
export function now(): string {
  return new Date().toLocaleTimeString("en-US", { hour12: false });
}

/** Human-readable byte size. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** Whether the user is on macOS. */
export const isMac = navigator.platform.toUpperCase().includes("MAC") ||
  navigator.userAgent.toUpperCase().includes("MAC");

/** Format a keyboard shortcut label, e.g. shortcutLabel("O") → "⌘O" on Mac, "Ctrl+O" elsewhere. */
export function shortcutLabel(key: string, shift = false): string {
  const mod = isMac ? "⌘" : "Ctrl+";
  const shiftMod = shift ? (isMac ? "⇧" : "Shift+") : "";
  return isMac ? `${mod}${shiftMod}${key.toUpperCase()}` : `${mod}${shiftMod}${key.toUpperCase()}`;
}

