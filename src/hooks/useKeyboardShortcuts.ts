import { useEffect, useRef } from "react";

export interface ShortcutActions {
  browseFile: () => void;
  install: (andRun: boolean) => void;
  launchApp: () => void;
  stopApp: () => void;
  uninstallApp: () => void;
  extractApk: () => void;
  canInstall: boolean | string | null;
  canLaunch: boolean;
  canStop: boolean;
  canUninstall: boolean;
  canExtract: boolean;
}

export function useKeyboardShortcuts(actions: ShortcutActions) {
  const ref = useRef(actions);
  ref.current = actions;

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (!mod) return;
      const key = e.key.toLowerCase();
      if (key === "o") {
        e.preventDefault();
        ref.current.browseFile();
      } else if (key === "i" && e.shiftKey) {
        e.preventDefault();
        if (ref.current.canInstall) ref.current.install(true);
      } else if (key === "i") {
        e.preventDefault();
        if (ref.current.canInstall) ref.current.install(false);
      } else if (key === "l") {
        e.preventDefault();
        if (ref.current.canLaunch) ref.current.launchApp();
      } else if (key === "k") {
        e.preventDefault();
        if (ref.current.canStop) ref.current.stopApp();
      } else if (key === "u") {
        e.preventDefault();
        if (ref.current.canUninstall) ref.current.uninstallApp();
      } else if (key === "e") {
        e.preventDefault();
        if (ref.current.canExtract) ref.current.extractApk();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);
}

