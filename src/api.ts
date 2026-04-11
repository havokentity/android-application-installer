// ─── Typed IPC Layer ──────────────────────────────────────────────────────────
// Wraps all Tauri `invoke()` calls with typed functions.
// Single source of truth for command names, argument types, and return types.

import { invoke } from "@tauri-apps/api/core";
import type {
  DeviceInfo, ToolsStatus, StaleTool, RecentFilesConfig,
} from "./types";

// ─── ADB ──────────────────────────────────────────────────────────────────────

export const findAdb = () =>
  invoke<string>("find_adb");

export const getDevices = (adbPath: string) =>
  invoke<DeviceInfo[]>("get_devices", { adbPath });

export const startDeviceTracking = (adbPath: string) =>
  invoke<void>("start_device_tracking", { adbPath });

export const stopDeviceTracking = () =>
  invoke<void>("stop_device_tracking");

// ─── Wireless ADB ─────────────────────────────────────────────────────────────

export const adbPair = (adbPath: string, ipPort: string, pairingCode: string) =>
  invoke<string>("adb_pair", { adbPath, ipPort, pairingCode });

export const adbConnect = (adbPath: string, ipPort: string) =>
  invoke<string>("adb_connect", { adbPath, ipPort });

export const adbDisconnect = (adbPath: string, ipPort: string) =>
  invoke<string>("adb_disconnect", { adbPath, ipPort });

export const installApk = (adbPath: string, device: string, apkPath: string) =>
  invoke<string>("install_apk", { adbPath, device, apkPath });

export const installAab = (params: {
  adbPath: string; device: string; aabPath: string;
  javaPath: string; bundletoolPath: string;
  keystorePath: string | null; keystorePass: string | null;
  keyAlias: string | null; keyPass: string | null;
}) => invoke<string>("install_aab", params);

export const extractApkFromAab = (params: {
  aabPath: string; outputPath: string;
  javaPath: string; bundletoolPath: string;
  keystorePath: string | null; keystorePass: string | null;
  keyAlias: string | null; keyPass: string | null;
}) => invoke<string>("extract_apk_from_aab", params);

export const launchApp = (adbPath: string, device: string, packageName: string) =>
  invoke<string>("launch_app", { adbPath, device, packageName });

export const stopApp = (adbPath: string, device: string, packageName: string) =>
  invoke<string>("stop_app", { adbPath, device, packageName });

export const uninstallApp = (adbPath: string, device: string, packageName: string) =>
  invoke<string>("uninstall_app", { adbPath, device, packageName });

// ─── Package ──────────────────────────────────────────────────────────────────

export const getPackageName = (apkPath: string) =>
  invoke<string>("get_package_name", { apkPath });

export const getAabPackageName = (aabPath: string, javaPath: string, bundletoolPath: string) =>
  invoke<string>("get_aab_package_name", { aabPath, javaPath, bundletoolPath });

// ─── Java & Bundletool ────────────────────────────────────────────────────────

export const checkJava = () =>
  invoke<string>("check_java");

export const findBundletool = () =>
  invoke<string>("find_bundletool");

export const listKeyAliases = (javaPath: string, keystorePath: string, keystorePass: string) =>
  invoke<string[]>("list_key_aliases", { javaPath, keystorePath, keystorePass });

// ─── Tools ────────────────────────────────────────────────────────────────────

export const getToolsStatus = () =>
  invoke<ToolsStatus>("get_tools_status");

export const setupPlatformTools = () =>
  invoke<string>("setup_platform_tools");

export const setupBundletool = () =>
  invoke<string>("setup_bundletool");

export const setupJava = () =>
  invoke<string>("setup_java");

export const checkForStaleTools = () =>
  invoke<StaleTool[]>("check_for_stale_tools");

// ─── Recent Files ─────────────────────────────────────────────────────────────

export const getRecentFiles = () =>
  invoke<RecentFilesConfig>("get_recent_files");

export const addRecentFile = (path: string, category: "packages" | "keystores") =>
  invoke<RecentFilesConfig>("add_recent_file", { path, category });

export const removeRecentFile = (path: string, category: "packages" | "keystores") =>
  invoke<RecentFilesConfig>("remove_recent_file", { path, category });

// ─── Cancellation ─────────────────────────────────────────────────────────────

export const setCancelFlag = (cancel: boolean) =>
  invoke("set_cancel_flag", { cancel });

