# Changelog

All notable changes to Android Application Installer are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/).

---

## [Unreleased]
### Fixed
- **ADB server persists after app exit** — the ADB daemon (`adb.exe` / `adb`) kept running as a ghost process after closing the app; added an exit handler that stops the device tracker and kills the managed ADB server on shutdown
- **Can't update ADB platform-tools on Windows** — the ADB server daemon held file locks on `adb.exe`, preventing `remove_dir_all` from deleting the old tools directory during updates; now kills the server before removal, with a retry loop (5 × 500 ms) for Windows file-lock release delay

### Changed
- App lifecycle uses `.build().run()` with `RunEvent::Exit` handler instead of bare `.run()` for graceful cleanup on exit
- Only the app's own managed ADB server is killed on exit — if the user is using a system ADB (from Android Studio or the Android SDK), the server is left running since another tool may depend on it
- `DeviceTracker` fields (`stop_flag`, `handle`) changed to `pub(crate)` for access from the exit handler
- `setup_platform_tools` now calls `adb kill-server` via the managed binary before removing the old `platform-tools/` directory

---

## [1.8.1] — 2026-04-11
### Fixed
- **Windows: blank console windows appearing** — ADB and Java commands spawned visible `cmd.exe` windows on Windows, stealing focus and freezing the main app window; added `CREATE_NO_WINDOW` creation flag to all sync and async command runners so processes run silently in the background
- **Windows: app freezing during ADB operations** — the visible console windows blocked the UI thread; closing them killed the ADB process, causing devices to appear disconnected; now all commands run windowless

### Changed
- Added `no_window_cmd()` and `no_window_async()` platform helpers in `cmd.rs` (`CREATE_NO_WINDOW` on Windows, no-op on macOS/Linux); applied to `run_cmd`, `run_cmd_lenient`, `run_cmd_async_with_cancel`, `run_cmd_async_lenient_with_cancel`, and both direct `tokio::process::Command` calls in the device tracker

### Removed
- Unused `run_cmd_async` function and its import — dead code superseded by `run_cmd_async_with_cancel`

---

## [1.8.0] — 2026-04-11
### Added
- **Batch file install** — select or drop multiple APK/AAB files; installs sequentially across all target devices with per-file progress prefixes (`[1/3]`, `[2/3]`, etc.); file dialog now supports multi-select; drag-drop accepts multiple files; batch file list displayed below the package section with numbered queue
- **Device info enrichment** — Android version, API level, and free storage shown in a details row below the device dropdown and inline in the device selector; fetched automatically via `adb shell getprop` and `df /data` when devices connect
- **Operation state machine** — replaced `isInstalling` / `isExtracting` / `operationProgress` boolean flags with a discriminated union `OperationState` type (`idle | installing | extracting`) that carries progress and cancel token in a single state value
- **Per-operation cancellation tokens** — each install/extract operation creates a unique cancellation token via `create_cancel_token`; cancel button targets the specific operation instead of setting a global flag; tokens are released after operation completes
- `getDeviceDetails` Tauri command with typed IPC wrapper in `api.ts`
- `createCancelToken`, `cancelOperation`, `releaseCancelToken` typed IPC wrappers in `api.ts`
- Cancel token parameter added to `installApk`, `installAab`, `extractApkFromAab`, `launchApp`, `stopApp`, `uninstallApp` API calls
- `selectedFiles` (batch) state in `useFileState` hook with `handleBatchFilesSelected` handler
- `deviceDetails` state in `useDeviceState` hook with auto-fetch on device connect
- CSS for `.batch-file-list`, `.device-details-row`, `.badge-blue` styles
- 11 new frontend tests (batch file display, device details row, OperationState type, DeviceDetails type) — total: 399

### Fixed
- **AAB metadata on first selection** — `onAabSelected` now returns `javaPath` and `bundletoolPath` directly, preventing stale React state from causing metadata retrieval to silently fail on the very first AAB file selection; `useFileState` prefers the returned paths and falls back to `getAabToolPaths` only when needed
- Refactored `checkJava` and `detectBundletool` in `useAabSettings` to return tool paths directly

### Changed
- File dialog now uses `multiple: true` for multi-file selection
- Drag-drop accepts multiple files and filters valid APK/AAB files
- Drop zone text updated: "Click or drop apk or aab file(s)"
- Window title shows file count when multiple files are selected
- Updated `docs/feature-analysis.md` — marked 4 items as completed (31/34 done)
- Updated `docs/architecture.md` — added OperationState, DeviceDetails, batch install, per-operation cancellation

---

