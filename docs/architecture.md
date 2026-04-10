# Architecture Notes

Internal developer reference for the **Android Application Installer** project.

---

## Overview

A Tauri 2 desktop app that installs `.apk` and `.aab` files onto connected Android devices. The entire Android SDK toolchain (ADB, bundletool, Java JRE) is downloaded and managed locally by the app — nothing is installed system-wide.

## Tech Stack

| Layer     | Technology                     |
|-----------|--------------------------------|
| Framework | Tauri 2                        |
| Backend   | Rust                           |
| Frontend  | React 19 + TypeScript + Vite   |
| UI Icons  | Lucide React                   |
| HTTP      | reqwest (Rust, async streaming) |
| Archive   | `zip` crate (Windows), `tar` CLI (macOS/Linux) |
| Dialogs   | tauri-plugin-dialog            |

## Source Layout

```
src/                          ← React frontend
├── App.tsx                   Main component — all state + UI
├── App.css                   Dark/light theme styles, layouts
├── types.ts                  Shared TS interfaces (mirrors Rust structs)
├── helpers.ts                Pure utility functions (IDs, formatting)
├── main.tsx                  React entry point
├── components/
│   ├── AppHeader.tsx         Header with title & version
│   ├── FileSection.tsx       File selection & AAB extraction
│   ├── DeviceSection.tsx     Device selection & actions
│   ├── AabSettingsSection.tsx  AAB signing settings
│   ├── ToolsSection.tsx      Tools setup & stale banner
│   ├── LogPanel.tsx          Activity log with auto-scroll + copy
│   ├── Toolbar.tsx           Layout & theme toggles
│   ├── EasterEggOverlay.tsx  Easter egg overlay
│   └── StatusIndicators.tsx  StatusDot + LogIcon components
├── hooks/
│   ├── useLayout.ts          Layout state & persistence
│   ├── useKeyboardShortcuts.ts  Keyboard shortcuts
│   └── useEasterEgg.ts       Easter egg hook
└── __tests__/                Unit tests (vitest)

src-tauri/src/                ← Rust backend
├── main.rs                   Entry point (calls lib::run)
├── lib.rs                    Tauri command registry (thin entry point)
├── adb.rs                    ADB device ops: install APK/AAB, extract APK,
│                             launch, uninstall, stop, list packages
├── cmd.rs                    Command execution utilities, cancellation
├── java.rs                   Java & bundletool detection, key alias listing
├── package.rs                Package name extraction from APK & AAB
└── tools/                    Managed tool downloads
    ├── mod.rs                Module root & shared helpers
    ├── config.rs             Tool config persistence (tools_config.json)
    ├── download.rs           Download & extract logic (ADB, bundletool, Java)
    ├── paths.rs              Platform-specific tool paths
    ├── recent.rs             Recent files tracking
    └── status.rs             Tool status & staleness checks

scripts/                      ← Developer tooling
├── bump-version.mjs          Sync version across package.json, tauri.conf.json, Cargo.toml
├── release.mjs               Bump + commit + tag + push (triggers CI release)
├── publish-release.mjs       Publish draft GitHub releases
├── update-changelog.mjs      Auto-generate CHANGES.md entries from git log
└── lib/
    └── categorize-commits.mjs  Shared commit categorization logic
```

## Key Design Decisions

### 1. Managed Tools (no SDK required)

Users shouldn't need the Android SDK. The app downloads tools into its `app_local_data_dir`:

| Tool             | Source                             | Location in data dir       |
|------------------|------------------------------------|----------------------------|
| ADB              | Google Platform-Tools ZIP          | `platform-tools/adb`       |
| bundletool       | GitHub Releases (latest)           | `bundletool.jar`           |
| Java JRE 21      | Eclipse Temurin / Adoptium         | `jre/<jdk-dir>/…/bin/java` |

Detection priority: **managed → env vars → common paths → PATH lookup**.

### 2. Staleness Tracking

`tools_config.json` records Unix timestamps for each tool's last download. After 30 days, a non-intrusive banner suggests re-downloading. No silent auto-updates.

