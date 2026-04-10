import { useRef, useEffect, useState } from "react";
import { Info, Copy, Check } from "lucide-react";
import { LogIcon } from "./StatusIndicators";
import type { LogEntry } from "../types";

interface LogPanelProps {
  logs: LogEntry[];
  onClear: () => void;
}

export function LogPanel({ logs, onClear }: LogPanelProps) {
  const logEndRef = useRef<HTMLDivElement>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs]);

  const copyLogs = async () => {
    const text = logs.map(e => `[${e.time}] [${e.level.toUpperCase()}] ${e.message}`).join("\n");
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch { /* clipboard not available */ }
  };

  return (
    <section className="section log-section">
      <div className="section-header">
        <Info size={16} />
        <span>Log</span>
        {logs.length > 0 && (
          <>
            <button className="btn btn-ghost btn-small" onClick={copyLogs} title="Copy log to clipboard">
              {copied ? <Check size={12} /> : <Copy size={12} />} {copied ? "Copied" : "Copy"}
            </button>
            <button className="btn btn-ghost btn-small" onClick={onClear}>
              Clear
            </button>
          </>
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