## [1.7.4] — 2026-04-11
### Fixed
- **Notifications not popping up as banners** — added `sound name "default"` to macOS notifications so they appear as banners instead of silently going to Notification Center

---

## [1.7.3] — 2026-04-11
### Added
- **APK/AAB metadata panel** — version name/code, min/target SDK levels displayed in a metadata row below the file info after selection; auto-detected from binary manifest (APK) or bundletool dump (AAB) with aapt/aapt2 fallback
- **Signing profile presets** — save, load, and delete named keystore + password + alias configurations in AAB Settings; profiles persisted in `signing_profiles.json`; UI with dropdown selector, save input, and delete button
- **Signing profile encryption** — passwords encrypted at rest using AES-256-GCM with a machine-local key (`signing_key.bin`); legacy plaintext profiles migrated transparently on first load
- **File-profile auto-association** — signing profiles are remembered per-file; when a previously used APK/AAB is re-selected, the associated profile is auto-restored; association saved on successful install
- `get_apk_metadata` and `get_aab_metadata` Tauri commands with typed IPC wrappers in `api.ts`
- `get_signing_profiles`, `save_signing_profile`, `delete_signing_profile` Tauri commands for profile management
- `get_profile_for_file`, `set_profile_for_file` Tauri commands for file-to-profile mapping
- `tools/profiles.rs` — Rust module for signing profile persistence with upsert, delete, encryption, file mapping, and corruption-safe loading
- `aes-gcm` and `rand` crate dependencies for AES-256-GCM encryption
- 8 new metadata parsing Rust tests (aapt, XML attribute, permission parsing) — total: 107 Rust tests
- 21 new frontend tests (metadata display, signing profiles, downgrade checkbox, type validation) — total: 388

### Fixed
- **APK metadata extraction** — rewrote `extract_manifest_from_apk` to use structured DOM traversal instead of broken `format!("{:?}", doc)` + regex approach; now correctly reads `android:`-prefixed attributes and traverses child elements for `<uses-sdk>` and `<uses-permission>`
- **Native OS notifications broken on macOS** — `notify-rust` was silently failing; fixed by trying `notify-rust` with proper bundle ID first (shows app icon in release builds), with automatic `osascript` fallback if it fails (guaranteed delivery); dev mode shows Terminal icon (known macOS limitation)

### Changed
- Updated `docs/feature-analysis.md` — marked 3 items as completed (27/34 done)
- Updated `docs/architecture.md` — added `profiles.rs`, encryption key, file mappings, `get_apk_metadata`, `get_aab_metadata`, signing profile commands, and updated component descriptions

---

## [1.7.2] — 2026-04-11
### Added
- **APK/AAB downgrade support** — "Downgrade" checkbox inline next to the package name field; passes `-d` to `adb install` for APKs and `--allow-downgrade` to `bundletool install-apks` for AABs
- **Native OS notifications** — desktop notifications via `notify-rust` when install or extract operations complete; shows app icon in release builds, Terminal icon in dev mode; cross-platform (macOS, Linux, Windows)
- **Log filtering & search** — filter bar in the Log panel with text search input, per-level toggle buttons (info, success, warning, error), and filtered/total count display
- `send_notification` Tauri command with typed IPC wrapper in `api.ts`
- `notify-rust` direct dependency for reliable cross-platform notifications
- 5 new Rust unit tests for `build_keystore_args()` helper — total: 89 Rust tests

### Changed
- **Refactored keystore arg construction** — `install_aab` and `extract_apk_from_aab` now use the shared `build_keystore_args()` helper, eliminating duplicated keystore argument building
- **Replaced swallowed errors** — all empty `.catch(() => {})` blocks across hooks and components now log via `console.warn`
- **Package name field layout** — input fills available width with downgrade checkbox placed inline to the right, saving a line
- Updated `docs/feature-analysis.md` — marked 5 items as completed (24/34 done)

---

## [1.7.1] — 2026-04-11
### Added
- **File size display** — selected file's size (e.g. "42.3 MB") shown in the Package section next to the file type badge
- **Log export to file** — "Save" button in the Log panel exports the full log to a `.log` or `.txt` file via a save dialog
- **React Error Boundary** — wraps the entire app in an `<ErrorBoundary>` that catches render errors and shows a styled recovery screen with a "Reload" button instead of a white screen
- `get_file_size` and `save_text_file` Tauri commands with typed IPC wrappers in `api.ts`
- 6 new frontend tests (file size display, log save button) — total: 367

