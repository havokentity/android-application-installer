# Android Application Installer

A cross-platform desktop application for installing **APK** and **AAB** files onto connected Android devices — no Android SDK required.

Built with [Tauri 2](https://tauri.app/) (Rust backend) and React + TypeScript (frontend).

---

## Features

- **One-click tool setup** — downloads ADB, bundletool, and a Java JRE automatically (no Android SDK or system Java needed)
- **APK installation** — install `.apk` files directly via ADB
- **AAB installation** — install `.aab` files via bundletool (build-apks → install-apks), with optional keystore signing
- **Device management** — auto-detect connected USB devices, refresh, and select target device
- **Package management** — launch or uninstall apps by package name
- **Automatic package name detection** — extracts the package name from APK files using `aapt2`
- **Update reminders** — notifies you when managed tools haven't been updated in 30+ days (no auto-downloads without consent)
- **Cross-platform** — works on macOS, Windows, and Linux

## Screenshots

<!-- Add screenshots here -->

## Getting Started

### Prerequisites

- **Node.js** ≥ 18
- **Rust** (stable toolchain) — install via [rustup](https://rustup.rs/)
- An Android device with **USB debugging** enabled

### Install dependencies

```bash
npm install
```

### Run in development mode

```bash
npm run tauri dev
```

### Build for production

```bash
npm run tauri build
```

Build artifacts will be in `src-tauri/target/release/bundle/`:
- **macOS**: `.app` bundle and `.dmg` installer
- **Windows**: `.exe` and `.msi` installer
- **Linux**: `.deb` and `.AppImage`

## Project Structure

```
├── src/                          # React frontend
│   ├── App.tsx                   # Main application component
│   ├── App.css                   # Styles (dark theme)
│   ├── types.ts                  # Shared TypeScript interfaces
│   ├── helpers.ts                # Utility functions
│   ├── main.tsx                  # React entry point
│   └── components/
│       ├── LogPanel.tsx          # Activity log panel
│       ├── StatusIndicators.tsx  # StatusDot & LogIcon components
│       └── ToolsSection.tsx      # Tools setup section + stale banner
│
├── src-tauri/                    # Rust backend (Tauri)
│   ├── src/
│   │   ├── main.rs               # App entry point
│   │   ├── lib.rs                 # Tauri commands (ADB, install, launch, etc.)
│   │   └── tools.rs               # Managed tool downloads (ADB, bundletool, Java JRE)
│   ├── Cargo.toml                 # Rust dependencies
│   ├── tauri.conf.json            # Tauri app configuration
│   └── capabilities/
│       └── default.json           # Tauri permissions
│
├── .github/workflows/
│   └── build.yml                  # CI: build for macOS & Windows
│
├── index.html                     # HTML entry point
├── vite.config.ts                 # Vite configuration
├── tsconfig.json                  # TypeScript configuration
└── package.json                   # npm scripts & dependencies
```

## How It Works

### Managed Tools

On first launch, the app has no external dependencies. When you click the download buttons:

| Tool | Source | Purpose |
|------|--------|---------|
| **ADB** | [Google Platform-Tools](https://developer.android.com/tools/releases/platform-tools) | Communicate with Android devices |
| **bundletool** | [GitHub Releases](https://github.com/google/bundletool) | Convert `.aab` → `.apks` and install |
| **Java JRE 21** | [Eclipse Temurin (Adoptium)](https://adoptium.net/) | Required to run bundletool |

All tools are stored in the app's local data directory — nothing is installed system-wide.

### Update Checks

The app tracks when each tool was last downloaded. If any tool hasn't been updated in **30+ days**, a non-intrusive banner appears suggesting an update. No automatic downloads happen without your consent.

### AAB Installation Flow

1. `bundletool build-apks` generates a device-specific `.apks` set from the `.aab`
2. `bundletool install-apks` sideloads the APK set onto the device
3. Temp files are cleaned up automatically

Custom keystores are supported for signed builds.

## CI / CD

The included GitHub Actions workflow (`.github/workflows/build.yml`) builds the app for:

- **macOS ARM64** (Apple Silicon)
- **macOS x64** (Intel)
- **Windows x64**

Trigger it by pushing a version tag:

```bash
git tag v0.1.0
git push --tags
```

Or run it manually from the GitHub Actions tab.

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Framework | [Tauri 2](https://tauri.app/) |
| Backend | Rust |
| Frontend | React 19 + TypeScript |
| Bundler | Vite |
| UI Icons | [Lucide React](https://lucide.dev/) |
| HTTP | reqwest (Rust) |
| Dialogs | tauri-plugin-dialog |

## License

MIT
