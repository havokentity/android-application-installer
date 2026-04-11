# Android Application Installer — Feature & Improvement Analysis

A full audit of the app covering new features, code quality, UX, performance, and architecture.
Items marked `[x]` are **completed**, items marked `[ ]` are **pending**.

---

## 1. New Features

### 🔥 High Impact

- [x] **Wireless ADB (WiFi)** — `adb pair` / `adb connect` / `adb disconnect` support for Android 11+. Collapsible WiFi panel in DeviceSection with IP/port/pairing-code entry, auto-fill connect IP after pairing, disconnect button for wireless devices. Backed by `useWirelessAdb` hook. See [wireless-adb-guide.md](wireless-adb-guide.md).
- [ ] **Batch file install** — Select or drop multiple APK/AAB files. Install sequentially with per-file progress.
- [x] **APK/AAB metadata panel** — Show version name/code, min/target SDK, permissions, file size before installing. Metadata row displayed below file info. Backend: `get_apk_metadata` (binary manifest + aapt fallback) and `get_aab_metadata` (bundletool dump manifest). Auto-fetched on file selection.
- [ ] **Device info enrichment** — Show Android version, API level, free storage next to each device.
- [x] **Signing profile presets** — Save named keystore + password + alias configs so you don't re-enter credentials. Profiles stored in `signing_profiles.json` with passwords encrypted (AES-256-GCM). File-to-profile auto-association remembers which profile was used with each file and auto-restores on re-selection. UI in AabSettingsSection with dropdown, save, and delete controls. Backend: `get_signing_profiles`, `save_signing_profile`, `delete_signing_profile`, `get_profile_for_file`, `set_profile_for_file` commands.

### ⚡ Medium Impact, Quick Wins

- [x] **Uninstall confirmation dialog** — `ask()` confirmation before destructive uninstall.
- [x] **Native OS notifications** — Desktop notifications via `notify-rust` when install/extract operations complete. Uses app bundle ID for proper icon in release builds; falls back to Terminal in dev mode. Cross-platform (macOS, Linux, Windows).
- [x] **Log export to file** — "Save log" button in LogPanel using `save()` dialog.
- [x] **Log filtering/search** — Filter input with text search and per-level toggle buttons (info, success, warning, error) in the log panel. Shows filtered/total count when active.
- [x] **File size display** — Show the selected file's size (e.g. "42.3 MB") in FileSection.
- [x] **APK downgrade support** — "Downgrade" checkbox inline next to the package name field; passes `-d` to `adb install` for APKs and `--allow-downgrade` to `bundletool install-apks` for AABs.

### 🔮 Future Roadmap

- [ ] **Lightweight logcat viewer** — Stream `adb logcat` filtered by package name.
- [ ] **Device screenshot capture** — `adb exec-out screencap -p` to grab a PNG and save via dialog.

---

## 2. Code Quality Improvements

- [x] **Extract state from `App.tsx` into custom hooks** — Created `useUpdater`, `useToolsState`, `useDeviceState`, `useFileState`, `useAabSettings`. App.tsx reduced from ~830 to ~320 lines.
- [x] **Remove duplicate `formatBytes`** — Deleted from `Toolbar.tsx`, importing from `helpers.ts`.
- [x] **Tighten `canInstall` type** — Cast to boolean at the call site.
- [x] **Extract keystore args builder** — Shared `build_keystore_args()` helper in `adb.rs`; used by `install_aab` and `extract_apk_from_aab`.
- [x] **Replace swallowed errors** — All catch blocks now log via `console.warn` or `addLog`; no empty `.catch(() => {})` remaining.
- [x] **Add React Error Boundary** — Wrap root in `<ErrorBoundary>` component.
- [ ] **Concurrency-safe cancellation** — Per-operation cancellation tokens instead of global `AtomicBool`.

---

## 3. UX Improvements

- [x] **Persist manual ADB path** — Save user-entered ADB path to localStorage or config.
- [x] **"Install & Run" as default action** — Make it the primary green button.
- [x] **Better drag rejection** — Show drop zone in red for unsupported file types during drag-over.
- [x] **Keyboard shortcut for Extract APK** — Add e.g. `Cmd+E`.
- [x] **Empty state for devices** — Visual guide when no device is connected.
- [x] **Toast/snackbar notifications** — Brief auto-dismissing toasts for important events.

---

## 4. Performance Improvements

- [x] **Virtualize the log panel** — Cap visible entries at 200; earlier entries hidden with indicator. Full log retained for copy/export.
- [x] **Memoize pure components** — Wrapped `StatusDot`, `LogIcon`, `ToolRow`, `StaleBanner` in `React.memo()`.
- [x] **Use `adb track-devices`** — Push-based device tracking via `adb track-devices -l` with automatic fallback to 8-second polling.
- [x] **Debounce log auto-scroll** — Replaced direct `scrollIntoView` with `requestAnimationFrame`-guarded debounce.

---

## 5. Architecture Improvements

- [x] **Typed IPC layer** — Created `src/api.ts` wrapping all `invoke()` calls with typed functions. Zero string-based command names in App.tsx.
- [ ] **Auto-generated TypeScript types** — Use `ts-rs` or `tauri-specta` to generate TS interfaces from Rust structs.
- [ ] **State machine for operations** — Replace boolean flags with discriminated union state machine.
- [x] **Keystore password security** — Signing profile passwords encrypted at rest using AES-256-GCM with a machine-local key (`signing_key.bin`). Legacy plaintext profiles migrated transparently on first load.

---

## Summary

| Category | Done | Remaining |
|----------|------|-----------|
| New Features (1) | 9 | 4 |
| Code Quality (2) | 6 | 1 |
| UX (3) | 6 | 0 |
| Performance (4) | 4 | 0 |
| Architecture (5) | 2 | 2 |
| **Total** | **27** | **7** |
