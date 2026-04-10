import { Monitor, Columns2, Sun, Moon } from "lucide-react";

interface ToolbarProps {
  layout: "portrait" | "landscape";
  theme: "dark" | "light";
  onToggleLayout: (mode: "portrait" | "landscape") => void;
  onSetTheme: (theme: "dark" | "light") => void;
}

export function Toolbar({ layout, theme, onToggleLayout, onSetTheme }: ToolbarProps) {
  return (
    <div className="toolbar">
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
  );
}

