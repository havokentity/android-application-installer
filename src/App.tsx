import { useState, useEffect, useCallback, useMemo } from "react";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import { getVersion } from "@tauri-apps/api/app";
import { ask } from "@tauri-apps/plugin-dialog";

import "./App.css";
import type { LogEntry, OperationProgress, RecentFilesConfig, SigningProfile } from "./types";
import { nextLogId, getFileName, now } from "./helpers";
import * as api from "./api";

// ─── Hooks ───────────────────────────────────────────────────────────────────
import { useLayout } from "./hooks/useLayout";
import { useEasterEgg } from "./hooks/useEasterEgg";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import { useUpdater } from "./hooks/useUpdater";
import { useToolsState } from "./hooks/useToolsState";
import { useDeviceState } from "./hooks/useDeviceState";
import { useFileState } from "./hooks/useFileState";
import { useAabSettings } from "./hooks/useAabSettings";
import { useWirelessAdb, deduplicateDevices, isIpPortDevice, isMdnsDevice, enrichWithDiscoveredServices } from "./hooks/useWirelessAdb";
import type { DeduplicatedDevice } from "./hooks/useWirelessAdb";
import type { InstallMode } from "./hooks/useDeviceState";

// ─── Components ──────────────────────────────────────────────────────────────
import { Toolbar } from "./components/Toolbar";
import { AppHeader } from "./components/AppHeader";
import { DeviceSection } from "./components/DeviceSection";
import { FileSection } from "./components/FileSection";
import { AabSettingsSection } from "./components/AabSettingsSection";
import { EasterEggOverlay } from "./components/EasterEggOverlay";
import { StaleBanner, ToolsSection } from "./components/ToolsSection";
import { LogPanel } from "./components/LogPanel";
import { useToast, ToastContainer } from "./components/Toast";

// ─── App Component ────────────────────────────────────────────────────────────

