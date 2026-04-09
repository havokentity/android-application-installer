import { useRef, useEffect } from "react";
import { Info } from "lucide-react";
import { LogIcon } from "./StatusIndicators";
import type { LogEntry } from "../types";

interface LogPanelProps {
  logs: LogEntry[];
  onClear: () => void;
}

export function LogPanel({ logs, onClear }: LogPanelProps) {
  const logEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs]);

  return (
    <section className="section log-section">
      <div className="section-header">
        <Info size={16} />
        <span>Log</span>
        {logs.length > 0 && (
          <button className="btn btn-ghost btn-small" onClick={onClear}>
            Clear
          </button>
        )}
      </div>
      <div className="log-panel">
        {logs.length === 0 && (
          <p className="log-empty">
            No activity yet. Download ADB above, then select a file and install.
          </p>
        )}
        {logs.map((entry) => (
          <div key={entry.id} className={`log-entry log-${entry.level}`}>
            <span className="log-time">{entry.time}</span>
            <LogIcon level={entry.level} />
            <span className="log-message">{entry.message}</span>
          </div>
        ))}
        <div ref={logEndRef} />
      </div>
    </section>
  );
}

