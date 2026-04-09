import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Smartphone, RefreshCw, FolderOpen, Download, Play, Rocket,
  Check, X, AlertTriangle, Search, Settings, Loader2, Package,
  MonitorSmartphone, Coffee, ChevronDown, ChevronRight, Trash2,
} from "lucide-react";

import "./App.css";
import type {
  DeviceInfo, LogEntry, ToolsStatus, DownloadProgress,
  StaleTool, DetectionStatus,
} from "./types";
import { nextLogId, getFileName, getFileType, now } from "./helpers";
import { StatusDot } from "./components/StatusIndicators";
import { StaleBanner, ToolsSection } from "./components/ToolsSection";
import { LogPanel } from "./components/LogPanel";

// ─── App Component ────────────────────────────────────────────────────────────

function App() {
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

  // ── File state ────────────────────────────────────────────────────────
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [fileType, setFileType] = useState<"apk" | "aab" | null>(null);
  const [packageName, setPackageName] = useState("");

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

  // ── General state ─────────────────────────────────────────────────────
  const [isInstalling, setIsInstalling] = useState(false);
  const [logs, setLogs] = useState<LogEntry[]>([]);

  // ─── Logging ──────────────────────────────────────────────────────────

  const addLog = useCallback((level: LogEntry["level"], message: string) => {
    setLogs((prev) => [...prev, { id: nextLogId(), time: now(), level, message }]);
  }, []);

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
      if (javaStatus === "unknown") checkJava();
      if (bundletoolStatus === "unknown") detectBundletool();
    }
  };

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
      if (file) setKeystorePath(file as string);
    } catch (e) {
      addLog("error", `File dialog error: ${e}`);
    }
  };

  // ─── Installation ─────────────────────────────────────────────────────

  const install = async (andRun = false) => {
    if (!selectedFile || !selectedDevice) {
      addLog("error", "Please select a file and a device first.");
      return;
    }
    setIsInstalling(true);
    const fileName = getFileName(selectedFile);

    try {
      if (fileType === "apk") {
        addLog("info", `Installing ${fileName} on ${selectedDevice}...`);
        addLog("success", await invoke<string>("install_apk", {
          adbPath, device: selectedDevice, apkPath: selectedFile,
        }));
      } else if (fileType === "aab") {
        if (!javaPath || javaStatus !== "found") {
          addLog("error", "Java is required for AAB installation. Please install a JDK.");
          return;
        }
        if (!bundletoolPath || bundletoolStatus !== "found") {
          addLog("error", "bundletool is required for AAB installation. Download it in the Tools or AAB Settings section.");
          return;
        }
        addLog("info", `Installing ${fileName} via bundletool on ${selectedDevice}...`);
        addLog("success", await invoke<string>("install_aab", {
          adbPath, device: selectedDevice, aabPath: selectedFile, javaPath, bundletoolPath,
          keystorePath: keystorePath || null, keystorePass: keystorePass || null,
          keyAlias: keyAlias || null, keyPass: keyPass || null,
        }));
      }

      if (andRun && packageName) {
        addLog("info", `Launching ${packageName}...`);
        addLog("success", await invoke<string>("launch_app", { adbPath, device: selectedDevice, packageName }));
      } else if (andRun && !packageName) {
        addLog("warning", "Cannot launch — package name not set. Enter it manually and use the Launch button.");
      }
    } catch (e) {
      addLog("error", String(e));
    } finally {
      setIsInstalling(false);
    }
  };

  const launchApp = async () => {
    if (!packageName || !selectedDevice) { addLog("error", "Please enter a package name and select a device."); return; }
    try {
      addLog("info", `Launching ${packageName}...`);
      addLog("success", await invoke<string>("launch_app", { adbPath, device: selectedDevice, packageName }));
    } catch (e) { addLog("error", String(e)); }
  };

  const uninstallApp = async () => {
    if (!packageName || !selectedDevice) { addLog("error", "Please enter a package name and select a device."); return; }
    try {
      addLog("info", `Uninstalling ${packageName}...`);
      addLog("success", await invoke<string>("uninstall_app", { adbPath, device: selectedDevice, packageName }));
    } catch (e) { addLog("error", String(e)); }
  };

  // ─── Derived state ────────────────────────────────────────────────────

  const selectedDeviceInfo = devices.find((d) => d.serial === selectedDevice);
  const canInstall = selectedFile && selectedDevice && !isInstalling && adbStatus === "found";
  const adbManaged = toolsStatus?.adb_installed ?? false;
  const javaManaged = toolsStatus?.java_installed ?? false;

  // ─── Render ───────────────────────────────────────────────────────────

  return (
    <div className="app">
      {/* Header */}
      <header className="header">
        <div className="header-title">
          <MonitorSmartphone size={28} className="header-icon" />
          <h1>Android Application Installer</h1>
        </div>
        <p className="header-subtitle">Install APK & AAB files onto connected Android devices</p>
      </header>

      {/* Stale tools banner */}
      <StaleBanner staleTools={staleTools} dismissed={staleDismissed} onDismiss={() => setStaleDismissed(true)} />

      {/* Required tools */}
      <ToolsSection
        toolsStatus={toolsStatus}
        downloadingAdb={downloadingAdb}
        downloadingBundletool={downloadingBundletool}
        downloadingJava={downloadingJava}
        adbProgress={adbProgress}
        btProgress={btProgress}
        javaProgress={javaProgress}
        onSetupAdb={setupAdb}
        onSetupBundletool={setupBundletool}
        onSetupJava={setupJava}
      />

      {/* Device section */}
      <section className="section">
        <div className="section-header"><Settings size={16} /><span>Device</span></div>

        <div className="adb-row">
          <label className="field-label">ADB Path</label>
          <div className="input-group">
            <input type="text" className="input" value={adbPath}
              onChange={(e) => { setAdbPath(e.target.value); setAdbStatus(e.target.value ? "found" : "not-found"); }}
              placeholder={adbManaged ? "Managed by app — auto-detected" : "Path to adb binary..."} />
            <StatusDot status={adbStatus} />
            <button className="btn btn-icon" onClick={detectAdb} title="Auto-detect ADB"><Search size={16} /></button>
          </div>
        </div>

        <div className="device-row">
          <label className="field-label"><Smartphone size={14} /> Connected Device</label>
          <div className="input-group">
            <select className="select" value={selectedDevice} onChange={(e) => setSelectedDevice(e.target.value)} disabled={devices.length === 0}>
              {devices.length === 0 && <option value="">No devices connected</option>}
              {devices.map((d) => (
                <option key={d.serial} value={d.serial}>
                  {d.model ? `${d.model} (${d.serial})` : d.serial}
                  {d.state !== "device" ? ` — ${d.state}` : ""}
                </option>
              ))}
            </select>
            <button className="btn btn-icon" onClick={refreshDevices} disabled={loadingDevices || !adbPath} title="Refresh devices">
              <RefreshCw size={16} className={loadingDevices ? "spin" : ""} />
            </button>
          </div>
          {selectedDeviceInfo?.state === "unauthorized" && (
            <p className="hint hint-warning"><AlertTriangle size={12} /> Accept the USB debugging prompt on your device.</p>
          )}
        </div>
      </section>

      {/* File selection */}
      <section className="section">
        <div className="section-header"><Package size={16} /><span>Package</span></div>
        <div className={`drop-zone ${selectedFile ? "has-file" : ""}`} onClick={browseFile}>
          {selectedFile ? (
            <div className="file-info">
              <div className="file-icon">{fileType === "apk" ? <Package size={32} /> : <FolderOpen size={32} />}</div>
              <div className="file-details">
                <span className="file-name">{getFileName(selectedFile)}</span>
                <span className="file-type">{fileType?.toUpperCase()} File</span>
                <span className="file-path">{selectedFile}</span>
              </div>
              <button className="btn btn-icon btn-ghost" onClick={(e) => { e.stopPropagation(); setSelectedFile(null); setFileType(null); setPackageName(""); }} title="Clear selection">
                <X size={16} />
              </button>
            </div>
          ) : (
            <div className="drop-zone-content">
              <FolderOpen size={40} className="drop-icon" />
              <p className="drop-text">Click to select an APK or AAB file</p>
              <p className="drop-hint">Supports .apk and .aab files</p>
            </div>
          )}
        </div>
        <div className="package-row">
          <label className="field-label">Package Name (for Launch / Uninstall)</label>
          <input type="text" className="input" value={packageName} onChange={(e) => setPackageName(e.target.value)} placeholder="com.example.myapp" />
        </div>
      </section>

      {/* Action buttons */}
      <section className="actions">
        <button className="btn btn-primary" disabled={!canInstall} onClick={() => install(false)}>
          {isInstalling ? <Loader2 size={16} className="spin" /> : <Download size={16} />}
          {isInstalling ? "Installing..." : "Install"}
        </button>
        <button className="btn btn-accent" disabled={!canInstall} onClick={() => install(true)}><Play size={16} /> Install & Run</button>
        <button className="btn btn-secondary" disabled={!packageName || !selectedDevice || isInstalling} onClick={launchApp}><Rocket size={16} /> Launch</button>
        <button className="btn btn-danger" disabled={!packageName || !selectedDevice || isInstalling} onClick={uninstallApp}><Trash2 size={16} /> Uninstall</button>
      </section>

      {/* AAB settings (collapsible) */}
      <section className="section collapsible">
        <button className="section-header clickable" onClick={() => setShowAabSettings(!showAabSettings)}>
          {showAabSettings ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
          <Coffee size={16} /><span>AAB Settings</span>
          <span className="section-hint">(Java, bundletool, keystore — required for .aab files)</span>
        </button>
        {showAabSettings && (
          <div className="collapsible-content">
            <div className="setting-row">
              <label className="field-label">Java</label>
              <div className="input-group">
                <input type="text" className="input" value={javaPath} onChange={(e) => setJavaPath(e.target.value)} placeholder={javaManaged ? "Managed by app — auto-detected" : "java"} />
                <StatusDot status={javaStatus} />
                <button className="btn btn-icon" onClick={checkJava} title="Detect Java"><Search size={16} /></button>
                {javaStatus === "not-found" && (
                  <button className="btn btn-small" onClick={setupJava} disabled={downloadingJava} title="Download Java JRE">
                    {downloadingJava ? <Loader2 size={14} className="spin" /> : <Download size={14} />} Download
                  </button>
                )}
              </div>
              {javaVersion && <p className="hint hint-success"><Check size={12} /> {javaVersion}</p>}
            </div>
            <div className="setting-row">
              <label className="field-label">bundletool.jar</label>
              <div className="input-group">
                <input type="text" className="input" value={bundletoolPath} onChange={(e) => { setBundletoolPath(e.target.value); setBundletoolStatus(e.target.value ? "found" : "not-found"); }} placeholder="Path to bundletool.jar..." />
                <StatusDot status={bundletoolStatus} />
                <button className="btn btn-icon" onClick={detectBundletool} title="Detect bundletool"><Search size={16} /></button>
                <button className="btn btn-small" onClick={setupBundletool} disabled={downloadingBundletool} title="Download latest from GitHub">
                  {downloadingBundletool ? <Loader2 size={14} className="spin" /> : <Download size={14} />} Download
                </button>
              </div>
            </div>
            <div className="setting-row">
              <label className="field-label">Keystore (optional)</label>
              <div className="input-group">
                <input type="text" className="input" value={keystorePath} onChange={(e) => setKeystorePath(e.target.value)} placeholder="Path to .jks / .keystore (leave empty for debug key)" />
                <button className="btn btn-icon" onClick={browseKeystore} title="Browse"><FolderOpen size={16} /></button>
              </div>
            </div>
            {keystorePath && (
              <>
                <div className="setting-row indent">
                  <label className="field-label">Keystore Password</label>
                  <input type="password" className="input" value={keystorePass} onChange={(e) => setKeystorePass(e.target.value)} placeholder="Keystore password" />
                </div>
                <div className="setting-row indent">
                  <label className="field-label">Key Alias</label>
                  <input type="text" className="input" value={keyAlias} onChange={(e) => setKeyAlias(e.target.value)} placeholder="Key alias" />
                </div>
                <div className="setting-row indent">
                  <label className="field-label">Key Password</label>
                  <input type="password" className="input" value={keyPass} onChange={(e) => setKeyPass(e.target.value)} placeholder="Key password" />
                </div>
              </>
            )}
          </div>
        )}
      </section>

      {/* Log panel */}
      <LogPanel logs={logs} onClear={() => setLogs([])} />
    </div>
  );
}

export default App;