### Changed
- **Tightened `canInstall` type** — `canInstall` is now a clean `boolean` instead of `string | false | boolean`; used `!!()` cast at the call site
- Updated `docs/feature-analysis.md` — marked 4 items as completed (19/34 done)
- Updated `docs/architecture.md` — added `ErrorBoundary.tsx`, `get_file_size`, `save_text_file` commands, and updated component descriptions

---

## [1.7.0] — 2026-04-11
### Added
- **Wireless ADB (WiFi)** — pair, connect, and disconnect Android 11+ devices over WiFi without a USB cable; collapsible WiFi panel in the Device section with IP/port/pairing-code fields
- **mDNS network discovery** — scan the local network for Android devices with Wireless Debugging enabled; auto-fill IP and port from discovered services; requires ADB platform-tools 31+
- **Install mode toggle** — choose between *Direct* (IP:port, bypasses Play Protect) and *Verified* (mDNS, goes through Play Protect) install modes for wireless devices
- **Device deduplication** — when the same physical device appears via both IP:port and mDNS transports, a single entry is shown with the preferred transport selected automatically
- **Pairing prompt on connect failure** — if a wireless connect fails, the app suggests pairing first and pre-fills the IP address into the pairing fields
- **Wireless disconnect button** — one-click disconnect for wireless devices, with alternate transport cleanup
- **`useWirelessAdb` hook** — full state management for wireless ADB: pair, connect, disconnect, scan, validation, cancellation, and mDNS enrichment
- **Wireless ADB guide** — new `docs/wireless-adb-guide.md` covering prerequisites, quick start, troubleshooting, and architecture
- Unit tests for wireless ADB: 80 tests in `useWirelessAdb.test.ts`, 24 WiFi-specific tests in `DeviceSection.test.tsx`, 18 Rust parser tests in `adb.rs` (total: 84 Rust / 361 frontend)
- `run_cmd_async_lenient` async command runner for tools that exit non-zero but produce useful output

### Fixed
- **mDNS scan after disconnect** — after disconnecting a wireless device, scanning now automatically restarts the ADB server (when no devices are connected) to clear the stale mDNS cache, so devices reappear without needing to toggle WiFi debugging on the phone

### Changed
- Device tracking now handles wireless mDNS serials alongside traditional USB/IP serials
- Added `adb_pair`, `adb_connect`, `adb_disconnect`, `adb_mdns_check`, `adb_mdns_services` Tauri commands with typed IPC wrappers in `api.ts`
- Updated `DeviceSection.tsx` with WiFi panel, discovery list, install mode pills, and grouped mDNS service display
- Updated `docs/feature-analysis.md` — marked Wireless ADB as completed

---

## [1.6.8] — 2026-04-11
### Added
- **Push-based device tracking** — replaced 8-second polling with `adb track-devices -l` for instant device connect/disconnect detection; automatically falls back to polling if tracking fails
- **Log panel virtualization** — only the most recent 200 log entries are rendered; earlier entries are retained for copy/export with a "N earlier entries hidden" indicator

### Changed
- **Memoized pure components** — wrapped `StatusDot`, `LogIcon`, `ToolRow`, and `StaleBanner` in `React.memo()` to prevent unnecessary re-renders
- **Debounced log auto-scroll** — replaced direct `scrollIntoView` with `requestAnimationFrame`-guarded debounce to avoid layout thrashing
- Added `start_device_tracking` and `stop_device_tracking` Tauri commands with managed `DeviceTracker` state
- Added `parse_device_list` shared parser with 4 new Rust unit tests (total: 66 Rust tests)
- Updated `docs/feature-analysis.md` — performance category now 4/4 complete

---

## [1.6.7] — 2026-04-10
### Added
- **Device empty state styling** — visual guide (USB icon, numbered setup steps, refresh button) when no device is connected now properly styled
- **Drag rejection styling** — drop zone turns red with red icon when unsupported file types are dragged over

### Changed
- Updated README with toast notifications, full keyboard shortcut list (`Cmd+K` stop, `Cmd+E` extract), and complete project structure listing all 8 hooks, `api.ts`, and `Toast.tsx`
- Updated architecture docs — source layout, frontend state management rewritten for hook-based architecture, auto-updater components table updated

---

## [1.6.6] — 2026-04-10
### Added
- **Toast notification system** — auto-dismissing toasts for key events (install, launch, stop, uninstall, ADB detection, APK extraction, cancellation) with four levels (success, error, warning, info), slide-in/out animations, and manual dismiss
- **`Toast.tsx` component** — `useToast` hook and `ToastContainer` component with configurable duration, max 5 visible toasts, and exit animations
- **Drag rejection styling** — drop zone turns red with red icon when an unsupported file type is dragged over
- Unit tests for `useToast` hook and `ToastContainer` component (15 new tests)