function App() {
  // ── Layout, theme & easter egg ────────────────────────────────────────
  const { layout, theme, setTheme, sidePanelWidth, toggleLayout, onDividerMouseDown, appRef } = useLayout();
  const { easterEggVisible, easterEggIndex, easterEggVerses, handleTitleClick } = useEasterEgg();
  const { toasts, addToast, removeToast } = useToast();

  // ── Logging ──────────────────────────────────────────────────────────
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const addLog = useCallback((level: LogEntry["level"], message: string) => {
    setLogs((prev) => [...prev, { id: nextLogId(), time: now(), level, message }]);
  }, []);

  // ── General state ─────────────────────────────────────────────────
  const [isInstalling, setIsInstalling] = useState(false);
  const [isExtracting, setIsExtracting] = useState(false);
  const [allowDowngrade, setAllowDowngrade] = useState(false);
  const [appVersion, setAppVersion] = useState("");
  const [operationProgress, setOperationProgress] = useState<OperationProgress | null>(null);

  useEffect(() => { getVersion().then(setAppVersion).catch((e) => console.warn("Failed to get app version:", e)); }, []);

  /** Send a native OS notification (non-blocking, best-effort). */
  const notify = useCallback(async (title: string, body: string) => {
    try {
      await api.sendNotification(title, body);
    } catch (e) { console.warn("Notification failed:", e); }
  }, []);

  // ── Operation progress listener ────────────────────────────────────
  useEffect(() => {
    const unlisten = listen<OperationProgress>("operation-progress", (event) => {
      const p = event.payload;
      setOperationProgress(p);
      if (p.status === "done") {
        setTimeout(() => setOperationProgress((prev) => prev?.status === "done" ? null : prev), 1500);
      } else if (p.status === "error" || p.status === "cancelled") {
        setTimeout(() => setOperationProgress((prev) =>
          (prev?.status === "error" || prev?.status === "cancelled") ? null : prev
        ), 500);
      }
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  // ── Recent files ───────────────────────────────────────────────────
  const [recentFiles, setRecentFiles] = useState<RecentFilesConfig>({ packages: [], keystores: [] });

  const loadRecentFiles = useCallback(async () => {
    try { setRecentFiles(await api.getRecentFiles()); } catch (e) { console.warn("Failed to load recent files:", e); }
  }, []);

  useEffect(() => { loadRecentFiles(); }, [loadRecentFiles]);

  const recordRecentFile = useCallback(async (path: string, category: "packages" | "keystores") => {
    try { setRecentFiles(await api.addRecentFile(path, category)); } catch (e) { console.warn("Failed to record recent file:", e); }
  }, []);

  const removeRecentFile = useCallback(async (path: string, category: "packages" | "keystores") => {
    try { setRecentFiles(await api.removeRecentFile(path, category)); } catch (e) { console.warn("Failed to remove recent file:", e); }
  }, []);

  // ── Signing profiles ──────────────────────────────────────────────
  const [signingProfiles, setSigningProfiles] = useState<SigningProfile[]>([]);
  const [activeProfileName, setActiveProfileName] = useState<string | null>(null);

  const loadSigningProfiles = useCallback(async () => {
    try { setSigningProfiles(await api.getSigningProfiles()); } catch (e) { console.warn("Failed to load signing profiles:", e); }
  }, []);

  useEffect(() => { loadSigningProfiles(); }, [loadSigningProfiles]);

  // ── Auto updater ──────────────────────────────────────────────────────
  const updater = useUpdater(addLog);

  // ── ADB detection ────────────────────────────────────────────────────
  const [adbPath, setAdbPath] = useState("");
  const [adbStatus, setAdbStatus] = useState<"unknown" | "found" | "not-found">("unknown");

  const detectAdb = useCallback(async () => {
    try {
      const path = await api.findAdb();
      setAdbPath(path);
      setAdbStatus("found");
      addLog("success", `ADB found: ${path}`);
      addToast("ADB detected successfully", "success");
      localStorage.removeItem("adbPath"); // clear manual override on auto-detect success
    } catch (e) {
      // Fall back to persisted manual ADB path
      const saved = localStorage.getItem("adbPath");
      if (saved) {
        setAdbPath(saved);
        setAdbStatus("found");
        addLog("info", `Using saved ADB path: ${saved}`);
      } else {
        setAdbStatus("not-found");
        addLog("warning", String(e));
        addToast("ADB not found — please install or set path manually", "warning");
      }
    }
  }, [addLog]);

  useEffect(() => { detectAdb(); }, [detectAdb]);

  // Persist manual ADB path changes
  const handleAdbPathChange = useCallback((path: string, status: "found" | "not-found" | "unknown") => {
    setAdbPath(path);
    setAdbStatus(status);
    if (path) {
      localStorage.setItem("adbPath", path);
    } else {
      localStorage.removeItem("adbPath");
    }
  }, []);

  // ── AAB settings ──────────────────────────────────────────────────────
  const aab = useAabSettings({ addLog, recordRecentFile });

  // ── Devices ───────────────────────────────────────────────────────────
  const dev = useDeviceState(adbPath, adbStatus, addLog);
  const wireless = useWirelessAdb({
    adbPath, addLog, addToast,
    onDeviceChange: () => {
      // Quiet refresh immediately, then again after a delay for mDNS twin discovery.
      // Uses quiet (non-verbose) refresh to avoid log spam and UI churn.
      dev.refreshDevicesQuiet();
      setTimeout(() => dev.refreshDevicesQuiet(), 2000);
    },
  });

  // Enrich devices with alternate serials from mDNS discovery data.
  // When only one transport (IP:port or mDNS) appears in `adb devices`,
  // this fills in the missing twin serial so both install modes are available.
  const enrichedDevices = useMemo(
    () => enrichWithDiscoveredServices(dev.devices, wireless.discoveredDevices),
    [dev.devices, wireless.discoveredDevices],
  );

  // Handle install mode change with auto-connect/scan for alternate transport.
  // When switching to "direct" → auto-connect IP:port if not yet in device list.
  // When switching to "verified" → re-scan mDNS to help ADB discover the transport.
  const handleInstallModeChange = useCallback((mode: InstallMode) => {
    dev.setInstallMode(mode);

    const deviceInfo = enrichedDevices.find((d) => d.serial === dev.selectedDevice);
    if (!deviceInfo?.alternateSerial) return;

    if (mode === "direct") {
      // We need the IP:port transport for direct installs
      const ipSerial = isIpPortDevice(deviceInfo.serial) ? deviceInfo.serial : deviceInfo.alternateSerial;
      if (ipSerial && isIpPortDevice(ipSerial) && ipSerial !== deviceInfo.serial) {
        // Check if this IP:port is already connected in ADB
        const alreadyConnected = dev.devices.some((d) => d.serial === ipSerial);
        if (!alreadyConnected) {
          addLog("info", `Connecting ${ipSerial} for direct mode...`);
          wireless.connectDirect(ipSerial);
        }
      }
    } else if (mode === "verified") {
      // We need the mDNS transport for verified installs; can't force-connect,
      // but re-scanning helps ADB discover it faster
      const mdnsSerial = isMdnsDevice(deviceInfo.serial) ? deviceInfo.serial : deviceInfo.alternateSerial;
      if (mdnsSerial && isMdnsDevice(mdnsSerial) && mdnsSerial !== deviceInfo.serial) {
        const alreadyConnected = dev.devices.some((d) => d.serial === mdnsSerial);
        if (!alreadyConnected) {
          addLog("info", "Scanning for mDNS transport for verified mode...");
          wireless.scan();
        }
      }
    }
  }, [enrichedDevices, dev, wireless, addLog]);

  // ── Tools ─────────────────────────────────────────────────────────────
  const tools = useToolsState(addLog, {
    onAdbSetup: (path) => {
      setAdbPath(path);
      setAdbStatus("found");
      dev.refreshDevices();
    },
    onJavaSetup: (path) => {
      aab.setJavaPath(path);
      aab.setJavaStatus("found");
      aab.checkJava();
    },
    onBundletoolSetup: () => {
      aab.detectBundletool();
    },
  });

  // ── File state ────────────────────────────────────────────────────────
  const file = useFileState({
    addLog,
    recordRecentFile,
    getAabToolPaths: () => {
      if (aab.javaPath && aab.bundletoolPath) return { javaPath: aab.javaPath, bundletoolPath: aab.bundletoolPath };
      return null;
    },
    onAabSelected: async (path) => {
      aab.setShowAabSettings(true);
      if (aab.javaStatus === "unknown") await aab.checkJava();
      if (aab.bundletoolStatus === "unknown") await aab.detectBundletool();
      try {
        const pkg = await aab.detectAabPackageName(path);
        if (pkg) {
          file.setPackageName(pkg);
          addLog("info", `Package: ${pkg}`);
        }
      } catch (e) {
        console.warn("AAB package name detection failed:", e);
        addLog("info", "Could not auto-detect package name from AAB. You can enter it manually.");
      }
    },
  });

  // ─── Installation ─────────────────────────────────────────────────────

  /** Resolve the effective ADB serial for a device based on the current install mode.
   *  In "verified" mode, use the mDNS alternate serial if available.
   *  In "direct" mode (default), use the primary (IP:port) serial.
   *  Falls back to primary serial if the preferred transport isn't actually connected. */
  const resolveSerial = useCallback((device: DeduplicatedDevice): string => {
    let preferred = device.serial;

    if (dev.installMode === "verified" && device.alternateSerial) {
      // We want the mDNS serial for verified mode
      if (isIpPortDevice(device.serial)) preferred = device.alternateSerial;
    } else if (dev.installMode === "direct" && device.alternateSerial) {
      // We want the IP:port serial for direct mode
      if (!isIpPortDevice(device.serial)) preferred = device.alternateSerial;
    }

    // Safety: if we resolved to an alternate that isn't actually connected
    // in ADB, fall back to the primary (which is always in the device list)
    if (preferred !== device.serial) {
      const isConnected = dev.devices.some((d) => d.serial === preferred);
      if (!isConnected) return device.serial;
    }

    return preferred;
  }, [dev.installMode, dev.devices]);

  const install = async (andRun = false) => {
    if (!file.selectedFile) { addLog("error", "Please select a file first."); addToast("No file selected", "error"); return; }

    const targetDevices = dev.installAllDevices && enrichedDevices.length > 1
      ? deduplicateDevices(enrichedDevices.filter((d) => d.state === "device")).map((d) => ({
          serial: resolveSerial(d),
          label: d.model || d.serial,
        }))
      : dev.selectedDevice
        ? (() => {
            const devInfo = enrichedDevices.find((d) => d.serial === dev.selectedDevice);
            const effectiveSerial = devInfo ? resolveSerial(devInfo) : dev.selectedDevice;
            return [{ serial: effectiveSerial, label: devInfo?.model || dev.selectedDevice }];
          })()
        : [];

    if (targetDevices.length === 0) { addLog("error", "Please select a device first."); addToast("No device selected", "error"); return; }

    if (file.fileType === "aab") {
      if (!aab.javaPath || aab.javaStatus !== "found") { addLog("error", "Java is required for AAB installation. Please install a JDK."); return; }
      if (!aab.bundletoolPath || aab.bundletoolStatus !== "found") { addLog("error", "bundletool is required for AAB installation. Download it in the Tools or AAB Settings section."); return; }
    }

    setIsInstalling(true);
    setOperationProgress(null);
    try { await api.setCancelFlag(false); } catch (e) { console.warn("setCancelFlag failed:", e); }

    const fileName = getFileName(file.selectedFile);
    const multi = targetDevices.length > 1;

    try {
      for (const target of targetDevices) {
        const { serial: device, label: deviceLabel } = target;
        const prefix = multi ? `[${deviceLabel}] ` : "";

        try {
          if (file.fileType === "apk") {
            addLog("info", `${prefix}Installing ${fileName}${allowDowngrade ? " (downgrade allowed)" : ""}...`);
            addLog("success", prefix + await api.installApk(adbPath, device, file.selectedFile, allowDowngrade));
            addToast(`${fileName} installed on ${deviceLabel}`, "success");
          } else if (file.fileType === "aab") {
            addLog("info", `${prefix}Installing ${fileName} via bundletool${allowDowngrade ? " (downgrade allowed)" : ""}...`);
            addLog("success", prefix + await api.installAab({
              adbPath, device, aabPath: file.selectedFile,
              javaPath: aab.javaPath, bundletoolPath: aab.bundletoolPath,
              keystorePath: aab.keystorePath || null, keystorePass: aab.keystorePass || null,
              keyAlias: aab.keyAlias || null, keyPass: aab.keyPass || null,
              allowDowngrade,
            }));
            addToast(`${fileName} installed on ${deviceLabel}`, "success");
          }

          if (andRun && file.packageName) {
            addLog("info", `${prefix}Launching ${file.packageName}...`);
            addLog("success", prefix + await api.launchApp(adbPath, device, file.packageName));
          } else if (andRun && !file.packageName) {
            addLog("warning", `${prefix}Cannot launch — package name not set.`);
          }
        } catch (e) {
          const msg = String(e);
          addLog("error", `${prefix}${msg}`);
          if (msg.includes("cancelled")) {
            addLog("warning", `${prefix}Operation cancelled by user.`);
            addToast("Installation cancelled", "warning");
            break;
          }
          addToast(`Install failed on ${deviceLabel}`, "error");
        }
      }
    } finally {
      setIsInstalling(false);
      setOperationProgress(null);
      notify("Installation Complete", `${getFileName(file.selectedFile!)} has been installed.`);
    }
  };

  const launchApp = async () => {
    if (!file.packageName || !dev.selectedDevice) { addLog("error", "Please enter a package name and select a device."); addToast("Package name or device missing", "error"); return; }
    const devInfo = enrichedDevices.find((d) => d.serial === dev.selectedDevice);
    const effectiveSerial = devInfo ? resolveSerial(devInfo) : dev.selectedDevice;
    try { await api.setCancelFlag(false); } catch (e) { console.warn("setCancelFlag failed:", e); }
    try {
      addLog("info", `Launching ${file.packageName}...`);
      addLog("success", await api.launchApp(adbPath, effectiveSerial, file.packageName));
      addToast(`${file.packageName} launched`, "success");
    } catch (e) { addLog("error", String(e)); addToast("Failed to launch app", "error"); }
  };

  const stopApp = async () => {
    if (!file.packageName || !dev.selectedDevice) { addLog("error", "Please enter a package name and select a device."); addToast("Package name or device missing", "error"); return; }
    const devInfo = enrichedDevices.find((d) => d.serial === dev.selectedDevice);
    const effectiveSerial = devInfo ? resolveSerial(devInfo) : dev.selectedDevice;
    try { await api.setCancelFlag(false); } catch (e) { console.warn("setCancelFlag failed:", e); }
    try {
      addLog("info", `Stopping ${file.packageName}...`);
      addLog("success", await api.stopApp(adbPath, effectiveSerial, file.packageName));
      addToast(`${file.packageName} stopped`, "info");
    } catch (e) { addLog("error", String(e)); addToast("Failed to stop app", "error"); }
  };

  const uninstallApp = async () => {
    if (!file.packageName || !dev.selectedDevice) { addLog("error", "Please enter a package name and select a device."); return; }
    const devInfo = enrichedDevices.find((d) => d.serial === dev.selectedDevice);
    const effectiveSerial = devInfo ? resolveSerial(devInfo) : dev.selectedDevice;
    const confirmed = await ask(`Are you sure you want to uninstall ${file.packageName}?\n\nThis will remove the app and all its data from the device.`, {
      title: "Confirm Uninstall", kind: "warning", okLabel: "Uninstall", cancelLabel: "Cancel",
    });
    if (!confirmed) return;
    try { await api.setCancelFlag(false); } catch (e) { console.warn("setCancelFlag failed:", e); }
    try {
      addLog("info", `Uninstalling ${file.packageName}...`);
      addLog("success", await api.uninstallApp(adbPath, effectiveSerial, file.packageName));
      addToast(`${file.packageName} uninstalled`, "success");
    } catch (e) { addLog("error", String(e)); addToast("Uninstall failed", "error"); }
  };

  const cancelOperation = async () => {
    try {
      await api.setCancelFlag(true);
      addLog("warning", "Cancelling operation...");
      addToast("Cancelling operation…", "warning");
    } catch (e) {
      addLog("error", `Cancel failed: ${e}`);
      addToast("Failed to cancel operation", "error");
    }
  };

  // ─── Extract APK from AAB ─────────────────────────────────────────

  const extractApk = async () => {
    if (!file.selectedFile || file.fileType !== "aab") { addLog("error", "Please select an AAB file first."); return; }
    if (!aab.javaPath || aab.javaStatus !== "found") { addLog("error", "Java is required for APK extraction. Please install a JDK."); return; }
    if (!aab.bundletoolPath || aab.bundletoolStatus !== "found") { addLog("error", "bundletool is required for APK extraction. Download it in the Tools or AAB Settings section."); return; }

    const stem = getFileName(file.selectedFile).replace(/\.aab$/i, "");
    const outputPath = await save({
      title: "Save extracted APK", defaultPath: `${stem}.apk`,
      filters: [{ name: "APK Files", extensions: ["apk"] }],
    });
    if (!outputPath) return;

    setIsExtracting(true);
    setOperationProgress(null);
    try { await api.setCancelFlag(false); } catch (e) { console.warn("setCancelFlag failed:", e); }

    try {
      addLog("info", `Extracting universal APK from ${getFileName(file.selectedFile)}...`);
      const result = await api.extractApkFromAab({
        aabPath: file.selectedFile, outputPath,
        javaPath: aab.javaPath, bundletoolPath: aab.bundletoolPath,
        keystorePath: aab.keystorePath || null, keystorePass: aab.keystorePass || null,
        keyAlias: aab.keyAlias || null, keyPass: aab.keyPass || null,
      });
      addLog("success", result);
      addToast("APK extracted successfully", "success");
    } catch (e) {
      addLog("error", String(e));
      addToast("APK extraction failed", "error");
    } finally {
      setIsExtracting(false);
      setOperationProgress(null);
      notify("Extraction Complete", `APK extracted from ${getFileName(file.selectedFile!)}`);
    }
  };

  // ─── Derived state ────────────────────────────────────────────────────

  const canInstall = !!(file.selectedFile &&
    (dev.selectedDevice || (dev.installAllDevices && enrichedDevices.length > 0)) &&
    !isInstalling && !isExtracting && adbStatus === "found");
  const canExtract = file.selectedFile && file.fileType === "aab" && !isExtracting && !isInstalling &&
    aab.javaStatus === "found" && aab.bundletoolStatus === "found";
  const adbManaged = tools.toolsStatus?.adb_installed ?? false;
  const javaManaged = tools.toolsStatus?.java_installed ?? false;
  const toolsMissing = tools.toolsStatus !== null && (!tools.toolsStatus.adb_installed || !tools.toolsStatus.bundletool_installed || !tools.toolsStatus.java_installed);
  const canLaunchOrUninstall = !!file.packageName && !!dev.selectedDevice && !isInstalling;

  // ── Keyboard shortcuts ─────────────────────────────────────────────────
  useKeyboardShortcuts({
    browseFile: file.browseFile, install, launchApp, stopApp, uninstallApp, extractApk,
    canInstall, canLaunch: canLaunchOrUninstall, canStop: canLaunchOrUninstall, canUninstall: canLaunchOrUninstall, canExtract: !!canExtract,
  });

  // ─── Shared UI elements ───────────────────────────────────────────────

  const toolbarEl = <Toolbar layout={layout} theme={theme} onToggleLayout={toggleLayout} onSetTheme={setTheme} onCheckForUpdates={() => updater.checkForUpdates(true)} checkingForUpdates={updater.checkingForUpdates} updateProgress={updater.updateProgress} autoCheckUpdates={updater.autoCheckUpdates} onToggleAutoCheck={updater.toggleAutoCheckUpdates} />;
  const headerEl = <AppHeader appVersion={appVersion} onTitleClick={handleTitleClick} />;
  const staleBannerEl = <StaleBanner staleTools={tools.staleTools} dismissed={tools.staleDismissed} onDismiss={() => tools.setStaleDismissed(true)} />;

  const toolsSectionEl = (
    <ToolsSection
      toolsStatus={tools.toolsStatus}
      downloadingAdb={tools.downloadingAdb} downloadingBundletool={tools.downloadingBundletool} downloadingJava={tools.downloadingJava}
      adbProgress={tools.adbProgress} btProgress={tools.btProgress} javaProgress={tools.javaProgress}
      onSetupAdb={tools.setupAdb} onSetupBundletool={tools.setupBundletool} onSetupJava={tools.setupJava}
      needsAttention={toolsMissing} compact={layout === "landscape"} collapsible
    />
  );

  const deviceSectionEl = (
    <DeviceSection
      devices={enrichedDevices} selectedDevice={dev.selectedDevice} onSelectDevice={dev.setSelectedDevice}
      loadingDevices={dev.loadingDevices} onRefreshDevices={dev.refreshDevices}
      adbPath={adbPath} adbStatus={adbStatus} adbManaged={adbManaged}
      onAdbPathChange={handleAdbPathChange}
      onDetectAdb={detectAdb}
      expanded={dev.deviceExpanded} onToggleExpanded={() => dev.setDeviceExpanded(!dev.deviceExpanded)}
      installAllDevices={dev.installAllDevices} onInstallAllDevicesChange={dev.setInstallAllDevices}
      installMode={dev.installMode} onInstallModeChange={handleInstallModeChange}
      isInstalling={isInstalling} canInstall={canInstall} packageName={file.packageName}
      onInstall={install} onLaunch={launchApp} onStopApp={stopApp} onUninstall={uninstallApp}
      operationProgress={operationProgress} onCancelOperation={cancelOperation}
      wireless={wireless}
    />
  );

  const fileSectionEl = (
    <FileSection
      selectedFile={file.selectedFile} fileType={file.fileType} fileSize={file.fileSize} isDragOver={file.isDragOver} isDragRejected={file.isDragRejected}
      packageName={file.packageName} onPackageNameChange={file.setPackageName}
      onBrowseFile={file.browseFile} onClearFile={file.clearFile}
      onFileSelected={file.handleFileSelected}
      recentFiles={recentFiles} onRemoveRecentFile={(path) => removeRecentFile(path, "packages")}
      canExtract={!!canExtract} isExtracting={isExtracting} onExtractApk={extractApk}
      allowDowngrade={allowDowngrade} onAllowDowngradeChange={setAllowDowngrade}
      metadata={file.metadata}
    />
  );

  const aabSettingsEl = (
    <AabSettingsSection
      show={aab.showAabSettings} onToggle={() => aab.setShowAabSettings(!aab.showAabSettings)}
      javaPath={aab.javaPath} javaVersion={aab.javaVersion} javaStatus={aab.javaStatus} javaManaged={javaManaged}
      onJavaPathChange={aab.setJavaPath} onCheckJava={aab.checkJava} onSetupJava={tools.setupJava} downloadingJava={tools.downloadingJava}
      bundletoolPath={aab.bundletoolPath} bundletoolStatus={aab.bundletoolStatus}
      onBundletoolPathChange={(path, status) => { aab.setBundletoolPath(path); aab.setBundletoolStatus(status); }}
      onDetectBundletool={aab.detectBundletool} onSetupBundletool={tools.setupBundletool} downloadingBundletool={tools.downloadingBundletool}
      keystorePath={aab.keystorePath} keystorePass={aab.keystorePass} keyAlias={aab.keyAlias} keyPass={aab.keyPass}
      keyAliases={aab.keyAliases} loadingAliases={aab.loadingAliases}
      onKeystorePathChange={aab.setKeystorePath} onKeystorePassChange={aab.setKeystorePass}
      onKeyAliasChange={aab.setKeyAlias} onKeyPassChange={aab.setKeyPass}
      onBrowseKeystore={aab.browseKeystore} onFetchKeyAliases={() => aab.fetchKeyAliases(aab.keystorePath, aab.keystorePass)}
      recentKeystores={recentFiles.keystores}
      onSelectRecentKeystore={(path) => { aab.setKeystorePath(path); aab.setKeyAlias(""); recordRecentFile(path, "keystores"); }}
      onRemoveRecentKeystore={(path) => removeRecentFile(path, "keystores")}
      signingProfiles={signingProfiles}
      activeProfileName={activeProfileName}
      onSelectProfile={(name: string | null) => {
        const profile = signingProfiles.find(p => p.name === name);
        if (profile) {
          aab.setKeystorePath(profile.keystorePath);
          aab.setKeystorePass(profile.keystorePass);
          aab.setKeyAlias(profile.keyAlias);
          aab.setKeyPass(profile.keyPass);
          setActiveProfileName(name);
          addLog("info", `Loaded signing profile: ${name}`);
        } else {
          aab.setKeystorePath(""); aab.setKeystorePass(""); aab.setKeyAlias(""); aab.setKeyPass("");
          setActiveProfileName(null);
        }
      }}
      onSaveProfile={async (name: string) => {
        const profile: SigningProfile = { name, keystorePath: aab.keystorePath, keystorePass: aab.keystorePass, keyAlias: aab.keyAlias, keyPass: aab.keyPass };
        try {
          setSigningProfiles(await api.saveSigningProfile(profile));
          setActiveProfileName(name);
          addToast(`Saved signing profile "${name}"`, "success");
        } catch (e) { addToast(`Failed to save profile: ${e}`, "error"); }
      }}
      onDeleteProfile={async (name: string) => {
        try {
          setSigningProfiles(await api.deleteSigningProfile(name));
          if (activeProfileName === name) setActiveProfileName(null);
          addToast(`Deleted signing profile "${name}"`, "info");
        } catch (e) { addToast(`Failed to delete profile: ${e}`, "error"); }
      }}
    />
  );

  const saveLogs = useCallback(async () => {
    const outputPath = await save({
      title: "Save Log", defaultPath: `install-log-${new Date().toISOString().slice(0, 10)}.log`,
      filters: [{ name: "Log Files", extensions: ["log", "txt"] }],
    });
    if (!outputPath) return;
    const text = logs.map(e => `[${e.time}] [${e.level.toUpperCase()}] ${e.message}`).join("\n");
    try {
      await api.saveTextFile(outputPath, text);
      addToast("Log saved successfully", "success");
    } catch (e) {
      addToast(`Failed to save log: ${e}`, "error");
    }
  }, [logs, addToast]);

  const logPanelEl = <LogPanel logs={logs} onClear={() => setLogs([])} onSaveLogs={saveLogs} />;
  const easterEggEl = <EasterEggOverlay visible={easterEggVisible} verse={easterEggVerses[easterEggIndex]} />;

  // ─── Render ───────────────────────────────────────────────────────────

  if (layout === "landscape") {
    return (
      <div className="app landscape" ref={appRef} style={{ gridTemplateColumns: `1fr auto ${sidePanelWidth}px` }}>
        {toolbarEl}
        {headerEl}
        <div className="main-content">
          {fileSectionEl}
          {deviceSectionEl}
          {aabSettingsEl}
        </div>
        <div className="landscape-divider" onMouseDown={onDividerMouseDown}>
          <div className="divider-handle" />
        </div>
        <div className="side-panel">
          {staleBannerEl}
          {toolsSectionEl}
          {logPanelEl}
        </div>
        {easterEggEl}
        <ToastContainer toasts={toasts} onDismiss={removeToast} />
      </div>
    );
  }

  return (
    <div className="app">
      {toolbarEl}
      {headerEl}
      {staleBannerEl}
      {fileSectionEl}
      {deviceSectionEl}
      {aabSettingsEl}
      {toolsSectionEl}
      {logPanelEl}
      {easterEggEl}
      <ToastContainer toasts={toasts} onDismiss={removeToast} />
    </div>
  );
}

export default App;
