import { FolderOpen, Package, X, Clock } from "lucide-react";
import { getFileName, shortcutLabel } from "../helpers";
import type { RecentFilesConfig } from "../types";

interface FileSectionProps {
  selectedFile: string | null;
  fileType: "apk" | "aab" | null;
  isDragOver: boolean;
  packageName: string;
  onPackageNameChange: (name: string) => void;
  onBrowseFile: () => void;
  onClearFile: () => void;
  onFileSelected: (path: string) => void;
  recentFiles: RecentFilesConfig;
  onRemoveRecentFile: (path: string) => void;
}

export function FileSection({
  selectedFile, fileType, isDragOver, packageName,
  onPackageNameChange, onBrowseFile, onClearFile, onFileSelected,
  recentFiles, onRemoveRecentFile,
}: FileSectionProps) {
  return (
    <section className="section">
      <div className="section-header"><Package size={16} /><span>Package</span></div>
      <div className={`drop-zone ${selectedFile ? "has-file" : ""} ${isDragOver ? "drag-over" : ""}`} onClick={onBrowseFile}>
        {selectedFile ? (
          <div className="file-info">
            <div className="file-icon">{fileType === "apk" ? <Package size={32} /> : <FolderOpen size={32} />}</div>
            <div className="file-details">
              <span className="file-name">{getFileName(selectedFile)}</span>
              <span className="file-type">{fileType?.toUpperCase()} File</span>
              <span className="file-path">{selectedFile}</span>
            </div>
            <button className="btn btn-icon btn-ghost" onClick={(e) => { e.stopPropagation(); onClearFile(); }} title="Clear selection">
              <X size={16} />
            </button>
          </div>
        ) : (
          <div className="drop-zone-content">
            <FolderOpen size={40} className="drop-icon" />
            <p className="drop-text">
              {isDragOver ? "Drop to select file" : (
                <>Click or drop an <span className="smaller-text">apk</span> or <span className="smaller-text">aab</span> file</>
              )}
            </p>
            {!isDragOver && (
              <p className="drop-hint">
                Supports <span className="smaller-text">.apk</span> and <span className="smaller-text">.aab</span> files — {shortcutLabel("O")} to browse
              </p>
            )}
          </div>
        )}
      </div>
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
        <input type="text" className="input" value={packageName} onChange={(e) => onPackageNameChange(e.target.value)} placeholder="com.example.myapp" />
      </div>
    </section>
  );
}

