import { Monitor, Columns2, Sun, Moon, RefreshCw, Download, Bell, BellOff } from "lucide-react";

interface UpdateProgress {
  downloaded: number;
  total: number;
  percent: number;
}

interface ToolbarProps {
  layout: "portrait" | "landscape";
  theme: "dark" | "light";
  onToggleLayout: (mode: "portrait" | "landscape") => void;
  onSetTheme: (theme: "dark" | "light") => void;
  onCheckForUpdates: () => void;
  checkingForUpdates: boolean;
  updateProgress: UpdateProgress | null;
  autoCheckUpdates: boolean;
  onToggleAutoCheck: () => void;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1048576) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / 1048576).toFixed(1)} MB`;
}

export function Toolbar({ layout, theme, onToggleLayout, onSetTheme, onCheckForUpdates, checkingForUpdates, updateProgress, autoCheckUpdates, onToggleAutoCheck }: ToolbarProps) {
  return (
    <div className="toolbar-wrapper">
      <div className="toolbar">
        <div className="toolbar-group">
          <button className="toolbar-btn" onClick={onCheckForUpdates} disabled={checkingForUpdates || !!updateProgress} title="Check for updates">
            <RefreshCw size={13} className={checkingForUpdates ? "spin" : ""} /> {checkingForUpdates ? "Checking…" : "Updates"}
          </button>
          <button
            className={`toolbar-btn ${autoCheckUpdates ? "active" : ""}`}
            onClick={onToggleAutoCheck}
            title={autoCheckUpdates ? "Auto-check on startup: on" : "Auto-check on startup: off"}
          >
            {autoCheckUpdates ? <Bell size={13} /> : <BellOff size={13} />}
          </button>
        </div>
        <div className="toolbar-group">
          <button className={`toolbar-btn ${layout === "portrait" ? "active" : ""}`} onClick={() => onToggleLayout("portrait")} title="Portrait layout">
            <Monitor size={13} /> Portrait
          </button>
          <button className={`toolbar-btn ${layout === "landscape" ? "active" : ""}`} onClick={() => onToggleLayout("landscape")} title="Landscape layout">
            <Columns2 size={13} /> Landscape
          </button>
        </div>
        <div className="toolbar-group">
          <button className={`toolbar-btn ${theme === "light" ? "active" : ""}`} onClick={() => onSetTheme("light")} title="Light theme">
            <Sun size={13} />
          </button>
          <button className={`toolbar-btn ${theme === "dark" ? "active" : ""}`} onClick={() => onSetTheme("dark")} title="Dark theme">
            <Moon size={13} />
          </button>
        </div>
      </div>
      {updateProgress && (
        <div className="update-progress-banner">
          <Download size={14} className="update-progress-icon" />
          <div className="update-progress-bar-container">
            <div className="update-progress-bar-track">
              <div
                className="update-progress-bar-fill"
                style={{ width: `${updateProgress.percent}%` }}
              />
            </div>
          </div>
          <span className="update-progress-text">
            {updateProgress.percent}%
            {updateProgress.total > 0 && (
              <> · {formatBytes(updateProgress.downloaded)} / {formatBytes(updateProgress.total)}</>
            )}
          </span>
        </div>
      )}
    </div>
  );
}

