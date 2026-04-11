import { useState, memo } from "react";
import { Download, HardDriveDownload, Loader2, Wrench, Clock, X, ChevronDown, ChevronRight } from "lucide-react";
import { StatusDot } from "./StatusIndicators";
import { formatBytes } from "../helpers";
import type { DownloadProgress, StaleTool, ToolsStatus } from "../types";

// ─── Stale Tools Banner ───────────────────────────────────────────────────────

interface StaleBannerProps {
  staleTools: StaleTool[];
  dismissed: boolean;
  onDismiss: () => void;
}

export const StaleBanner = memo(function StaleBanner({ staleTools, dismissed, onDismiss }: StaleBannerProps) {
  if (staleTools.length === 0 || dismissed) return null;

  return (
    <div className="stale-banner">
      <div className="stale-banner-content">
        <Clock size={16} className="stale-banner-icon" />
        <div className="stale-banner-text">
          <strong>Updates available</strong>
          <span>
            {staleTools.map((s) => s.label).join(", ")}{" "}
            {staleTools.length === 1 ? "hasn't" : "haven't"} been updated in{" "}
            {Math.max(...staleTools.map((s) => s.age_days))}+ days.
            Use the buttons below to update.
          </span>
        </div>
      </div>
      <button
        className="btn btn-ghost btn-icon stale-banner-dismiss"
        onClick={onDismiss}
        title="Dismiss"
      >
        <X size={14} />
      </button>
    </div>
  );
});

// ─── Progress Bar ─────────────────────────────────────────────────────────────

interface ProgressBarProps {
  progress: DownloadProgress;
  showExtract?: boolean;
}

export function ProgressBar({ progress, showExtract = false }: ProgressBarProps) {
  return (
    <div className="progress-container">
      <div className="progress-bar">
        <div className="progress-fill" style={{ width: `${progress.percent}%` }} />
      </div>
      <span className="progress-text">
        {showExtract && progress.status === "extracting"
          ? "Extracting..."
          : progress.status === "done"
            ? "Done!"
            : `${formatBytes(progress.downloaded)} / ${formatBytes(progress.total)} (${progress.percent}%)`}
      </span>
    </div>
  );
}

// ─── Single Tool Row ──────────────────────────────────────────────────────────

interface ToolRowProps {
  installed: boolean;
  downloading: boolean;
  name: string;
  hint?: string;
  primary?: boolean;
  progress: DownloadProgress | null;
  showExtract?: boolean;
  onSetup: () => void;
}

const ToolRow = memo(function ToolRow({
  installed,
  downloading,
  name,
  hint,
  primary,
  progress,
  showExtract,
  onSetup,
}: ToolRowProps) {
  const Icon = primary ? HardDriveDownload : Download;
  const btnClass = primary ? "btn btn-primary btn-small" : "btn btn-small";

  const label = installed
    ? downloading ? "Updating..." : "Update"
    : downloading ? "Downloading..." : primary ? "Download ADB" : "Download";

  return (
    <>
      <div className="tool-row">
        <div className="tool-info">
          <StatusDot status={installed ? "found" : "not-found"} />
          <span className="tool-name">{name}</span>
          {installed && <span className="tool-badge badge-green">Installed</span>}
          {!installed && !downloading && <span className="tool-badge badge-yellow">Not installed</span>}
          {hint && <span className="tool-hint">{hint}</span>}
        </div>
        <div className="tool-actions">
          <button className={btnClass} onClick={onSetup} disabled={downloading}>
            {downloading ? <Loader2 size={14} className="spin" /> : <Icon size={14} />}
            {label}
          </button>
        </div>
      </div>
      {downloading && progress && (
        <ProgressBar progress={progress} showExtract={showExtract} />
      )}
    </>
  );
});

// ─── Tools Section ────────────────────────────────────────────────────────────

interface ToolsSectionProps {
  toolsStatus: ToolsStatus | null;
  downloadingAdb: boolean;
  downloadingBundletool: boolean;
  downloadingJava: boolean;
  adbProgress: DownloadProgress | null;
  btProgress: DownloadProgress | null;
  javaProgress: DownloadProgress | null;
  onSetupAdb: () => void;
  onSetupBundletool: () => void;
  onSetupJava: () => void;
  needsAttention?: boolean;
  compact?: boolean;
  collapsible?: boolean;
}

export function ToolsSection({
  toolsStatus,
  downloadingAdb,
  downloadingBundletool,
  downloadingJava,
  adbProgress,
  btProgress,
  javaProgress,
  onSetupAdb,
  onSetupBundletool,
  onSetupJava,
  needsAttention,
  compact,
  collapsible,
}: ToolsSectionProps) {
  const adbManaged = toolsStatus?.adb_installed ?? false;
  const btManaged = toolsStatus?.bundletool_installed ?? false;
  const javaManaged = toolsStatus?.java_installed ?? false;
  const allInstalled = adbManaged && btManaged && javaManaged;

  const [expanded, setExpanded] = useState(!allInstalled);

  // Auto-expand when tools go missing (e.g. after a status recheck)
  // Auto-collapse handled only by initial state — user can toggle freely after
  const isExpanded = collapsible ? expanded : true;

  const cls = [
    "section tools-section",
    needsAttention ? "tools-attention" : "",
    compact ? "tools-compact" : "",
    collapsible ? "collapsible" : "",
  ].filter(Boolean).join(" ");

  const toolContent = (
    <>
      <ToolRow
        installed={adbManaged}
        downloading={downloadingAdb}
        name="ADB (Android Debug Bridge)"
        primary
        progress={adbProgress}
        showExtract
        onSetup={onSetupAdb}
      />

      <ToolRow
        installed={btManaged}
        downloading={downloadingBundletool}
        name="bundletool"
        hint="(required for .aab files only)"
        progress={btProgress}
        onSetup={onSetupBundletool}
      />

      <ToolRow
        installed={javaManaged}
        downloading={downloadingJava}
        name="Java (Eclipse Temurin JRE 21)"
        hint="(required for .aab files only)"
        progress={javaProgress}
        showExtract
        onSetup={onSetupJava}
      />
    </>
  );

  if (collapsible) {
    return (
      <section className={cls}>
        <button className="section-header clickable" onClick={() => setExpanded(!expanded)}>
          {expanded ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
          <Wrench size={16} />
          <span>Required Tools</span>
          {allInstalled && <span className="tool-badge badge-green" style={{ marginLeft: 4 }}>All installed</span>}
          {!allInstalled && <span className="tool-badge badge-yellow" style={{ marginLeft: 4 }}>Setup needed</span>}
        </button>
        {isExpanded && (
          <div className="collapsible-content">
            {toolContent}
          </div>
        )}
      </section>
    );
  }

  return (
    <section className={cls}>
      <div className="section-header">
        <Wrench size={16} />
        <span>Required Tools</span>
        {!compact && <span className="section-hint">(auto-downloaded — no SDK needed)</span>}
      </div>
      {toolContent}
    </section>
  );
}

