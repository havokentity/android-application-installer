# Wireless ADB (WiFi) Guide

Deploy APKs and AABs to your Android device over WiFi — no USB cable required.
Requires **Android 11+** and both devices on the **same local network**.

> **Note:** Your computer does NOT need to be on WiFi. Ethernet/LAN works fine — "wireless" refers to the phone's connection. As long as both are on the same subnet, it works.

---

## Prerequisites

| Requirement | Details |
|-------------|---------|
| Android version | 11 (API 30) or later |
| ADB version | Platform-tools **30.0.0+** (the app manages this automatically) |
| Network | Phone and computer on the same WiFi / LAN subnet |
| Developer Options | USB debugging **and** Wireless debugging enabled |

---

## Quick Start

### 1. Enable Wireless Debugging on your phone

1. **Settings → Developer Options → Wireless Debugging** → toggle **ON**
2. Confirm the dialog if prompted

> If you don't see Developer Options, go to **Settings → About Phone** and tap **Build Number** 7 times.

### 2. Open the WiFi panel in the app

In the **Device** section, click the **WiFi icon** (📶) next to the Refresh button.
This expands the **Wireless ADB (Android 11+)** panel.

### 3. Pair (first time only)

1. On your phone, tap **"Pair device with pairing code"** (inside the Wireless Debugging screen).
   It will display:
   - An **IP address & port** (e.g. `192.168.1.42:37215`)
   - A **6-digit pairing code** (e.g. `482301`)
2. In the app's **"1. Pair"** row, fill in:
   - **IP address** — e.g. `192.168.1.42`
   - **Port** — the *pairing* port shown (e.g. `37215`)
   - **Pairing code** — the 6-digit code
3. Click **Pair**.
4. You should see a ✅ *"Device paired successfully"* toast.

> ⚠️ The pairing port and code are **temporary** — they expire when the dialog is dismissed.
> Enter them quickly while the dialog is still open on the phone.

### 4. Connect

1. Go back to the main **Wireless Debugging** screen on your phone (dismiss the pairing dialog).
   It shows an **IP address & port** (e.g. `192.168.1.42:43567`).
   - ⚠️ This port is **different** from the pairing port.
2. In the app's **"2. Connect"** row, fill in:
   - **IP address** — same IP (auto-filled if you just paired)
   - **Port** — the *connection* port from the Wireless Debugging screen
3. Click **Connect**.
4. Your device appears in the device dropdown as `192.168.1.42:43567`.

### 5. Deploy

Once connected, wireless devices work exactly like USB devices:

- Select an APK or AAB file
- Click **Install** or **Install & Run**
- Use **Launch**, **Stop**, **Uninstall** as normal

---

## Network Discovery (Auto-Scan)

Instead of manually entering IP addresses and ports, you can **scan your local network** for Android devices that have Wireless Debugging enabled.

1. Open the WiFi panel (click the 📶 button)
2. In the **"Devices on network"** section at the bottom, click **Scan**
3. Discovered devices appear as a list with:
   - **Device name** (e.g. `adb-PIXEL7`)
   - **IP:port** address
   - **Type badge**: "Connect" (ready to connect) or "Pair" (needs pairing first)
4. Click **Use** next to a device to auto-fill the IP and port fields

> Requires ADB platform-tools **31+** for mDNS support. If your ADB is older, the app will show a warning.

---

## Disconnecting

When a wireless device is selected, a **Disconnect** button appears below the device dropdown. Click it to disconnect.

---

## Tips & Troubleshooting

| Issue | Solution |
|-------|----------|
| **"protocol fault" or "Undefined error: 0"** | The pairing code or port expired. Re-open the pairing dialog on your phone and try again with the fresh code. |
| **"Connection refused"** | Wireless Debugging may have been toggled off, or the port changed. Check the Wireless Debugging screen for the current port. |
| **"Connection timed out"** | Devices are on different subnets or a firewall is blocking. Ensure both are on the same WiFi. |
| **Pair succeeds but Connect fails** | You're using the *pairing* port for Connect. The *connection* port is different — it's shown on the main Wireless Debugging screen, not the pairing dialog. |
| **Connection drops after a while** | Android may disable Wireless Debugging when the screen turns off. Keep the screen on, or re-connect when needed. |
| **Only need to pair once** | After the initial pairing, you only need to **Connect** each session. The pairing is remembered. |

---

## How It Works (Architecture)

```
┌──────────────────────────────────────────────┐
│  Frontend (React)                            │
│                                              │
│  DeviceSection.tsx                            │
│    ├─ WiFi panel (pair/connect/disconnect)   │
│    └─ Network discovery list + Scan button   │
│                                              │
│  useWirelessAdb.ts (hook)                    │
│    ├─ IP/port/code state management          │
│    ├─ Validation (isValidIp, isValidPort,    │
│    │   isValidPairingCode)                   │
│    ├─ mDNS discovery (scan, selectDiscovered)│
│    └─ Calls api.ts → Tauri invoke            │
├──────────────────────────────────────────────┤
│  Backend (Rust / Tauri)                      │
│                                              │
│  adb.rs                                      │
│    ├─ adb_pair()    → `adb pair <ip:port> <code>` │
│    ├─ adb_connect() → `adb connect <ip:port>`     │
│    ├─ adb_disconnect() → `adb disconnect <ip:port>`│
│    ├─ adb_mdns_check() → `adb mdns check`         │
│    ├─ adb_mdns_services() → `adb mdns services`   │
│    ├─ parse_pair_result()                    │
│    ├─ parse_connect_result()                 │
│    ├─ parse_disconnect_result()              │
│    └─ parse_mdns_services()                  │
└──────────────────────────────────────────────┘
```

### Key files

| File | Role |
|------|------|
| `src/hooks/useWirelessAdb.ts` | State management, validation, mDNS discovery, IPC calls |
| `src/components/DeviceSection.tsx` | WiFi panel UI, discovery list, disconnect button |
| `src/api.ts` | Typed `invoke()` wrappers (`adbPair`, `adbConnect`, `adbDisconnect`, `adbMdnsCheck`, `adbMdnsServices`) |
| `src-tauri/src/adb.rs` | Rust commands + output parsers |

### Test coverage

- **42 tests** in `useWirelessAdb.test.ts` — validation functions, hook state, pair/connect/disconnect, discovery
- **24 WiFi-specific tests** in `DeviceSection.test.tsx` — panel toggle, pair/connect buttons, disconnect, discovery list, scan
- **18 Rust tests** in `adb.rs` — `parse_pair_result`, `parse_connect_result`, `parse_disconnect_result`, `parse_mdns_services` edge cases

