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
├── App.css                   Dark-theme styles
├── types.ts                  Shared TS interfaces (mirrors Rust structs)
├── helpers.ts                Pure utility functions (IDs, formatting)
└── components/
    ├── LogPanel.tsx           Activity log with auto-scroll
    ├── StatusIndicators.tsx   StatusDot + LogIcon components
    └── ToolsSection.tsx       Tools download section + stale-tools banner

src-tauri/src/                ← Rust backend
├── main.rs                   Entry point (calls lib::run)
├── lib.rs                    Tauri commands: ADB detection, device listing,
│                             APK/AAB install, launch, uninstall, package name
└── tools.rs                  Managed tool downloads (platform-tools, bundletool,
                              Java JRE) + staleness tracking via tools_config.json
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

| Command                | File      | Purpose                                        |
|------------------------|-----------|-------------------------------------------------|
| `find_adb`             | lib.rs    | Auto-detect ADB binary                          |
| `get_devices`          | lib.rs    | List connected devices via `adb devices -l`     |
| `install_apk`          | lib.rs    | `adb install -r <apk>`                          |
| `install_aab`          | lib.rs    | bundletool build-apks + install-apks            |
| `launch_app`           | lib.rs    | `adb shell monkey -p <pkg> 1`                   |
| `get_package_name`     | lib.rs    | Extract package name via aapt2/aapt             |
| `check_java`           | lib.rs    | Detect Java path + version                      |
| `find_bundletool`      | lib.rs    | Locate bundletool.jar                           |
| `uninstall_app`        | lib.rs    | `adb uninstall <pkg>`                           |
| `list_packages`        | lib.rs    | `adb shell pm list packages -3`                 |
| `get_tools_status`     | tools.rs  | Check which managed tools are installed          |
| `setup_platform_tools` | tools.rs  | Download + extract ADB platform-tools           |
| `setup_bundletool`     | tools.rs  | Download latest bundletool from GitHub           |
| `setup_java`           | tools.rs  | Download + extract Temurin JRE 21               |
| `check_for_stale_tools`| tools.rs  | Return tools not updated in 30+ days            |

## CI / CD

GitHub Actions workflow (`.github/workflows/build.yml`) builds for:
- macOS ARM64 (Apple Silicon)
- macOS x64 (Intel)
- Windows x64

Triggered by version tags (`v*`) or manual dispatch.

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

