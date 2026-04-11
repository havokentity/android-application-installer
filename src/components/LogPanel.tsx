import { useRef, useEffect, useState, useCallback } from "react";
import { Info, Copy, Check, Download } from "lucide-react";
import { LogIcon } from "./StatusIndicators";
import type { LogEntry } from "../types";

const MAX_VISIBLE_LOGS = 200;

interface LogPanelProps {
  logs: LogEntry[];
  onClear: () => void;
  onSaveLogs?: () => void;
}

export function LogPanel({ logs, onClear, onSaveLogs }: LogPanelProps) {
  const logEndRef = useRef<HTMLDivElement>(null);
  const rafRef = useRef<number | null>(null);
  const [copied, setCopied] = useState(false);

  // Debounced auto-scroll using requestAnimationFrame
  useEffect(() => {
    if (rafRef.current !== null) cancelAnimationFrame(rafRef.current);
    rafRef.current = requestAnimationFrame(() => {
      logEndRef.current?.scrollIntoView({ behavior: "smooth" });
      rafRef.current = null;
    });
    return () => {
      if (rafRef.current !== null) cancelAnimationFrame(rafRef.current);
    };
  }, [logs]);

  const copyLogs = useCallback(async () => {
    const text = logs.map(e => `[${e.time}] [${e.level.toUpperCase()}] ${e.message}`).join("\n");
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch { /* clipboard not available */ }
  }, [logs]);

  const hiddenCount = Math.max(0, logs.length - MAX_VISIBLE_LOGS);
  const visibleLogs = hiddenCount > 0 ? logs.slice(-MAX_VISIBLE_LOGS) : logs;

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
            {onSaveLogs && (
              <button className="btn btn-ghost btn-small" onClick={onSaveLogs} title="Save log to file">
                <Download size={12} /> Save
              </button>
            )}
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
        {hiddenCount > 0 && (
          <div className="log-hidden-indicator">
            {hiddenCount} earlier {hiddenCount === 1 ? "entry" : "entries"} hidden
          </div>
        )}
        {visibleLogs.map((entry) => (
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

