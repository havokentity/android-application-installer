// ─── File State Hook ──────────────────────────────────────────────────────────
import { useState, useEffect, useRef, useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { LogEntry, PackageMetadata } from "../types";
import { getFileName, getFileType } from "../helpers";
import * as api from "../api";

interface UseFileStateOptions {
  addLog: (level: LogEntry["level"], message: string) => void;
  recordRecentFile: (path: string, category: "packages" | "keystores") => void;
  onAabSelected?: (path: string) => Promise<{ javaPath: string; bundletoolPath: string } | null | void>;
  getAabToolPaths?: () => { javaPath: string; bundletoolPath: string } | null;
  /** Called when a file has an associated signing profile that should be auto-restored. */
  onAutoProfileRestore?: (profileName: string) => void;
}

export function useFileState({ addLog, recordRecentFile, onAabSelected, getAabToolPaths, onAutoProfileRestore }: UseFileStateOptions) {
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [selectedFiles, setSelectedFiles] = useState<string[]>([]);
  const [fileType, setFileType] = useState<"apk" | "aab" | null>(null);
  const [fileSize, setFileSize] = useState<number | null>(null);
  const [packageName, setPackageName] = useState("");
  const [metadata, setMetadata] = useState<PackageMetadata | null>(null);
  const [isDragOver, setIsDragOver] = useState(false);
  const [isDragRejected, setIsDragRejected] = useState(false);
  const handleFileSelectedRef = useRef<((path: string) => Promise<void>) | undefined>(undefined);

  // ── File selection ──────────────────────────────────────────────────
  const handleFileSelected = useCallback(async (path: string) => {
    const ft = getFileType(path);
    if (!ft) { addLog("error", "Please select an APK or AAB file."); return; }

    setSelectedFile(path);
    setSelectedFiles([path]);
    setFileType(ft);
    addLog("info", `Selected: ${getFileName(path)} (${ft.toUpperCase()})`);
    recordRecentFile(path, "packages");

    // Fetch file size
    try {
      const size = await api.getFileSize(path);
      setFileSize(size);
    } catch (e) {
      console.warn("Failed to get file size:", e);
      setFileSize(null);
    }

    if (ft === "apk") {
      try {
        const pkg = await api.getPackageName(path);
        setPackageName(pkg);
        addLog("info", `Package: ${pkg}`);
      } catch (e) {
        console.warn("Failed to get package name:", e);
        addLog("info", "Could not auto-detect package name. You can enter it manually for the Launch feature.");
      }
      // Fetch APK metadata
      try {
        const meta = await api.getApkMetadata(path);
        setMetadata(meta);
        if (meta.versionName) addLog("info", `Version: ${meta.versionName} (code ${meta.versionCode ?? "?"})`);
      } catch (e) { console.warn("Failed to get APK metadata:", e); }
    }

    if (ft === "aab") {
      const returnedTools = await onAabSelected?.(path);
      // Use tool paths returned directly from onAabSelected (avoids stale React state on first selection),
      // falling back to getAabToolPaths for cases where onAabSelected doesn't return them.
      const tools = (returnedTools && 'javaPath' in returnedTools ? returnedTools : null) ?? getAabToolPaths?.();
      // Fetch AAB metadata (needs java + bundletool)
      try {
        if (tools) {
          const meta = await api.getAabMetadata(path, tools.javaPath, tools.bundletoolPath);
          setMetadata(meta);
          if (meta.packageName) { setPackageName(meta.packageName); addLog("info", `Package: ${meta.packageName}`); }
          if (meta.versionName) addLog("info", `Version: ${meta.versionName} (code ${meta.versionCode ?? "?"})`);
        }
      } catch (e) { console.warn("Failed to get AAB metadata:", e); }
    }

    // Auto-restore signing profile associated with this file
    if (onAutoProfileRestore) {
      try {
        const profileName = await api.getProfileForFile(path);
        if (profileName) {
          onAutoProfileRestore(profileName);
          addLog("info", `Auto-restored signing profile: ${profileName}`);
        }
      } catch (e) { console.warn("Failed to get profile for file:", e); }
    }
  }, [addLog, recordRecentFile, onAabSelected, onAutoProfileRestore]);

  handleFileSelectedRef.current = handleFileSelected;

  /** Handle multiple files being selected (batch mode). */
  const handleBatchFilesSelected = useCallback(async (paths: string[]) => {
    const valid = paths.filter((p) => getFileType(p));
    if (valid.length === 0) {
      addLog("error", "No valid APK or AAB files found in selection.");
      return;
    }
    if (valid.length === 1) {
      return handleFileSelected(valid[0]);
    }

    // Batch mode: set the list but only fully process the first file
    setSelectedFiles(valid);
    addLog("info", `Selected ${valid.length} files for batch install`);

    // Process the first file for metadata display
    const first = valid[0];
    const ft = getFileType(first);
    setSelectedFile(first);
    setFileType(ft);
    setMetadata(null);

    try {
      const size = await api.getFileSize(first);
      setFileSize(size);
    } catch (e) {
      setFileSize(null);
    }

    if (ft === "apk") {
      try {
        const pkg = await api.getPackageName(first);
        setPackageName(pkg);
      } catch (e) { console.warn("Failed to get package name:", e); }
    }

    for (const p of valid) {
      recordRecentFile(p, "packages");
    }
  }, [addLog, handleFileSelected, recordRecentFile]);

  const browseFile = useCallback(async () => {
    try {
      const file = await open({
        title: "Select APK or AAB file(s)",
        multiple: true,
        filters: [
          { name: "Android Package", extensions: ["apk", "aab"] },
          { name: "APK Files", extensions: ["apk"] },
          { name: "AAB Files", extensions: ["aab"] },
        ],
      });
      if (file) {
        const paths = Array.isArray(file) ? file : [file];
        if (paths.length === 1) {
          handleFileSelected(paths[0] as string);
        } else if (paths.length > 1) {
          handleBatchFilesSelected(paths as string[]);
        }
      }
    } catch (e) {
      addLog("error", `File dialog error: ${e}`);
    }
  }, [handleFileSelected, handleBatchFilesSelected, addLog]);

  const clearFile = useCallback(() => {
    setSelectedFile(null);
    setSelectedFiles([]);
    setFileType(null);
    setFileSize(null);
    setMetadata(null);
    setPackageName("");
  }, []);

  // ── Drag & drop ───────────────────────────────────────────────────
  useEffect(() => {
    const win = getCurrentWindow();
    const unlisten = win.onDragDropEvent((event) => {
      if (event.payload.type === "enter") {
        setIsDragOver(true);
        // Check if the dragged file is supported
        const paths = (event.payload as any).paths;
        if (paths && paths.length > 0 && !paths.some((p: string) => getFileType(p))) {
          setIsDragRejected(true);
        } else {
          setIsDragRejected(false);
        }
      } else if (event.payload.type === "leave") {
        setIsDragOver(false);
        setIsDragRejected(false);
      } else if (event.payload.type === "drop") {
        setIsDragOver(false);
        setIsDragRejected(false);
        const paths = event.payload.paths;
        if (paths && paths.length > 0) {
          const valid = paths.filter((p: string) => getFileType(p));
          if (valid.length === 0) {
            addLog("error", "Unsupported file type. Please drop APK or AAB files.");
          } else if (valid.length === 1) {
            handleFileSelectedRef.current?.(valid[0]);
          } else {
            // Batch drop
            handleBatchFilesSelected(valid);
          }
        }
      }
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // ── Window title with current file ─────────────────────────────────
  useEffect(() => {
    const win = getCurrentWindow();
    const base = "Android Application Installer";
    if (selectedFiles.length > 1) {
      win.setTitle(`${base} — ${selectedFiles.length} files selected`);
    } else {
      win.setTitle(selectedFile ? `${base} — ${getFileName(selectedFile)}` : base);
    }
  }, [selectedFile, selectedFiles]);

  return {
    selectedFile, selectedFiles, fileType, fileSize, packageName, setPackageName, metadata,
    isDragOver, isDragRejected, browseFile, handleFileSelected, handleBatchFilesSelected, clearFile,
  };
}