### Changed
- Updated README with toast notifications feature, full keyboard shortcuts (`Cmd+K` stop, `Cmd+E` extract), and complete project structure (all 8 hooks, `api.ts`, `Toast.tsx`)
- Updated `docs/architecture.md` — source layout, frontend state management section rewritten for hook-based architecture, auto-updater components table references `useUpdater.ts`
- Updated `docs/feature-analysis.md` — marked 4 UX features as completed (persist ADB path, drag rejection, extract shortcut, toasts)

### Fixed
- `FileSection` tests — added missing `isDragRejected` prop default, switched to regex matching for Extract APK button title (now includes shortcut label)
- Release script — prevent duplicate version sections in CHANGES.md when `[Unreleased]` is promoted

---


## [1.6.5] — 2026-04-11
### Added
- **Uninstall confirmation dialog** — warns before removing an app and all its data from the device

### Changed
- **Typed IPC layer** — all Tauri `invoke()` calls now go through `src/api.ts` with fully typed functions; zero string-based command names
- **State extraction** — extracted `useUpdater`, `useToolsState`, `useDeviceState`, `useFileState`, `useAabSettings` hooks; App.tsx reduced from ~830 to ~320 lines
- **Removed duplicate `formatBytes`** — Toolbar now imports the shared helper from `helpers.ts`
- Added `docs/feature-analysis.md` tracking planned features and improvements

---

## [1.6.4] — 2026-04-11
### Added
- **Auto-check on startup toggle** — bell icon in the toolbar lets you enable or disable automatic update checks on launch; preference is saved across sessions

### Changed
- **Update download progress bar** — replaced per-chunk log spam with a clean inline progress bar showing download percentage, downloaded/total bytes, and a smooth animated fill; appears below the toolbar during updates

---

## [1.6.3] — 2026-04-11
### Changed
- **Update download progress bar** — replaced per-chunk log spam with a clean inline progress bar showing download percentage, downloaded/total bytes, and a smooth animated fill; appears below the toolbar during updates

---


## [1.6.2] — 2026-04-11
### Added
- **Check for Updates button** — manually trigger an update check from the toolbar; shows a spinner while checking and reports "You're on the latest version" when no update is found

---

## [1.6.1] — 2026-04-10
### Added
- **Auto-updater** — the app now checks for updates on launch and prompts to download, install, and relaunch automatically; uses Tauri's signed update mechanism with Ed25519 signature verification
- CI workflow now produces signed update artifacts (`.sig` files) and auto-generates `updater.json` after each release

---

## [1.6.0] — 2026-04-10
### Added
- **Extract APK from AAB** — convert `.aab` files to universal `.apk` without a connected device; uses bundletool `--mode=universal` and extracts the APK from the resulting archive
- "Extract APK" button appears in the Package section when an AAB file is selected

---

## [1.5.3] — 2026-04-10
### Added
- Sync native window theme with app theme to match macOS title bar

### Changed
- Enhance Tauri build process to open built bundle automatically after build completion

---

## [1.5.2] — 2026-04-10
### Added
- **macOS ad-hoc code signing** — builds are now ad-hoc signed by default, eliminating the "app is damaged" Gatekeeper error without needing an Apple Developer account
- **macOS notarization support** — CI workflow accepts optional Apple Developer ID secrets for full code signing and notarization (zero Gatekeeper warnings)
- macOS first-launch instructions in README, GitHub Release notes, and release script output

### Changed
- CI workflow defaults `APPLE_SIGNING_IDENTITY` to `-` (ad-hoc) when no signing secrets are configured
- Release script includes macOS Gatekeeper bypass note in generated GitHub Release body

### Fixed
- macOS CI build failure caused by Tauri attempting to import an empty `APPLE_CERTIFICATE` env var into the keychain

---

## [1.5.1] — 2026-04-10

### Added
- **Update Changelog** script (`npm run changelog`) — auto-generate CHANGES.md entries from git history with commit categorization
- **Publish Release** script (`npm run release:publish`) to publish draft GitHub releases
- Auto-generated release notes from git log when CHANGES.md has no entry for the version
- Shared commit categorization library (`scripts/lib/categorize-commits.mjs`) used by both release and changelog scripts
- IntelliJ run configurations for Update Changelog and Publish Release

### Changed
- Refactored release script to use shared commit categorization library
- Release script now reads from `[Unreleased]` section and auto-promotes it to the versioned heading

---

## [1.5.0] — 2026-04-10

### Added
- **Stop Application** button — force-stop a running app on the device (`adb shell am force-stop`)
- Keyboard shortcut `Cmd/Ctrl+K` for stopping an app
- `.btn-warning` yellow CSS style for the Stop button

