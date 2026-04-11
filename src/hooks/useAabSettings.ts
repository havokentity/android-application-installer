// ─── AAB Settings Hook ────────────────────────────────────────────────────────
import { useState, useCallback, useEffect } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { LogEntry, DetectionStatus } from "../types";
import * as api from "../api";

interface UseAabSettingsOptions {
  addLog: (level: LogEntry["level"], message: string) => void;
  recordRecentFile: (path: string, category: "packages" | "keystores") => void;
}

export function useAabSettings({ addLog, recordRecentFile }: UseAabSettingsOptions) {
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

  // ── Java detection ──────────────────────────────────────────────────
  const checkJava = useCallback(async () => {
    try {
      const result = await api.checkJava();
      const [path, version] = result.split("|", 2);
      setJavaPath(path);
      setJavaVersion(version);
      setJavaStatus("found");
      addLog("success", `Java found: ${version}`);
    } catch (e) {
      setJavaStatus("not-found");
      addLog("warning", String(e));
    }
  }, [addLog]);

  // ── Bundletool detection ────────────────────────────────────────────
  const detectBundletool = useCallback(async () => {
    try {
      const path = await api.findBundletool();
      setBundletoolPath(path);
      setBundletoolStatus("found");
      addLog("success", `bundletool found: ${path}`);
    } catch (e) {
      console.warn("bundletool detection failed:", e);
      setBundletoolStatus("not-found");
      addLog("info", "bundletool not found — use the Download button in AAB Settings or in the Tools section above.");
    }
  }, [addLog]);

  // ── Keystore browsing ──────────────────────────────────────────────
  const browseKeystore = useCallback(async () => {
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
  }, [addLog, recordRecentFile]);

  // ── Key alias listing ──────────────────────────────────────────────
  const fetchKeyAliases = useCallback(async (ksPath: string, ksPass: string) => {
    if (!ksPath || !ksPass || !javaPath) return;
    setLoadingAliases(true);
    try {
      const aliases = await api.listKeyAliases(javaPath, ksPath, ksPass);
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

  // ── AAB package name detection helper ──────────────────────────────
  const detectAabPackageName = useCallback(async (aabPath: string): Promise<string | null> => {
    try {
      const jp = javaPath || (await api.checkJava()).split("|")[0];
      const bt = bundletoolPath || (await api.findBundletool());
      if (jp && bt) {
        return await api.getAabPackageName(aabPath, jp, bt);
      }
    } catch (e) { console.warn("AAB package name detection failed:", e); }
    return null;
  }, [javaPath, bundletoolPath]);

  return {
    showAabSettings, setShowAabSettings,
    javaPath, setJavaPath, javaVersion, javaStatus, setJavaStatus,
    bundletoolPath, setBundletoolPath, bundletoolStatus, setBundletoolStatus,
    keystorePath, setKeystorePath, keystorePass, setKeystorePass,
    keyAlias, setKeyAlias, keyPass, setKeyPass,
    keyAliases, loadingAliases,
    checkJava, detectBundletool, browseKeystore, fetchKeyAliases,
    detectAabPackageName,
  };
}

