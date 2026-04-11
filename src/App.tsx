import { useState, useEffect, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import { getVersion } from "@tauri-apps/api/app";
import { ask } from "@tauri-apps/plugin-dialog";

import "./App.css";
import type { LogEntry, OperationProgress, RecentFilesConfig } from "./types";
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
import { useWirelessAdb } from "./hooks/useWirelessAdb";

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
  const [appVersion, setAppVersion] = useState("");
  const [operationProgress, setOperationProgress] = useState<OperationProgress | null>(null);

  useEffect(() => { getVersion().then(setAppVersion).catch(() => {}); }, []);

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
    try { setRecentFiles(await api.getRecentFiles()); } catch { /* non-critical */ }
  }, []);

  useEffect(() => { loadRecentFiles(); }, [loadRecentFiles]);

  const recordRecentFile = useCallback(async (path: string, category: "packages" | "keystores") => {
    try { setRecentFiles(await api.addRecentFile(path, category)); } catch { /* non-critical */ }
  }, []);

  const removeRecentFile = useCallback(async (path: string, category: "packages" | "keystores") => {
    try { setRecentFiles(await api.removeRecentFile(path, category)); } catch { /* non-critical */ }
  }, []);

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
  const wireless = useWirelessAdb({ adbPath, addLog, addToast });

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
      } catch {
        addLog("info", "Could not auto-detect package name from AAB. You can enter it manually.");
      }
    },
  });

  // ─── Installation ─────────────────────────────────────────────────────

  const install = async (andRun = false) => {
    if (!file.selectedFile) { addLog("error", "Please select a file first."); addToast("No file selected", "error"); return; }

    const targetDevices = dev.installAllDevices && dev.devices.length > 1
      ? dev.devices.filter((d) => d.state === "device").map((d) => d.serial)
      : dev.selectedDevice ? [dev.selectedDevice] : [];

    if (targetDevices.length === 0) { addLog("error", "Please select a device first."); addToast("No device selected", "error"); return; }

    if (file.fileType === "aab") {
      if (!aab.javaPath || aab.javaStatus !== "found") { addLog("error", "Java is required for AAB installation. Please install a JDK."); return; }
      if (!aab.bundletoolPath || aab.bundletoolStatus !== "found") { addLog("error", "bundletool is required for AAB installation. Download it in the Tools or AAB Settings section."); return; }
    }

    setIsInstalling(true);
    setOperationProgress(null);
    try { await api.setCancelFlag(false); } catch { /* non-critical */ }

    const fileName = getFileName(file.selectedFile);
    const multi = targetDevices.length > 1;

    try {
      for (const device of targetDevices) {
        const devInfo = dev.devices.find((d) => d.serial === device);
        const deviceLabel = devInfo?.model || device;
        const prefix = multi ? `[${deviceLabel}] ` : "";

        try {
          if (file.fileType === "apk") {
            addLog("info", `${prefix}Installing ${fileName}...`);
            addLog("success", prefix + await api.installApk(adbPath, device, file.selectedFile));
            addToast(`${fileName} installed on ${deviceLabel}`, "success");
          } else if (file.fileType === "aab") {
            addLog("info", `${prefix}Installing ${fileName} via bundletool...`);
            addLog("success", prefix + await api.installAab({
              adbPath, device, aabPath: file.selectedFile,
              javaPath: aab.javaPath, bundletoolPath: aab.bundletoolPath,
              keystorePath: aab.keystorePath || null, keystorePass: aab.keystorePass || null,
              keyAlias: aab.keyAlias || null, keyPass: aab.keyPass || null,
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
    }
  };

  const launchApp = async () => {
    if (!file.packageName || !dev.selectedDevice) { addLog("error", "Please enter a package name and select a device."); addToast("Package name or device missing", "error"); return; }
    try { await api.setCancelFlag(false); } catch { /* non-critical */ }
    try {
      addLog("info", `Launching ${file.packageName}...`);
      addLog("success", await api.launchApp(adbPath, dev.selectedDevice, file.packageName));
      addToast(`${file.packageName} launched`, "success");
    } catch (e) { addLog("error", String(e)); addToast("Failed to launch app", "error"); }
  };

  const stopApp = async () => {
    if (!file.packageName || !dev.selectedDevice) { addLog("error", "Please enter a package name and select a device."); addToast("Package name or device missing", "error"); return; }
    try { await api.setCancelFlag(false); } catch { /* non-critical */ }
    try {
      addLog("info", `Stopping ${file.packageName}...`);
      addLog("success", await api.stopApp(adbPath, dev.selectedDevice, file.packageName));
      addToast(`${file.packageName} stopped`, "info");
    } catch (e) { addLog("error", String(e)); addToast("Failed to stop app", "error"); }
  };

  const uninstallApp = async () => {
    if (!file.packageName || !dev.selectedDevice) { addLog("error", "Please enter a package name and select a device."); return; }
    const confirmed = await ask(`Are you sure you want to uninstall ${file.packageName}?\n\nThis will remove the app and all its data from the device.`, {
      title: "Confirm Uninstall", kind: "warning", okLabel: "Uninstall", cancelLabel: "Cancel",
    });
    if (!confirmed) return;
    try { await api.setCancelFlag(false); } catch { /* non-critical */ }
    try {
      addLog("info", `Uninstalling ${file.packageName}...`);
      addLog("success", await api.uninstallApp(adbPath, dev.selectedDevice, file.packageName));
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
    try { await api.setCancelFlag(false); } catch { /* non-critical */ }

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
    }
  };

  // ─── Derived state ────────────────────────────────────────────────────

  const canInstall = file.selectedFile &&
    (dev.selectedDevice || (dev.installAllDevices && dev.devices.length > 0)) &&
    !isInstalling && !isExtracting && adbStatus === "found";
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
      devices={dev.devices} selectedDevice={dev.selectedDevice} onSelectDevice={dev.setSelectedDevice}
      loadingDevices={dev.loadingDevices} onRefreshDevices={dev.refreshDevices}
      adbPath={adbPath} adbStatus={adbStatus} adbManaged={adbManaged}
      onAdbPathChange={handleAdbPathChange}
      onDetectAdb={detectAdb}
      expanded={dev.deviceExpanded} onToggleExpanded={() => dev.setDeviceExpanded(!dev.deviceExpanded)}
      installAllDevices={dev.installAllDevices} onInstallAllDevicesChange={dev.setInstallAllDevices}
      isInstalling={isInstalling} canInstall={canInstall} packageName={file.packageName}
      onInstall={install} onLaunch={launchApp} onStopApp={stopApp} onUninstall={uninstallApp}
      operationProgress={operationProgress} onCancelOperation={cancelOperation}
      wireless={wireless}
    />
  );

  const fileSectionEl = (
    <FileSection
      selectedFile={file.selectedFile} fileType={file.fileType} isDragOver={file.isDragOver} isDragRejected={file.isDragRejected}
      packageName={file.packageName} onPackageNameChange={file.setPackageName}
      onBrowseFile={file.browseFile} onClearFile={file.clearFile}
      onFileSelected={file.handleFileSelected}
      recentFiles={recentFiles} onRemoveRecentFile={(path) => removeRecentFile(path, "packages")}
      canExtract={!!canExtract} isExtracting={isExtracting} onExtractApk={extractApk}
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
    />
  );

  const logPanelEl = <LogPanel logs={logs} onClear={() => setLogs([])} />;
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
