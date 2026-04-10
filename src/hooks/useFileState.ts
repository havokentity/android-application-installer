// ─── File State Hook ──────────────────────────────────────────────────────────
import { useState, useEffect, useRef, useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { LogEntry } from "../types";
import { getFileName, getFileType } from "../helpers";
import * as api from "../api";

interface UseFileStateOptions {
  addLog: (level: LogEntry["level"], message: string) => void;
  recordRecentFile: (path: string, category: "packages" | "keystores") => void;
  onAabSelected?: (path: string) => Promise<void>;
}

export function useFileState({ addLog, recordRecentFile, onAabSelected }: UseFileStateOptions) {
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [fileType, setFileType] = useState<"apk" | "aab" | null>(null);
  const [packageName, setPackageName] = useState("");
  const [isDragOver, setIsDragOver] = useState(false);
  const handleFileSelectedRef = useRef<((path: string) => Promise<void>) | undefined>(undefined);

  // ── File selection ──────────────────────────────────────────────────
  const handleFileSelected = useCallback(async (path: string) => {
    const ft = getFileType(path);
    if (!ft) { addLog("error", "Please select an APK or AAB file."); return; }

    setSelectedFile(path);
    setFileType(ft);
    addLog("info", `Selected: ${getFileName(path)} (${ft.toUpperCase()})`);
    recordRecentFile(path, "packages");

    if (ft === "apk") {
      try {
        const pkg = await api.getPackageName(path);
        setPackageName(pkg);
        addLog("info", `Package: ${pkg}`);
      } catch {
        addLog("info", "Could not auto-detect package name. You can enter it manually for the Launch feature.");
      }
    }

    if (ft === "aab") {
      await onAabSelected?.(path);
    }
  }, [addLog, recordRecentFile, onAabSelected]);

  handleFileSelectedRef.current = handleFileSelected;

  const browseFile = useCallback(async () => {
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
  }, [handleFileSelected, addLog]);

  const clearFile = useCallback(() => {
    setSelectedFile(null);
    setFileType(null);
    setPackageName("");
  }, []);

  // ── Drag & drop ───────────────────────────────────────────────────
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

  // ── Window title with current file ─────────────────────────────────
  useEffect(() => {
    const win = getCurrentWindow();
    const base = "Android Application Installer";
    win.setTitle(selectedFile ? `${base} — ${getFileName(selectedFile)}` : base);
  }, [selectedFile]);

  return {
    selectedFile, fileType, packageName, setPackageName,
    isDragOver, browseFile, handleFileSelected, clearFile,
  };
}


