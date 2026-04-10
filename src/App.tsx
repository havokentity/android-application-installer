import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { getVersion } from "@tauri-apps/api/app";
  import {
  Smartphone, RefreshCw, FolderOpen, Download, Play, Rocket,
  Check, X, AlertTriangle, Search, Settings, Loader2, Package,
  MonitorSmartphone, Coffee, ChevronDown, ChevronRight, Trash2, Key, Clock,
  Monitor, Columns2, Sun, Moon,
} from "lucide-react";

import "./App.css";
import type {
  DeviceInfo, LogEntry, ToolsStatus, DownloadProgress,
  StaleTool, DetectionStatus, RecentFilesConfig,
} from "./types";
import { nextLogId, getFileName, getFileType, now, shortcutLabel } from "./helpers";
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
  const [keyAliases, setKeyAliases] = useState<string[]>([]);
  const [loadingAliases, setLoadingAliases] = useState(false);

  // ── UI collapse state ──────────────────────────────────────────────────
  const [deviceExpanded, setDeviceExpanded] = useState(true);

  // ── General state ─────────────────────────────────────────────────────
  const [isInstalling, setIsInstalling] = useState(false);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [recentFiles, setRecentFiles] = useState<RecentFilesConfig>({ packages: [], keystores: [] });

  // ── Layout & Theme ────────────────────────────────────────────────────
  const DEFAULT_SIDE_WIDTH = 340;

  const [layout, setLayout] = useState<"portrait" | "landscape">(() => {
    return (localStorage.getItem("layout") as "portrait" | "landscape") || "landscape";
  });
  const [sidePanelWidth, setSidePanelWidth] = useState<number>(() => {
    const saved = localStorage.getItem("landscapeWidth");
    return saved ? Number(saved) : DEFAULT_SIDE_WIDTH;
  });
  const [theme, setTheme] = useState<"dark" | "light">(() => {
    return (localStorage.getItem("theme") as "dark" | "light") || "dark";
  });

  // ── New feature state ─────────────────────────────────────────────────
  const [isDragOver, setIsDragOver] = useState(false);
  const [appVersion, setAppVersion] = useState("");
  const [installAllDevices, setInstallAllDevices] = useState(false);
  const [easterEggVisible, setEasterEggVisible] = useState(false);
  const [easterEggIndex, setEasterEggIndex] = useState(0);
  const easterEggClicks = useRef(0);
  const easterEggTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const prevDeviceSerials = useRef("");
  const handleFileSelectedRef = useRef<((path: string) => Promise<void>) | undefined>(undefined);
  const shortcutRefs = useRef({
    browseFile: () => {}, install: (_andRun: boolean) => {},
    launchApp: () => {}, uninstallApp: () => {},
    canInstall: false as boolean | string | null,
    canLaunch: false, canUninstall: false,
  });

  // Apply correct window size on first mount based on saved layout
  useEffect(() => {
    const win = getCurrentWindow();
    (async () => {
      if (layout === "landscape") {
        await win.setMinSize(new LogicalSize(1080, 520));
        await win.setSize(new LogicalSize(1280, 720));
      } else {
        await win.setMinSize(new LogicalSize(680, 520));
        await win.setSize(new LogicalSize(920, 740));
      }
      await win.center();
    })();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem("theme", theme);
  }, [theme]);

  // ── Fetch app version ──────────────────────────────────────────────────
  useEffect(() => {
    getVersion().then(setAppVersion).catch(() => {});
  }, []);

  const toggleLayout = useCallback(async (mode: "portrait" | "landscape") => {
    const win = getCurrentWindow();
    if (mode === "landscape") {
      await win.setMinSize(new LogicalSize(1080, 520));
      await win.setSize(new LogicalSize(1280, 720));
      // Switching to landscape always resets to defaults
      setSidePanelWidth(DEFAULT_SIDE_WIDTH);
      localStorage.setItem("landscapeWidth", String(DEFAULT_SIDE_WIDTH));
    } else {
      await win.setSize(new LogicalSize(920, 740));
      await win.setMinSize(new LogicalSize(680, 520));
      // Switching to portrait clears saved landscape width
      localStorage.removeItem("landscapeWidth");
    }
    await win.center();
    setLayout(mode);
    localStorage.setItem("layout", mode);
  }, []);

  // ── Draggable divider ─────────────────────────────────────────────────
  const dragging = useRef(false);
  const appRef = useRef<HTMLDivElement>(null);

  const onDividerMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    dragging.current = true;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";

    const onMouseMove = (ev: MouseEvent) => {
      if (!dragging.current || !appRef.current) return;
      const appRect = appRef.current.getBoundingClientRect();
      const newSideWidth = appRect.right - ev.clientX - 12; // 12 = half gap + divider
      const clamped = Math.max(240, Math.min(newSideWidth, appRect.width - 400));
      setSidePanelWidth(clamped);
    };

    const onMouseUp = () => {
      dragging.current = false;
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
      // Save final width
      setSidePanelWidth((w) => {
        localStorage.setItem("landscapeWidth", String(w));
        return w;
      });
    };

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  }, []);

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

  // Keep prevDeviceSerials in sync with manual refreshes too
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

  // Auto-collapse device section when a device is selected, expand when none
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

      // Try to extract package name from AAB if Java + bundletool are available
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

  // Keep ref to latest handleFileSelected for drag-drop effect
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
      if (aliases.length === 1) {
        setKeyAlias(aliases[0]);
      }
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
    if (!selectedFile) {
      addLog("error", "Please select a file first.");
      return;
    }

    const targetDevices = installAllDevices && devices.length > 1
      ? devices.filter(d => d.state === "device").map(d => d.serial)
      : selectedDevice ? [selectedDevice] : [];

    if (targetDevices.length === 0) {
      addLog("error", "Please select a device first.");
      return;
    }

    // Check AAB prerequisites before starting
    if (fileType === "aab") {
      if (!javaPath || javaStatus !== "found") {
        addLog("error", "Java is required for AAB installation. Please install a JDK.");
        return;
      }
      if (!bundletoolPath || bundletoolStatus !== "found") {
        addLog("error", "bundletool is required for AAB installation. Download it in the Tools or AAB Settings section.");
        return;
      }
    }

    setIsInstalling(true);
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
            addLog("success", prefix + await invoke<string>("install_apk", {
              adbPath, device, apkPath: selectedFile,
            }));
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
          addLog("error", `${prefix}${String(e)}`);
        }
      }
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
  const canInstall = selectedFile &&
    (selectedDevice || (installAllDevices && devices.length > 0)) &&
    !isInstalling && adbStatus === "found";
  const adbManaged = toolsStatus?.adb_installed ?? false;
  const javaManaged = toolsStatus?.java_installed ?? false;
  const toolsMissing = toolsStatus !== null && (!toolsStatus.adb_installed || !toolsStatus.bundletool_installed || !toolsStatus.java_installed);

  // ── Window title with current file ─────────────────────────────────────
  useEffect(() => {
    const win = getCurrentWindow();
    const base = "Android Application Installer";
    win.setTitle(selectedFile ? `${base} — ${getFileName(selectedFile)}` : base);
  }, [selectedFile]);

  // ── Keyboard shortcuts (Cmd/Ctrl + O, I, L, U) ─────────────────────────
  const canLaunchOrUninstall = !!packageName && !!selectedDevice && !isInstalling;
  shortcutRefs.current = {
    browseFile, install, launchApp, uninstallApp,
    canInstall, canLaunch: canLaunchOrUninstall, canUninstall: canLaunchOrUninstall,
  };

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (!mod) return;
      const key = e.key.toLowerCase();
      if (key === "o") {
        e.preventDefault();
        shortcutRefs.current.browseFile();
      } else if (key === "i" && e.shiftKey) {
        e.preventDefault();
        if (shortcutRefs.current.canInstall) shortcutRefs.current.install(true);
      } else if (key === "i") {
        e.preventDefault();
        if (shortcutRefs.current.canInstall) shortcutRefs.current.install(false);
      } else if (key === "l") {
        e.preventDefault();
        if (shortcutRefs.current.canLaunch) shortcutRefs.current.launchApp();
      } else if (key === "u") {
        e.preventDefault();
        if (shortcutRefs.current.canUninstall) shortcutRefs.current.uninstallApp();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  // ── Easter egg ─────────────────────────────────────────────────────────
  const easterEggVerses = [
    { text: "Kiss the Son, lest he be angry, and ye perish from the way, when his wrath is kindled but a little. Blessed are all they that put their trust in him.", ref: "Psalm 2:12" },
    { text: "Who hath ascended up into heaven, or descended? who hath gathered the wind in his fists? who hath bound the waters in a garment? who hath established all the ends of the earth? what is his name, and what is his son\u2019s name, if thou canst tell?", ref: "Proverbs 30:4" },
  ];

  const handleTitleClick = useCallback(() => {
    easterEggClicks.current += 1;
    if (easterEggTimer.current) clearTimeout(easterEggTimer.current);

    if (easterEggClicks.current >= 7) {
      easterEggClicks.current = 0;
      setEasterEggIndex(prev => prev);  // capture current
      setEasterEggVisible(true);
      setTimeout(() => {
        setEasterEggVisible(false);
        setEasterEggIndex(prev => (prev + 1) % 2);
      }, 6500);
    } else {
      easterEggTimer.current = setTimeout(() => {
        easterEggClicks.current = 0;
      }, 2000);
    }
  }, []);

  // ─── Render ───────────────────────────────────────────────────────────

  // ─── Shared UI blocks ─────────────────────────────────────────────────

  const toolbarEl = (
    <div className="toolbar">
      <div className="toolbar-group">
        <button className={`toolbar-btn ${layout === "portrait" ? "active" : ""}`} onClick={() => toggleLayout("portrait")} title="Portrait layout">
          <Monitor size={13} /> Portrait
        </button>
        <button className={`toolbar-btn ${layout === "landscape" ? "active" : ""}`} onClick={() => toggleLayout("landscape")} title="Landscape layout">
          <Columns2 size={13} /> Landscape
        </button>
      </div>
      <div className="toolbar-group">
        <button className={`toolbar-btn ${theme === "light" ? "active" : ""}`} onClick={() => setTheme("light")} title="Light theme">
          <Sun size={13} />
        </button>
        <button className={`toolbar-btn ${theme === "dark" ? "active" : ""}`} onClick={() => setTheme("dark")} title="Dark theme">
          <Moon size={13} />
        </button>
      </div>
    </div>
  );

  const headerEl = (
    <header className="header">
      <div className="header-title">
        <MonitorSmartphone size={28} className="header-icon" />
        <h1 onClick={handleTitleClick}>Android Application Installer</h1>
      </div>
      <p className="header-subtitle">
        Install APK & AAB files onto connected Android devices
        {appVersion && <span className="version-badge">v{appVersion}</span>}
      </p>
    </header>
  );

  const staleBannerEl = (
    <StaleBanner staleTools={staleTools} dismissed={staleDismissed} onDismiss={() => setStaleDismissed(true)} />
  );

  const toolsSectionEl = (
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
      needsAttention={toolsMissing}
      compact={layout === "landscape"}
      collapsible
    />
  );

  const deviceConnected = selectedDevice && devices.length > 0;
  const deviceLabel = deviceConnected
    ? (selectedDeviceInfo?.model || selectedDevice)
    : null;

  const deviceSectionEl = (
    <section className={`section collapsible ${!deviceConnected ? "device-attention" : ""}`}>
      <div className="section-header clickable device-header" onClick={() => setDeviceExpanded(!deviceExpanded)}>
        <div className="device-header-left">
          {deviceExpanded ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
          <Settings size={16} /><span>Device</span>
          {deviceConnected && <span className="tool-badge badge-green">{deviceLabel}</span>}
          {!deviceConnected && <span className="tool-badge badge-yellow">No device</span>}
        </div>
        <div className="device-actions" onClick={(e) => e.stopPropagation()}>
          <button className="btn btn-primary btn-small" disabled={!canInstall} onClick={() => install(false)} title={`Install (${shortcutLabel("I")})`}>
            {isInstalling ? <Loader2 size={14} className="spin" /> : <Download size={14} />}
            {isInstalling ? "Installing..." : "Install"}
          </button>
          <button className="btn btn-accent btn-small" disabled={!canInstall} onClick={() => install(true)} title={`Install & Run (${shortcutLabel("I", true)})`}><Play size={14} /> Install & Run</button>
          <button className="btn btn-secondary btn-small" disabled={!packageName || !selectedDevice || isInstalling} onClick={launchApp} title={`Launch (${shortcutLabel("L")})`}><Rocket size={14} /> Launch</button>
          <button className="btn btn-danger btn-small" disabled={!packageName || !selectedDevice || isInstalling} onClick={uninstallApp} title={`Uninstall (${shortcutLabel("U")})`}><Trash2 size={14} /> Uninstall</button>
        </div>
      </div>
      {deviceExpanded && (
        <div className="collapsible-content">
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
            {devices.length > 1 && (
              <div className="multi-device-row">
                <label className="multi-device-label">
                  <input
                    type="checkbox"
                    checked={installAllDevices}
                    onChange={(e) => setInstallAllDevices(e.target.checked)}
                  />
                  Install to all {devices.filter(d => d.state === "device").length} connected devices
                </label>
              </div>
            )}
          </div>
        </div>
      )}
    </section>
  );

  const fileSectionEl = (
    <section className="section">
      <div className="section-header"><Package size={16} /><span>Package</span></div>
      <div className={`drop-zone ${selectedFile ? "has-file" : ""} ${isDragOver ? "drag-over" : ""}`} onClick={browseFile}>
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
            <p className="drop-text">{isDragOver ? "Drop to select file" : "Click or drop an APK or AAB file"}</p>
            {!isDragOver && <p className="drop-hint">Supports .apk and .aab files — {shortcutLabel("O")} to browse</p>}
          </div>
        )}
      </div>
      {!selectedFile && recentFiles.packages.length > 0 && (
        <div className="recent-list">
          <div className="recent-header"><Clock size={12} /> Recent Packages</div>
          {recentFiles.packages.map((f) => (
            <div key={f.path} className="recent-item" onClick={() => handleFileSelected(f.path)} title={f.path}>
              <Package size={14} className="recent-icon" />
              <span className="recent-name">{f.name}</span>
              <span className="recent-path">{f.path}</span>
              <button className="btn btn-icon btn-ghost recent-remove" onClick={(e) => { e.stopPropagation(); removeRecentFile(f.path, "packages"); }} title="Remove">
                <X size={12} />
              </button>
            </div>
          ))}
        </div>
      )}
      <div className="package-row">
        <label className="field-label">Package Name (for Launch / Uninstall)</label>
        <input type="text" className="input" value={packageName} onChange={(e) => setPackageName(e.target.value)} placeholder="com.example.myapp" />
      </div>
    </section>
  );

  const aabSettingsEl = (
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
            {!keystorePath && recentFiles.keystores.length > 0 && (
              <div className="recent-list recent-list-compact">
                <div className="recent-header"><Clock size={12} /> Recent Keystores</div>
                {recentFiles.keystores.map((f) => (
                  <div key={f.path} className="recent-item" onClick={() => { setKeystorePath(f.path); setKeyAlias(""); setKeyAliases([]); recordRecentFile(f.path, "keystores"); }} title={f.path}>
                    <Key size={14} className="recent-icon" />
                    <span className="recent-name">{f.name}</span>
                    <span className="recent-path">{f.path}</span>
                    <button className="btn btn-icon btn-ghost recent-remove" onClick={(e) => { e.stopPropagation(); removeRecentFile(f.path, "keystores"); }} title="Remove">
                      <X size={12} />
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>
          {keystorePath && (
            <>
              <div className="setting-row indent">
                <label className="field-label">Keystore Password</label>
                <input type="password" className="input" value={keystorePass} onChange={(e) => setKeystorePass(e.target.value)} placeholder="Keystore password" />
              </div>
              <div className="setting-row indent">
                <label className="field-label"><Key size={14} /> Key Alias</label>
                <div className="input-group">
                  {keyAliases.length > 0 ? (
                    <select className="select" value={keyAlias} onChange={(e) => setKeyAlias(e.target.value)}>
                      <option value="">— Select alias —</option>
                      {keyAliases.map((a) => (
                        <option key={a} value={a}>{a}</option>
                      ))}
                    </select>
                  ) : (
                    <input type="text" className="input" value={keyAlias} onChange={(e) => setKeyAlias(e.target.value)} placeholder={loadingAliases ? "Loading aliases..." : "Key alias (enter password to list)"} />
                  )}
                  {loadingAliases && <Loader2 size={14} className="spin" />}
                  {keystorePass && !loadingAliases && (
                    <button className="btn btn-icon" onClick={() => fetchKeyAliases(keystorePath, keystorePass)} title="Refresh aliases">
                      <RefreshCw size={14} />
                    </button>
                  )}
                </div>
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
  );

  const logPanelEl = <LogPanel logs={logs} onClear={() => setLogs([])} />;

  // ─── Render ───────────────────────────────────────────────────────────

  if (layout === "landscape") {
    return (
      <div
        className="app landscape"
        ref={appRef}
        style={{ gridTemplateColumns: `1fr auto ${sidePanelWidth}px` }}
      >
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
        {easterEggVisible && (
          <div className="easter-egg-overlay">
            <p className="easter-egg-text">
              &ldquo;{easterEggVerses[easterEggIndex].text}&rdquo;
            </p>
            <p className="easter-egg-ref">— {easterEggVerses[easterEggIndex].ref}</p>
          </div>
        )}
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
      {easterEggVisible && (
        <div className="easter-egg-overlay">
          <p className="easter-egg-text">
            &ldquo;{easterEggVerses[easterEggIndex].text}&rdquo;
          </p>
          <p className="easter-egg-ref">— {easterEggVerses[easterEggIndex].ref}</p>
        </div>
      )}
    </div>
  );
}

export default App;

