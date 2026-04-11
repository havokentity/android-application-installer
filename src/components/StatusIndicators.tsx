import { memo } from "react";
import { Check, X, AlertTriangle, Info } from "lucide-react";
import type { LogEntry, DetectionStatus } from "../types";

/** Small coloured dot indicating found / not-found / unknown status. */
export const StatusDot = memo(function StatusDot({ status }: { status: DetectionStatus }) {
  const cls =
    status === "found"
      ? "status-dot green"
      : status === "not-found"
        ? "status-dot red"
        : "status-dot gray";
  return <span className={cls} />;
});

/** Coloured icon for a log entry. */
export const LogIcon = memo(function LogIcon({ level }: { level: LogEntry["level"] }) {
  switch (level) {
    case "success":
      return <Check size={12} className="log-icon green" />;
    case "error":
      return <X size={12} className="log-icon red" />;
    case "warning":
      return <AlertTriangle size={12} className="log-icon yellow" />;
    default:
      return <Info size={12} className="log-icon blue" />;
  }
});