### 3. AAB Installation Flow

```
.aab → bundletool build-apks → .apks (temp) → bundletool install-apks → device
```

- Requires Java + bundletool
- Optional custom keystore (`.jks` / `.keystore`) for signed builds
- Temp `.apks` file cleaned up after install

### 3a. AAB → APK Extraction

```
.aab → bundletool build-apks --mode=universal → .apks (temp ZIP) → extract universal.apk → output .apk
```

- Produces a **universal APK** (single APK containing all ABIs, densities, and locales)
- No device required — works offline
- Uses the same keystore settings as AAB installation (optional)
- Temp `.apks` ZIP cleaned up after extraction
- User picks the output location via a save dialog

### 4. Frontend State Management

All state lives in `App.tsx` via `useState` hooks — no external state library. State groups:

- **Tools**: download status, progress, staleness
- **ADB**: path, detection status
- **Device**: device list, selected device
- **File**: selected file path, type (apk/aab), package name
- **AAB settings**: Java/bundletool paths, keystore config
- **General**: installing flag, log entries

### 5. Progress Events

Downloads emit `download-progress` Tauri events (tool name, bytes, percentage, status). The frontend listens via `@tauri-apps/api/event` and routes to the correct progress state by `tool` field.

### 6. Cross-Platform Handling

- **ADB binary name**: `adb` vs `adb.exe` (via `adb_binary()`)
- **Java binary name**: `java` vs `java.exe` (via `java_binary()`)
- **Archive format**: `.tar.gz` on macOS/Linux (extracted via `tar` CLI), `.zip` on Windows (via `zip` crate)
- **JRE layout**: macOS uses `Contents/Home/bin/java`, Linux/Windows use `bin/java`
- **Adoptium URL**: per-platform + per-arch variants (x64, aarch64)
- **SDK paths**: `~/Library/Android/sdk` (macOS), `~/Android/Sdk` (Linux), `%LOCALAPPDATA%\Android\Sdk` (Windows)

## Tauri Commands (IPC)

| Command                | File        | Purpose                                        |
|------------------------|-------------|-------------------------------------------------|
| `find_adb`             | adb.rs      | Auto-detect ADB binary                          |
| `get_devices`          | adb.rs      | List connected devices via `adb devices -l`     |
| `install_apk`          | adb.rs      | `adb install -r <apk>`                          |
| `install_aab`          | adb.rs      | bundletool build-apks + install-apks            |
| `extract_apk_from_aab` | adb.rs      | Extract universal APK from AAB via bundletool   |
| `launch_app`           | adb.rs      | `adb shell monkey -p <pkg> 1`                   |
| `uninstall_app`        | adb.rs      | `adb uninstall <pkg>`                           |
| `stop_app`             | adb.rs      | `adb shell am force-stop <pkg>`                 |
| `list_packages`        | adb.rs      | `adb shell pm list packages -3`                 |
| `get_package_name`     | package.rs  | Extract package name from APK (binary XML parser) |
| `get_aab_package_name` | package.rs  | Extract package name from AAB via bundletool     |
| `check_java`           | java.rs     | Detect Java path + version                      |
| `find_bundletool`      | java.rs     | Locate bundletool.jar                           |
| `list_key_aliases`     | java.rs     | List key aliases from a keystore via keytool    |
| `set_cancel_flag`      | cmd.rs      | Set/clear cancellation flag for async ops       |
| `get_tools_status`     | tools/status.rs  | Check which managed tools are installed     |
| `setup_platform_tools` | tools/download.rs | Download + extract ADB platform-tools      |
| `setup_bundletool`     | tools/download.rs | Download latest bundletool from GitHub      |
| `setup_java`           | tools/download.rs | Download + extract Temurin JRE 21          |
| `check_for_stale_tools`| tools/status.rs  | Return tools not updated in 30+ days        |
| `get_recent_files`     | tools/recent.rs  | Load recent packages & keystores            |
| `add_recent_file`      | tools/recent.rs  | Add a file to recent list                   |
| `remove_recent_file`   | tools/recent.rs  | Remove a file from recent list              |