### Changed
- Refactored Rust backend from 2 monolithic files (~2400 lines) into 9 focused modules (`cmd.rs`, `adb.rs`, `package.rs`, `java.rs`, `tools/{config, download, paths, status, recent}.rs`)
- `lib.rs` reduced to a thin 35-line entry point

---

## [1.4.2] — 2026-04-10

### Changed
- Added run configurations for tests (Vitest, Cargo, combined) and improved UI test assertions
- Enhanced README and UI for consistency and clarity
- Refined README headers and layout for consistency
- Standardized README section headers and badge alignment

---

## [1.4.1] — 2026-04-10

### Changed
- Adjusted `tsconfig.json` and dependencies

---

## [1.4.0] — 2026-04-10
### Added
- Modular component structure — split App.tsx into dedicated components (AppHeader, FileSection, DeviceSection, AabSettingsSection, ToolsSection, LogPanel, Toolbar, StatusIndicators, EasterEggOverlay)
- Custom React hooks — useLayout, useKeyboardShortcuts, useEasterEgg extracted from App.tsx
- Comprehensive unit tests for all components, hooks, helper functions, and types (vitest + React Testing Library)
- Operation progress tracking and cancellation support for install, launch, and uninstall
- Run configurations for tests and release script test gate
- Themed SVG illustrations for project structure in README

### Changed
- Refactored App layout to use new modular component architecture
- Updated UI screenshots (dark/light, landscape/portrait)
- Improved version management scripts and documentation

---

## [1.3.2] — 2026-04-10

### Added
- **Landscape layout** — toggle between a wide two-panel layout and a compact vertical layout
- **Theme switching** — dark and light themes with one-click toggle; preference saved across sessions
- **Resizable side panel** — draggable divider in landscape mode; width is remembered
- **Compact mode** — attention indicators on ToolsSection when tools need action
- **Collapsible sections** — Device, Tools, and AAB Settings collapse when not needed
- **Smart auto-collapse** — Device section collapses when a device is connected; Tools section collapses when all tools are installed
- **Collapsible Device section** with attention indicators for missing devices
- **Inline action buttons** — Install, Launch, and Uninstall buttons on the Device section header, always accessible
- **Enhanced LogPanel** — improved device handling and new features
- **Keyboard shortcuts** — `Cmd/Ctrl+O` open file, `Cmd/Ctrl+I` install, `Cmd/Ctrl+Shift+I` install & run, `Cmd/Ctrl+L` launch, `Cmd/Ctrl+U` uninstall
- Version management script (`bump-version.mjs`) to sync version across all config files
- Automated release script (`release.mjs`) — bump, commit, tag, and push in one command

### Fixed
- Correct layout defaults, window sizing, and rendering order
- Skip commit in release script if version files are already up to date

### Changed
- Restructured Device section header and action buttons
- Updated app icons and enhanced README with screenshots

---

## [1.0.1] — 2026-04-10

### Added
- **Portable EXE** for Windows — standalone executable, no installation needed
- Portable EXE uploaded to GitHub Release alongside `.msi` and `-setup.exe`

### Changed
- Upgraded GitHub Actions to Node 24 native versions (actions/checkout v6, actions/setup-node v6, actions/upload-artifact v7)
- Removed `FORCE_JAVASCRIPT_ACTIONS_TO_NODE24` workaround

---

## [1.0.0] — 2026-04-10

### Added
- **APK installation** — install `.apk` files directly via ADB
- **AAB installation** — install `.aab` files via bundletool with optional keystore signing
- **Zero-dependency tool setup** — downloads ADB, bundletool, and Java JRE on demand (no Android SDK or system Java needed)
- **Device auto-detection** — automatic discovery and selection of connected Android devices
- **Package name extraction** — auto-detect package names from APK and AAB files
- **Key alias management** — read and select key aliases from keystore files
- **Recent files** — quickly re-select recently used APK/AAB files and keystores
- **Update reminders** — 30-day stale-tool notifications (never auto-downloads without consent)
- **Visual status indicators** — flashing red borders when tools are missing or no device is connected
- **Drag & drop** — drag APK or AAB files from Finder / Explorer directly into the app
- Dark theme UI with React 19 + TypeScript frontend
- CI workflow for macOS (ARM64 + x64), Windows, and Linux builds
- Architecture documentation

### Fixed
- macOS select dropdown positioning (WebKit native appearance fix)
- CI: replaced retired `macos-13` runner with `macos-latest`
- CI: bumped Node.js from 20 to 22

