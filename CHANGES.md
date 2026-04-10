# Changelog

All notable changes to Android Application Installer are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/).

---

## [Unreleased]

### Added
- **macOS ad-hoc code signing** — builds are now ad-hoc signed by default, eliminating the "app is damaged" Gatekeeper error without needing an Apple Developer account
- **macOS notarization support** — CI workflow accepts optional Apple Developer ID secrets for full code signing and notarization (zero Gatekeeper warnings)
- macOS first-launch instructions in README, GitHub Release notes, and release script output

### Changed
- CI workflow defaults `APPLE_SIGNING_IDENTITY` to `-` (ad-hoc) when no signing secrets are configured
- Release script includes macOS Gatekeeper bypass note in generated GitHub Release body

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

