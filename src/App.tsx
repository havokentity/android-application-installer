import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getVersion } from "@tauri-apps/api/app";

import "./App.css";
import type {
  DeviceInfo, LogEntry, ToolsStatus, DownloadProgress, OperationProgress,
  StaleTool, DetectionStatus, RecentFilesConfig,
} from "./types";
import { nextLogId, getFileName, getFileType, now } from "./helpers";

// ─── Hooks ───────────────────────────────────────────────────────────────────
import { useLayout } from "./hooks/useLayout";
import { useEasterEgg } from "./hooks/useEasterEgg";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";

// ─── Components ──────────────────────────────────────────────────────────────
import { Toolbar } from "./components/Toolbar";
import { AppHeader } from "./components/AppHeader";
import { DeviceSection } from "./components/DeviceSection";
import { FileSection } from "./components/FileSection";
import { AabSettingsSection } from "./components/AabSettingsSection";
import { EasterEggOverlay } from "./components/EasterEggOverlay";
import { StaleBanner, ToolsSection } from "./components/ToolsSection";
import { LogPanel } from "./components/LogPanel";

// ─── App Component ────────────────────────────────────────────────────────────

function App() {
  // ── Layout, theme & easter egg (extracted hooks) ──────────────────────
  const { layout, theme, setTheme, sidePanelWidth, toggleLayout, onDividerMouseDown, appRef } = useLayout();
  const { easterEggVisible, easterEggIndex, easterEggVerses, handleTitleClick } = useEasterEgg();

  // ── Tools state ───────────────────────────────────────────────────────
  const [toolsStatus, setToolsStatus] = useState<ToolsStatus | null>(null);
  const [downloadingAdb, setDownloadingAdb] = useState(false);
  const [downloadingBundletool, setDownloadingBundletool] = useState(false);
  const [downloadingJava, setDownloadingJava] = useState(false);
  const [adbProgress, setAdbProgress] = useState<DownloadProgress | null>(null);
  const [btProgress, setBtProgress] = useState<DownloadProgress | null>(null);
  const [javaProgress, setJavaProgress] = useState<DownloadProgress | null>(null);
  const [staleTools, setStaleTools] = useState<StaleTool[]>([]);
  const [staleDismissed, setStaleDismissed] = useState(false);

  // ── ADB state ─────────────────────────────────────────────────────────
  const [adbPath, setAdbPath] = useState("");
  const [adbStatus, setAdbStatus] = useState<DetectionStatus>("unknown");

  // ── Device state ──────────────────────────────────────────────────────
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [selectedDevice, setSelectedDevice] = useState("");
  const [loadingDevices, setLoadingDevices] = useState(false);
  const [deviceExpanded, setDeviceExpanded] = useState(true);
  const [installAllDevices, setInstallAllDevices] = useState(false);

  // ── File state ────────────────────────────────────────────────────────
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [fileType, setFileType] = useState<"apk" | "aab" | null>(null);
  const [packageName, setPackageName] = useState("");
  const [isDragOver, setIsDragOver] = useState(false);

  // ── AAB settings ──────────────────────────────────────────────────────
  const [showAabSettings, setShowAabSettings] = useState(false);
  const [javaPath, setJavaPath] = useState("");
  const [javaVersion, setJavaVersion] = useState("");
  const [javaStatus, setJavaStatus] = useState<DetectionStatus>("unknown");
  const [bundletoolPath, setBundletoolPath] = useState("");
  const [bundletoolStatus, setBundletoolStatus] = useState<DetectionStatus>("unknown");
  const [keystorePath, setKeystorePath] = useState("");
  const [keystorePass, setKeystorePass] = useState("");
  const [keyAlias, setKeyAlias] = useState("");
  const [keyPass, setKeyPass] = useState("");
  const [keyAliases, setKeyAliases] = useState<string[]>([]);
  const [loadingAliases, setLoadingAliases] = useState(false);

  // ── General state ─────────────────────────────────────────────────
  const [isInstalling, setIsInstalling] = useState(false);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [recentFiles, setRecentFiles] = useState<RecentFilesConfig>({ packages: [], keystores: [] });
  const [appVersion, setAppVersion] = useState("");
  const [operationProgress, setOperationProgress] = useState<OperationProgress | null>(null);

  const prevDeviceSerials = useRef("");
  const handleFileSelectedRef = useRef<((path: string) => Promise<void>) | undefined>(undefined);

  // ─── Logging ──────────────────────────────────────────────────────────

  const addLog = useCallback((level: LogEntry["level"], message: string) => {
    setLogs((prev) => [...prev, { id: nextLogId(), time: now(), level, message }]);
  }, []);

  // ── Fetch app version ─────────────────────────────────────────────────
  useEffect(() => { getVersion().then(setAppVersion).catch(() => {}); }, []);

  // ─── Download progress listener ───────────────────────────────────────

  useEffect(() => {
    const unlisten = listen<DownloadProgress>("download-progress", (event) => {
      const p = event.payload;
      if (p.tool === "platform-tools") setAdbProgress(p);
      else if (p.tool === "bundletool") setBtProgress(p);
      else if (p.tool === "java") setJavaProgress(p);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  // ─── Operation progress listener (install / launch / uninstall) ────

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

  // ─── Tools status & staleness ─────────────────────────────────────────

  const checkToolsStatus = useCallback(async () => {
    try {
      setToolsStatus(await invoke<ToolsStatus>("get_tools_status"));
    } catch (e) {
      addLog("warning", `Could not check tools status: ${e}`);
    }
  }, [addLog]);

  const checkStaleTools = useCallback(async () => {
    try {
      const stale = await invoke<StaleTool[]>("check_for_stale_tools");
      setStaleTools(stale);
      if (stale.length > 0) {
        const names = stale.map((s) => `${s.label} (${s.age_days}d ago)`).join(", ");
        addLog("warning", `Some managed tools haven't been updated in 30+ days: ${names}`);
      }
    } catch { /* non-critical */ }
  }, [addLog]);

  useEffect(() => { checkToolsStatus(); }, [checkToolsStatus]);
  useEffect(() => { checkStaleTools(); }, [checkStaleTools]);

  // ─── Recent files ────────────────────────────────────────────────────

  const loadRecentFiles = useCallback(async () => {
    try {
      setRecentFiles(await invoke<RecentFilesConfig>("get_recent_files"));
    } catch { /* non-critical */ }
  }, []);

  useEffect(() => { loadRecentFiles(); }, [loadRecentFiles]);

  const recordRecentFile = useCallback(async (path: string, category: "packages" | "keystores") => {
    try {
      setRecentFiles(await invoke<RecentFilesConfig>("add_recent_file", { path, category }));
    } catch { /* non-critical */ }
  }, []);

  const removeRecentFile = useCallback(async (path: string, category: "packages" | "keystores") => {
    try {
      setRecentFiles(await invoke<RecentFilesConfig>("remove_recent_file", { path, category }));
    } catch { /* non-critical */ }
  }, []);

  // ─── ADB detection ────────────────────────────────────────────────────

  const detectAdb = useCallback(async () => {
    try {
      const path = await invoke<string>("find_adb");
      setAdbPath(path);
      setAdbStatus("found");
      addLog("success", `ADB found: ${path}`);
    } catch (e) {
      setAdbStatus("not-found");
      addLog("warning", String(e));
    }
  }, [addLog]);

  useEffect(() => { detectAdb(); }, [detectAdb]);

  // ─── Tool setup actions ───────────────────────────────────────────────

  const setupAdb = async () => {
    setDownloadingAdb(true);
    setAdbProgress(null);
    addLog("info", "Downloading ADB platform-tools from Google...");
    try {
      const path = await invoke<string>("setup_platform_tools");
      addLog("success", `ADB installed: ${path}`);
      setAdbPath(path);
      setAdbStatus("found");
      await checkToolsStatus();
      await checkStaleTools();
      refreshDevices();
    } catch (e) {
      addLog("error", `ADB setup failed: ${e}`);
    } finally {
      setDownloadingAdb(false);
      setAdbProgress(null);
    }
  };

  const setupBundletool = async () => {
    setDownloadingBundletool(true);
    setBtProgress(null);
    addLog("info", "Downloading bundletool from GitHub...");
    try {
      addLog("success", await invoke<string>("setup_bundletool"));
      await checkToolsStatus();
      await checkStaleTools();
      await detectBundletool();
    } catch (e) {
      addLog("error", `Bundletool setup failed: ${e}`);
    } finally {
      setDownloadingBundletool(false);
      setBtProgress(null);
    }
  };

  const setupJava = async () => {
    setDownloadingJava(true);
    setJavaProgress(null);
    addLog("info", "Downloading Java JRE (Eclipse Temurin 21)...");
    try {
      const path = await invoke<string>("setup_java");
      addLog("success", `Java JRE installed: ${path}`);
      setJavaPath(path);
      setJavaStatus("found");
      await checkToolsStatus();
      await checkStaleTools();
      await checkJava();
    } catch (e) {
      addLog("error", `Java setup failed: ${e}`);
    } finally {
      setDownloadingJava(false);
      setJavaProgress(null);
    }
  };

  // ─── Device refresh ───────────────────────────────────────────────────

  const refreshDevices = useCallback(async () => {
    if (!adbPath) return;
    setLoadingDevices(true);
    try {
      const devs = await invoke<DeviceInfo[]>("get_devices", { adbPath });
      setDevices(devs);
      if (devs.length > 0) {
        if (!selectedDevice || !devs.find((d) => d.serial === selectedDevice)) {
          setSelectedDevice(devs[0].serial);
        }
        addLog("info", `Found ${devs.length} device(s)`);
      } else {
        setSelectedDevice("");
        addLog("warning", "No devices connected. Enable USB debugging on your phone and connect via USB.");
      }
    } catch (e) {
      setDevices([]);
      setSelectedDevice("");
      addLog("error", `Failed to list devices: ${e}`);
    } finally {
      setLoadingDevices(false);
    }
  }, [adbPath, selectedDevice, addLog]);

  useEffect(() => {
    if (adbStatus === "found") refreshDevices();
  }, [adbStatus]); // eslint-disable-line react-hooks/exhaustive-deps

  // ─── Auto device refresh (silent — only logs on change) ────────────────

  const refreshDevicesQuiet = useCallback(async () => {
    if (!adbPath) return;
    try {
      const devs = await invoke<DeviceInfo[]>("get_devices", { adbPath });
      const newSerials = devs.map(d => d.serial).sort().join(",");
      if (newSerials === prevDeviceSerials.current) return;
      prevDeviceSerials.current = newSerials;
      setDevices(devs);
      if (devs.length > 0) {
        setSelectedDevice(prev => {
          if (!prev || !devs.find(d => d.serial === prev)) return devs[0].serial;
          return prev;
        });
        addLog("info", `Device update: ${devs.length} device(s) connected`);
      } else {
        setSelectedDevice("");
        addLog("info", "All devices disconnected.");
      }
    } catch { /* silent */ }
  }, [adbPath, addLog]);

  useEffect(() => {
    prevDeviceSerials.current = devices.map(d => d.serial).sort().join(",");
  }, [devices]);

  useEffect(() => {
    if (adbStatus !== "found" || !adbPath) return;
    const interval = setInterval(refreshDevicesQuiet, 8000);
    const onFocus = () => refreshDevicesQuiet();
    window.addEventListener("focus", onFocus);
    return () => {
      clearInterval(interval);
      window.removeEventListener("focus", onFocus);
    };
  }, [adbStatus, adbPath, refreshDevicesQuiet]);

  useEffect(() => {
    if (selectedDevice && devices.length > 0) setDeviceExpanded(false);
    else setDeviceExpanded(true);
  }, [selectedDevice, devices.length]);

  // ─── File selection ───────────────────────────────────────────────────

  const browseFile = async () => {
    try {
      const file = await open({
        title: "Select APK or AAB file",
        filters: [
          { name: "Android Package", extensions: ["apk", "aab"] },
          { name: "APK Files", extensions: ["apk"] },
          { name: "AAB Files", extensions: ["aab"] },
        ],
      });
      if (file) handleFileSelected(file as string);
    } catch (e) {
      addLog("error", `File dialog error: ${e}`);
    }
  };

  const handleFileSelected = async (path: string) => {
    const ft = getFileType(path);
    if (!ft) { addLog("error", "Please select an APK or AAB file."); return; }

    setSelectedFile(path);
    setFileType(ft);
    addLog("info", `Selected: ${getFileName(path)} (${ft.toUpperCase()})`);
    recordRecentFile(path, "packages");

    if (ft === "apk") {
      try {
        const pkg = await invoke<string>("get_package_name", { apkPath: path });
        setPackageName(pkg);
        addLog("info", `Package: ${pkg}`);
      } catch {
        addLog("info", "Could not auto-detect package name. You can enter it manually for the Launch feature.");
      }
    }

    if (ft === "aab") {
      setShowAabSettings(true);
      if (javaStatus === "unknown") await checkJava();
      if (bundletoolStatus === "unknown") await detectBundletool();

      try {
        const jp = javaPath || (await invoke<string>("check_java")).split("|")[0];
        const bt = bundletoolPath || (await invoke<string>("find_bundletool"));
        if (jp && bt) {
          const pkg = await invoke<string>("get_aab_package_name", {
            aabPath: path, javaPath: jp, bundletoolPath: bt,
          });
          setPackageName(pkg);
          addLog("info", `Package: ${pkg}`);
        }
      } catch {
        addLog("info", "Could not auto-detect package name from AAB. You can enter it manually.");
      }
    }
  };

  handleFileSelectedRef.current = handleFileSelected;

  // ─── Drag & drop ───────────────────────────────────────────────────────

  useEffect(() => {
    const win = getCurrentWindow();
    const unlisten = win.onDragDropEvent((event) => {
      if (event.payload.type === "enter") {
        setIsDragOver(true);
      } else if (event.payload.type === "leave") {
        setIsDragOver(false);
      } else if (event.payload.type === "drop") {
        setIsDragOver(false);
        const paths = event.payload.paths;
        if (paths && paths.length > 0) {
          const file = paths[0];
          if (getFileType(file)) {
            handleFileSelectedRef.current?.(file);
          } else {
            addLog("error", "Unsupported file type. Please drop an APK or AAB file.");
          }
        }
      }
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // ─── Java & bundletool detection ──────────────────────────────────────

  const checkJava = async () => {
    try {
      const result = await invoke<string>("check_java");
      const [path, version] = result.split("|", 2);
      setJavaPath(path);
      setJavaVersion(version);
      setJavaStatus("found");
      addLog("success", `Java found: ${version}`);
    } catch (e) {
      setJavaStatus("not-found");
      addLog("warning", String(e));
    }
  };

  const detectBundletool = async () => {
    try {
      const path = await invoke<string>("find_bundletool");
      setBundletoolPath(path);
      setBundletoolStatus("found");
      addLog("success", `bundletool found: ${path}`);
    } catch {
      setBundletoolStatus("not-found");
      addLog("info", "bundletool not found — use the Download button in AAB Settings or in the Tools section above.");
    }
  };

  const browseKeystore = async () => {
    try {
      const file = await open({
        title: "Select Keystore File",
        filters: [
          { name: "Keystore", extensions: ["jks", "keystore"] },
          { name: "All Files", extensions: ["*"] },
        ],
      });
      if (file) {
        setKeystorePath(file as string);
        setKeyAlias("");
        setKeyAliases([]);
        recordRecentFile(file as string, "keystores");
      }
    } catch (e) {
      addLog("error", `File dialog error: ${e}`);
    }
  };

  // ─── Key alias listing ─────────────────────────────────────────────────

  const fetchKeyAliases = useCallback(async (ksPath: string, ksPass: string) => {
    if (!ksPath || !ksPass || !javaPath) return;
    setLoadingAliases(true);
    try {
      const aliases = await invoke<string[]>("list_key_aliases", {
        javaPath, keystorePath: ksPath, keystorePass: ksPass,
      });
      setKeyAliases(aliases);
      if (aliases.length === 1) setKeyAlias(aliases[0]);
      addLog("info", `Found ${aliases.length} key alias(es) in keystore`);
    } catch (e) {
      setKeyAliases([]);
      addLog("warning", `Could not list key aliases: ${e}`);
    } finally {
      setLoadingAliases(false);
    }
  }, [javaPath, addLog]);

  useEffect(() => {
    if (keystorePath && keystorePass && javaPath) {
      const timer = setTimeout(() => fetchKeyAliases(keystorePath, keystorePass), 500);
      return () => clearTimeout(timer);
    } else {
      setKeyAliases([]);
    }
  }, [keystorePath, keystorePass, javaPath, fetchKeyAliases]);

  // ─── Installation ─────────────────────────────────────────────────────

  const install = async (andRun = false) => {
    if (!selectedFile) { addLog("error", "Please select a file first."); return; }

    const targetDevices = installAllDevices && devices.length > 1
      ? devices.filter(d => d.state === "device").map(d => d.serial)
      : selectedDevice ? [selectedDevice] : [];

    if (targetDevices.length === 0) { addLog("error", "Please select a device first."); return; }

    if (fileType === "aab") {
      if (!javaPath || javaStatus !== "found") { addLog("error", "Java is required for AAB installation. Please install a JDK."); return; }
      if (!bundletoolPath || bundletoolStatus !== "found") { addLog("error", "bundletool is required for AAB installation. Download it in the Tools or AAB Settings section."); return; }
    }

    setIsInstalling(true);
    setOperationProgress(null);
    // Reset cancel flag before the batch
    try { await invoke("set_cancel_flag", { cancel: false }); } catch { /* non-critical */ }

    const fileName = getFileName(selectedFile);
    const multi = targetDevices.length > 1;

    try {
      for (const device of targetDevices) {
        const devInfo = devices.find(d => d.serial === device);
        const deviceLabel = devInfo?.model || device;
        const prefix = multi ? `[${deviceLabel}] ` : "";

        try {
          if (fileType === "apk") {
            addLog("info", `${prefix}Installing ${fileName}...`);
            addLog("success", prefix + await invoke<string>("install_apk", { adbPath, device, apkPath: selectedFile }));
          } else if (fileType === "aab") {
            addLog("info", `${prefix}Installing ${fileName} via bundletool...`);
            addLog("success", prefix + await invoke<string>("install_aab", {
              adbPath, device, aabPath: selectedFile, javaPath, bundletoolPath,
              keystorePath: keystorePath || null, keystorePass: keystorePass || null,
              keyAlias: keyAlias || null, keyPass: keyPass || null,
            }));
          }

          if (andRun && packageName) {
            addLog("info", `${prefix}Launching ${packageName}...`);
            addLog("success", prefix + await invoke<string>("launch_app", { adbPath, device, packageName }));
          } else if (andRun && !packageName) {
            addLog("warning", `${prefix}Cannot launch — package name not set.`);
          }
        } catch (e) {
          const msg = String(e);
          addLog("error", `${prefix}${msg}`);
          if (msg.includes("cancelled")) {
            addLog("warning", `${prefix}Operation cancelled by user.`);
            break;
          }
        }
      }
    } finally {
      setIsInstalling(false);
      setOperationProgress(null);
    }
  };

  const launchApp = async () => {
    if (!packageName || !selectedDevice) { addLog("error", "Please enter a package name and select a device."); return; }
    try { await invoke("set_cancel_flag", { cancel: false }); } catch { /* non-critical */ }
    try {
      addLog("info", `Launching ${packageName}...`);
      addLog("success", await invoke<string>("launch_app", { adbPath, device: selectedDevice, packageName }));
    } catch (e) { addLog("error", String(e)); }
  };

  const uninstallApp = async () => {
    if (!packageName || !selectedDevice) { addLog("error", "Please enter a package name and select a device."); return; }
    try { await invoke("set_cancel_flag", { cancel: false }); } catch { /* non-critical */ }
    try {
      addLog("info", `Uninstalling ${packageName}...`);
      addLog("success", await invoke<string>("uninstall_app", { adbPath, device: selectedDevice, packageName }));
    } catch (e) { addLog("error", String(e)); }
  };

  // ─── Cancel operation ───────────────────────────────────────────────

  const cancelOperation = async () => {
    try {
      await invoke("set_cancel_flag", { cancel: true });
      addLog("warning", "Cancelling operation...");
    } catch (e) {
      addLog("error", `Cancel failed: ${e}`);
    }
  };

  // ─── Derived state ────────────────────────────────────────────────────

  const canInstall = selectedFile &&
    (selectedDevice || (installAllDevices && devices.length > 0)) &&
    !isInstalling && adbStatus === "found";
  const adbManaged = toolsStatus?.adb_installed ?? false;
  const javaManaged = toolsStatus?.java_installed ?? false;
  const toolsMissing = toolsStatus !== null && (!toolsStatus.adb_installed || !toolsStatus.bundletool_installed || !toolsStatus.java_installed);
  const canLaunchOrUninstall = !!packageName && !!selectedDevice && !isInstalling;

  // ── Window title with current file ─────────────────────────────────────
  useEffect(() => {
    const win = getCurrentWindow();
    const base = "Android Application Installer";
    win.setTitle(selectedFile ? `${base} — ${getFileName(selectedFile)}` : base);
  }, [selectedFile]);

  // ── Keyboard shortcuts ─────────────────────────────────────────────────
  useKeyboardShortcuts({
    browseFile, install, launchApp, uninstallApp,
    canInstall, canLaunch: canLaunchOrUninstall, canUninstall: canLaunchOrUninstall,
  });

  // ─── Shared UI elements ───────────────────────────────────────────────

  const toolbarEl = <Toolbar layout={layout} theme={theme} onToggleLayout={toggleLayout} onSetTheme={setTheme} />;
  const headerEl = <AppHeader appVersion={appVersion} onTitleClick={handleTitleClick} />;
  const staleBannerEl = <StaleBanner staleTools={staleTools} dismissed={staleDismissed} onDismiss={() => setStaleDismissed(true)} />;

  const toolsSectionEl = (
    <ToolsSection
      toolsStatus={toolsStatus}
      downloadingAdb={downloadingAdb} downloadingBundletool={downloadingBundletool} downloadingJava={downloadingJava}
      adbProgress={adbProgress} btProgress={btProgress} javaProgress={javaProgress}
      onSetupAdb={setupAdb} onSetupBundletool={setupBundletool} onSetupJava={setupJava}
      needsAttention={toolsMissing} compact={layout === "landscape"} collapsible
    />
  );

  const deviceSectionEl = (
    <DeviceSection
      devices={devices} selectedDevice={selectedDevice} onSelectDevice={setSelectedDevice}
      loadingDevices={loadingDevices} onRefreshDevices={refreshDevices}
      adbPath={adbPath} adbStatus={adbStatus} adbManaged={adbManaged}
      onAdbPathChange={(path, status) => { setAdbPath(path); setAdbStatus(status); }}
      onDetectAdb={detectAdb}
      expanded={deviceExpanded} onToggleExpanded={() => setDeviceExpanded(!deviceExpanded)}
      installAllDevices={installAllDevices} onInstallAllDevicesChange={setInstallAllDevices}
      isInstalling={isInstalling} canInstall={canInstall} packageName={packageName}
      onInstall={install} onLaunch={launchApp} onUninstall={uninstallApp}
      operationProgress={operationProgress} onCancelOperation={cancelOperation}
    />
  );

  const fileSectionEl = (
    <FileSection
      selectedFile={selectedFile} fileType={fileType} isDragOver={isDragOver}
      packageName={packageName} onPackageNameChange={setPackageName}
      onBrowseFile={browseFile} onClearFile={() => { setSelectedFile(null); setFileType(null); setPackageName(""); }}
      onFileSelected={handleFileSelected}
      recentFiles={recentFiles} onRemoveRecentFile={(path) => removeRecentFile(path, "packages")}
    />
  );

  const aabSettingsEl = (
    <AabSettingsSection
      show={showAabSettings} onToggle={() => setShowAabSettings(!showAabSettings)}
      javaPath={javaPath} javaVersion={javaVersion} javaStatus={javaStatus} javaManaged={javaManaged}
      onJavaPathChange={setJavaPath} onCheckJava={checkJava} onSetupJava={setupJava} downloadingJava={downloadingJava}
      bundletoolPath={bundletoolPath} bundletoolStatus={bundletoolStatus}
      onBundletoolPathChange={(path, status) => { setBundletoolPath(path); setBundletoolStatus(status); }}
      onDetectBundletool={detectBundletool} onSetupBundletool={setupBundletool} downloadingBundletool={downloadingBundletool}
      keystorePath={keystorePath} keystorePass={keystorePass} keyAlias={keyAlias} keyPass={keyPass}
      keyAliases={keyAliases} loadingAliases={loadingAliases}
      onKeystorePathChange={setKeystorePath} onKeystorePassChange={setKeystorePass}
      onKeyAliasChange={setKeyAlias} onKeyPassChange={setKeyPass}
      onBrowseKeystore={browseKeystore} onFetchKeyAliases={() => fetchKeyAliases(keystorePath, keystorePass)}
      recentKeystores={recentFiles.keystores}
      onSelectRecentKeystore={(path) => { setKeystorePath(path); setKeyAlias(""); setKeyAliases([]); recordRecentFile(path, "keystores"); }}
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
    </div>
  );
}

export default App;

