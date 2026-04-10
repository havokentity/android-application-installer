// ─── Tools State Hook ─────────────────────────────────────────────────────────
import { useState, useCallback, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import type { LogEntry, ToolsStatus, DownloadProgress, StaleTool } from "../types";
import * as api from "../api";

interface ToolSetupCallbacks {
  onAdbSetup?: (path: string) => void;
  onJavaSetup?: (path: string) => void;
  onBundletoolSetup?: () => void;
}

export function useToolsState(
  addLog: (level: LogEntry["level"], message: string) => void,
  callbacks: ToolSetupCallbacks = {},
) {
  const [toolsStatus, setToolsStatus] = useState<ToolsStatus | null>(null);
  const [downloadingAdb, setDownloadingAdb] = useState(false);
  const [downloadingBundletool, setDownloadingBundletool] = useState(false);
  const [downloadingJava, setDownloadingJava] = useState(false);
  const [adbProgress, setAdbProgress] = useState<DownloadProgress | null>(null);
  const [btProgress, setBtProgress] = useState<DownloadProgress | null>(null);
  const [javaProgress, setJavaProgress] = useState<DownloadProgress | null>(null);
  const [staleTools, setStaleTools] = useState<StaleTool[]>([]);
  const [staleDismissed, setStaleDismissed] = useState(false);

  // ── Download progress listener ──────────────────────────────────────
  useEffect(() => {
    const unlisten = listen<DownloadProgress>("download-progress", (event) => {
      const p = event.payload;
      if (p.tool === "platform-tools") setAdbProgress(p);
      else if (p.tool === "bundletool") setBtProgress(p);
      else if (p.tool === "java") setJavaProgress(p);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  // ── Status checks ──────────────────────────────────────────────────
  const checkToolsStatus = useCallback(async () => {
    try {
      setToolsStatus(await api.getToolsStatus());
    } catch (e) {
      addLog("warning", `Could not check tools status: ${e}`);
    }
  }, [addLog]);

  const checkStaleTools = useCallback(async () => {
    try {
      const stale = await api.checkForStaleTools();
      setStaleTools(stale);
      if (stale.length > 0) {
        const names = stale.map((s) => `${s.label} (${s.age_days}d ago)`).join(", ");
        addLog("warning", `Some managed tools haven't been updated in 30+ days: ${names}`);
      }
    } catch { /* non-critical */ }
  }, [addLog]);

  useEffect(() => { checkToolsStatus(); }, [checkToolsStatus]);
  useEffect(() => { checkStaleTools(); }, [checkStaleTools]);

  // ── Setup actions ──────────────────────────────────────────────────
  const setupAdb = async () => {
    setDownloadingAdb(true);
    setAdbProgress(null);
    addLog("info", "Downloading ADB platform-tools from Google...");
    try {
      const path = await api.setupPlatformTools();
      addLog("success", `ADB installed: ${path}`);
      await checkToolsStatus();
      await checkStaleTools();
      callbacks.onAdbSetup?.(path);
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
      addLog("success", await api.setupBundletool());
      await checkToolsStatus();
      await checkStaleTools();
      callbacks.onBundletoolSetup?.();
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
      const path = await api.setupJava();
      addLog("success", `Java JRE installed: ${path}`);
      await checkToolsStatus();
      await checkStaleTools();
      callbacks.onJavaSetup?.(path);
    } catch (e) {
      addLog("error", `Java setup failed: ${e}`);
    } finally {
      setDownloadingJava(false);
      setJavaProgress(null);
    }
  };

  return {
    toolsStatus,
    downloadingAdb, downloadingBundletool, downloadingJava,
    adbProgress, btProgress, javaProgress,
    staleTools, staleDismissed, setStaleDismissed,
    checkToolsStatus, checkStaleTools,
    setupAdb, setupBundletool, setupJava,
  };
}

