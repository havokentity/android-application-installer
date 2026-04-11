import { FolderOpen, Package, X, Clock, FileOutput, Loader2, Info } from "lucide-react";
import { getFileName, shortcutLabel, formatBytes } from "../helpers";
import type { PackageMetadata, RecentFilesConfig } from "../types";

interface FileSectionProps {
  selectedFile: string | null;
  fileType: "apk" | "aab" | null;
  fileSize: number | null;
  isDragOver: boolean;
  isDragRejected: boolean;
  packageName: string;
  onPackageNameChange: (name: string) => void;
  onBrowseFile: () => void;
  onClearFile: () => void;
  onFileSelected: (path: string) => void;
  recentFiles: RecentFilesConfig;
  onRemoveRecentFile: (path: string) => void;
  canExtract: boolean;
  isExtracting: boolean;
  onExtractApk: () => void;
  allowDowngrade: boolean;
  onAllowDowngradeChange: (value: boolean) => void;
  metadata?: PackageMetadata | null;
}

export function FileSection({
  selectedFile, fileType, fileSize, isDragOver, isDragRejected, packageName,
  onPackageNameChange, onBrowseFile, onClearFile, onFileSelected,
  recentFiles, onRemoveRecentFile,
  canExtract, isExtracting, onExtractApk,
  allowDowngrade, onAllowDowngradeChange,
  metadata,
}: FileSectionProps) {
  return (
    <section className="section">
      <div className="section-header"><Package size={16} /><span>Package</span></div>
      <div className={`drop-zone ${selectedFile ? "has-file" : ""} ${isDragOver ? (isDragRejected ? "drag-rejected" : "drag-over") : ""}`} onClick={onBrowseFile}>
        {selectedFile ? (
          <div className="file-info">
            <div className="file-icon">{fileType === "apk" ? <Package size={32} /> : <FolderOpen size={32} />}</div>
            <div className="file-details">
              <span className="file-name">{getFileName(selectedFile)}</span>
              <span className="file-type">{fileType?.toUpperCase()} File{fileSize != null ? ` · ${formatBytes(fileSize)}` : ""}</span>
              <span className="file-path">{selectedFile}</span>
            </div>
            {fileType === "aab" && (
              <button className="btn btn-secondary btn-small" disabled={!canExtract} onClick={(e) => { e.stopPropagation(); onExtractApk(); }} title={`Extract universal APK from AAB (${shortcutLabel("E")})`}>
                {isExtracting ? <Loader2 size={14} className="spin" /> : <FileOutput size={14} />}
                {isExtracting ? "Extracting..." : "Extract APK"}
              </button>
            )}
            <button className="btn btn-icon btn-ghost" onClick={(e) => { e.stopPropagation(); onClearFile(); }} title="Clear selection">
              <X size={16} />
            </button>
          </div>
        ) : (
          <div className="drop-zone-content">
            <FolderOpen size={40} className="drop-icon" />
            <p className="drop-text">
              {isDragRejected ? "Unsupported file type" : isDragOver ? "Drop to select file" : (
                <>Click or drop an <span className="smaller-text">apk</span> or <span className="smaller-text">aab</span> file</>
              )}
            </p>
            {!isDragOver && (
              <p className="drop-hint">
                Supports <span className="smaller-text">.apk</span> and <span className="smaller-text">.aab</span> files — {shortcutLabel("O")} to browse
              </p>
            )}
            {isDragRejected && (
              <p className="drop-hint" style={{ color: "var(--red)" }}>
                Only <span className="smaller-text">.apk</span> and <span className="smaller-text">.aab</span> files are supported
              </p>
            )}
          </div>
        )}
      </div>
      {selectedFile && metadata && (metadata.versionName || metadata.minSdk || metadata.targetSdk) && (
        <div className="metadata-row">
          <Info size={12} />
          {metadata.versionName && <span>v{metadata.versionName}{metadata.versionCode ? ` (${metadata.versionCode})` : ""}</span>}
          {metadata.minSdk && <span>Min SDK {metadata.minSdk}</span>}
          {metadata.targetSdk && <span>Target SDK {metadata.targetSdk}</span>}
        </div>
      )}
      {!selectedFile && recentFiles.packages.length > 0 && (
        <div className="recent-list">
          <div className="recent-header"><Clock size={12} /> Recent Packages</div>
          {recentFiles.packages.map((f) => (
            <div key={f.path} className="recent-item" onClick={() => onFileSelected(f.path)} title={f.path}>
              <Package size={14} className="recent-icon" />
              <span className="recent-name">{f.name}</span>
              <span className="recent-path">{f.path}</span>
              <button className="btn btn-icon btn-ghost recent-remove" onClick={(e) => { e.stopPropagation(); onRemoveRecentFile(f.path); }} title="Remove">
                <X size={12} />
              </button>
            </div>
          ))}
        </div>
      )}
      <div className="package-row">
        <label className="field-label">Package Name (for Launch / Uninstall)</label>
        <div className="package-input-row">
          <input type="text" className="input" value={packageName} onChange={(e) => onPackageNameChange(e.target.value)} placeholder="com.example.myapp" />
          {selectedFile && (fileType === "apk" || fileType === "aab") && (
            <label className="checkbox-inline" title={fileType === "apk" ? "Pass -d flag to adb install" : "Pass --allow-downgrade to bundletool"}>
              <input type="checkbox" checked={allowDowngrade} onChange={(e) => onAllowDowngradeChange(e.target.checked)} />
              <span>Downgrade</span>
            </label>
          )}
        </div>
      </div>
    </section>
  );
}

