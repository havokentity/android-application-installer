// ─── Auto-Updater Hook ────────────────────────────────────────────────────────
import { useState, useCallback, useEffect } from "react";
import { check } from "@tauri-apps/plugin-updater";
import { ask } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import type { LogEntry } from "../types";

export interface UpdateProgress {
  downloaded: number;
  total: number;
  percent: number;
}

export function useUpdater(addLog: (level: LogEntry["level"], message: string) => void) {
  const [checkingForUpdates, setCheckingForUpdates] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<UpdateProgress | null>(null);
  const [autoCheckUpdates, setAutoCheckUpdates] = useState<boolean>(() => {
    const saved = localStorage.getItem("autoCheckUpdates");
    return saved === null ? true : saved === "true";
  });

  const toggleAutoCheckUpdates = useCallback(() => {
    setAutoCheckUpdates((prev) => {
      const next = !prev;
      localStorage.setItem("autoCheckUpdates", String(next));
      return next;
    });
  }, []);

  const checkForUpdates = useCallback(async (manual = false) => {
    setCheckingForUpdates(true);
    try {
      const update = await check();
      if (update) {
        const yes = await ask(`Update to ${update.version} is available! \n\nRelease notes: ${update.body}`, {
          title: "Update Available", kind: "info", okLabel: "Update", cancelLabel: "Cancel",
        });
        if (yes) {
          addLog("info", `Downloading update ${update.version}...`);
          let downloaded = 0;
          setUpdateProgress({ downloaded: 0, total: 0, percent: 0 });
          await update.downloadAndInstall((event) => {
            if (event.event === "Started" && event.data.contentLength) {
              setUpdateProgress({ downloaded: 0, total: event.data.contentLength, percent: 0 });
            } else if (event.event === "Progress") {
              downloaded += event.data.chunkLength;
              setUpdateProgress((prev) => {
                const total = prev?.total || 0;
                const percent = total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : 0;
                return { downloaded, total, percent };
              });
            } else if (event.event === "Finished") {
              setUpdateProgress((prev) => ({ downloaded: prev?.total || 0, total: prev?.total || 0, percent: 100 }));
            }
          });
          setUpdateProgress(null);
          addLog("success", "Update installed. Relaunching...");
          await relaunch();
        }
      } else if (manual) {
        addLog("info", "You're on the latest version.");
      }
    } catch (e) {
      setUpdateProgress(null);
      addLog("warning", `Failed to check for updates: ${e}`);
    } finally {
      setCheckingForUpdates(false);
    }
  }, [addLog]);

  // Auto-check on startup
  useEffect(() => {
    if (autoCheckUpdates) checkForUpdates(false);
  }, [checkForUpdates]); // eslint-disable-line react-hooks/exhaustive-deps

  return {
    checkingForUpdates, updateProgress,
    autoCheckUpdates, toggleAutoCheckUpdates,
    checkForUpdates,
  };
}

