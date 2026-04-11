import { useRef, useEffect, useState, useCallback, useMemo } from "react";
import { Info, Copy, Check, Download, Search, X } from "lucide-react";
import { LogIcon } from "./StatusIndicators";
import type { LogEntry } from "../types";

const MAX_VISIBLE_LOGS = 200;
const LOG_LEVELS: LogEntry["level"][] = ["info", "success", "warning", "error"];

interface LogPanelProps {
  logs: LogEntry[];
  onClear: () => void;
  onSaveLogs?: () => void;
}

export function LogPanel({ logs, onClear, onSaveLogs }: LogPanelProps) {
  const logEndRef = useRef<HTMLDivElement>(null);
  const rafRef = useRef<number | null>(null);
  const [copied, setCopied] = useState(false);
  const [filterText, setFilterText] = useState("");
  const [hiddenLevels, setHiddenLevels] = useState<Set<LogEntry["level"]>>(new Set());

  const toggleLevel = useCallback((level: LogEntry["level"]) => {
    setHiddenLevels((prev) => {
      const next = new Set(prev);
      if (next.has(level)) next.delete(level); else next.add(level);
      return next;
    });
  }, []);

  // Filter logs by search text and level
  const filteredLogs = useMemo(() => {
    let result = logs;
    if (hiddenLevels.size > 0) {
      result = result.filter((e) => !hiddenLevels.has(e.level));
    }
    if (filterText) {
      const lower = filterText.toLowerCase();
      result = result.filter((e) => e.message.toLowerCase().includes(lower));
    }
    return result;
  }, [logs, hiddenLevels, filterText]);

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
  }, [filteredLogs]);

  const copyLogs = useCallback(async () => {
    const text = logs.map(e => `[${e.time}] [${e.level.toUpperCase()}] ${e.message}`).join("\n");
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch (e) { console.warn("Clipboard write failed:", e); }
  }, [logs]);

  const hiddenCount = Math.max(0, filteredLogs.length - MAX_VISIBLE_LOGS);
  const visibleLogs = hiddenCount > 0 ? filteredLogs.slice(-MAX_VISIBLE_LOGS) : filteredLogs;
  const isFiltering = filterText.length > 0 || hiddenLevels.size > 0;

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
      {logs.length > 0 && (
        <div className="log-filter-bar">
          <div className="log-filter-input-wrap">
            <Search size={12} />
            <input
              type="text" className="log-filter-input" placeholder="Filter logs…"
              value={filterText} onChange={(e) => setFilterText(e.target.value)}
            />
            {filterText && (
              <button className="btn btn-icon btn-ghost log-filter-clear" onClick={() => setFilterText("")} title="Clear filter">
                <X size={12} />
              </button>
            )}
          </div>
          <div className="log-level-toggles">
            {LOG_LEVELS.map((level) => (
              <button
                key={level}
                className={`btn btn-ghost btn-small log-level-btn log-level-${level} ${hiddenLevels.has(level) ? "log-level-off" : ""}`}
                onClick={() => toggleLevel(level)}
                title={`${hiddenLevels.has(level) ? "Show" : "Hide"} ${level} logs`}
              >
                <LogIcon level={level} /> {level}
              </button>
            ))}
          </div>
          {isFiltering && (
            <span className="log-filter-count">{filteredLogs.length} / {logs.length}</span>
          )}
        </div>
      )}
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