## CI / CD

GitHub Actions workflow (`.github/workflows/build.yml`) builds for:
- macOS ARM64 (Apple Silicon)
- macOS x64 (Intel)
- Windows x64
- Linux x64

Triggered by version tags (`v*`) or manual dispatch.

**Tag push** → builds all platforms + creates a GitHub Release draft with artifacts.
**Manual dispatch** → builds all platforms, artifacts downloadable from the Actions tab (no release created).

## Version Management

The app version lives in **five files** that must stay in sync:

| File | Field | Used by |
|------|-------|---------|
| `package.json` | `"version"` | npm / frontend tooling |
| `package-lock.json` | `"version"` (×2) | npm lockfile |
| `src-tauri/tauri.conf.json` | `"version"` | Tauri binary, app metadata, `getVersion()` API |
| `src-tauri/Cargo.toml` | `version` (in `[package]`) | Rust crate metadata |
| `src-tauri/Cargo.lock` | `version` (app entry) | Rust lockfile |

All five are updated automatically by the bump and release scripts.

### Checking versions

```bash
npm run version
# Shows all three files, flags any drift
```

### Bumping versions

```bash
npm run version -- 1.2.0         # set explicit version
npm run version:patch             # 1.1.2 → 1.1.3
npm run version:minor             # 1.1.2 → 1.2.0
npm run version:major             # 1.1.2 → 2.0.0
```

The bump script (`scripts/bump-version.mjs`):
- Updates all three files atomically
- Finds the **highest** current version across all files and bumps from there
- **Refuses downgrades** — you can't set a version lower than the current highest
- Does NOT commit or tag — use the release script for that

### Releasing

The release script (`scripts/release.mjs`) automates the full release flow:

```bash
npm run release -- 1.2.0         # set explicit version + release
npm run release:patch             # bump patch + release
npm run release:minor             # bump minor + release
npm run release:major             # bump major + release
```

What it does:
1. Checks for a clean working tree (refuses to release with uncommitted changes)
2. Runs `bump-version.mjs` to update all version files
3. Commits: `release: v1.2.0`
4. Creates tag: `v1.2.0`
5. Pushes the commit + tag to `origin`
6. The tag push triggers the GitHub Actions workflow automatically

After running, the CI will:
- Build for macOS (ARM64 + x64), Windows (x64), and Linux (x64)
- Create a GitHub Release **draft** with all platform artifacts attached
- You just need to review and publish the draft on GitHub

### Manual release (without the script)

```bash
# 1. Bump version
npm run version -- 1.2.0

# 2. Commit + tag
git add -A
git commit -m "release: v1.2.0"
git tag v1.2.0

# 3. Push
git push origin main
git push origin v1.2.0
```

## Developer Scripts

All scripts live in `scripts/` and are exposed via npm:

| npm command | Script | Purpose |
|---|---|---|
| `npm run version` | `bump-version.mjs` | Show current versions, check sync |
| `npm run version -- X.Y.Z` | `bump-version.mjs` | Set explicit version across all files |
| `npm run version:patch` | `bump-version.mjs patch` | Bump patch version |
| `npm run version:minor` | `bump-version.mjs minor` | Bump minor version |
| `npm run version:major` | `bump-version.mjs major` | Bump major version |
| `npm run release -- X.Y.Z` | `release.mjs` | Bump + commit + tag + push (triggers CI) |
| `npm run release:patch` | `release.mjs patch` | Patch release (bump + push) |
| `npm run release:minor` | `release.mjs minor` | Minor release (bump + push) |
| `npm run release:major` | `release.mjs major` | Major release (bump + push) |

## Data Directory

All managed tools + config live under the Tauri `app_local_data_dir`:

```
<app-data>/
├── platform-tools/       ← extracted Google platform-tools
│   ├── adb
│   └── ...
├── bundletool.jar        ← latest from GitHub
├── jre/                  ← extracted Adoptium JRE
│   └── jdk-21.x.y-jre/
│       └── Contents/Home/bin/java   (macOS)
└── tools_config.json     ← last-updated timestamps
```

