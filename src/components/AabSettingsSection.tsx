import {
  FolderOpen, Download, Search, Check, X, Coffee, Loader2,
  ChevronDown, ChevronRight, Key, Clock, RefreshCw, Save, Trash2, User,
} from "lucide-react";
import { useState } from "react";
import { StatusDot } from "./StatusIndicators";
import type { DetectionStatus, RecentFile, SigningProfile } from "../types";

interface AabSettingsSectionProps {
  show: boolean;
  onToggle: () => void;
  javaPath: string;
  javaVersion: string;
  javaStatus: DetectionStatus;
  javaManaged: boolean;
  onJavaPathChange: (path: string) => void;
  onCheckJava: () => void;
  onSetupJava: () => void;
  downloadingJava: boolean;
  bundletoolPath: string;
  bundletoolStatus: DetectionStatus;
  onBundletoolPathChange: (path: string, status: DetectionStatus) => void;
  onDetectBundletool: () => void;
  onSetupBundletool: () => void;
  downloadingBundletool: boolean;
  keystorePath: string;
  keystorePass: string;
  keyAlias: string;
  keyPass: string;
  keyAliases: string[];
  loadingAliases: boolean;
  onKeystorePathChange: (path: string) => void;
  onKeystorePassChange: (pass: string) => void;
  onKeyAliasChange: (alias: string) => void;
  onKeyPassChange: (pass: string) => void;
  onBrowseKeystore: () => void;
  onFetchKeyAliases: () => void;
  recentKeystores: RecentFile[];
  onSelectRecentKeystore: (path: string) => void;
  onRemoveRecentKeystore: (path: string) => void;
  signingProfiles?: SigningProfile[];
  activeProfileName?: string | null;
  onSelectProfile?: (name: string | null) => void;
  onSaveProfile?: (name: string) => void;
  onDeleteProfile?: (name: string) => void;
}

export function AabSettingsSection({
  show, onToggle,
  javaPath, javaVersion, javaStatus, javaManaged,
  onJavaPathChange, onCheckJava, onSetupJava, downloadingJava,
  bundletoolPath, bundletoolStatus,
  onBundletoolPathChange, onDetectBundletool, onSetupBundletool, downloadingBundletool,
  keystorePath, keystorePass, keyAlias, keyPass, keyAliases, loadingAliases,
  onKeystorePathChange, onKeystorePassChange, onKeyAliasChange, onKeyPassChange,
  onBrowseKeystore, onFetchKeyAliases,
  recentKeystores, onSelectRecentKeystore, onRemoveRecentKeystore,
  signingProfiles, activeProfileName, onSelectProfile, onSaveProfile, onDeleteProfile,
}: AabSettingsSectionProps) {
  const [newProfileName, setNewProfileName] = useState("");
  const [showSaveInput, setShowSaveInput] = useState(false);
  return (
    <section className="section collapsible">
      <button className="section-header clickable" onClick={onToggle}>
        {show ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
        <Coffee size={16} /><span>AAB Settings</span>
        <span className="section-hint">(Java, bundletool, keystore — required for .aab files)</span>
      </button>
      {show && (
        <div className="collapsible-content">
          <div className="setting-row">
            <label className="field-label">Java</label>
            <div className="input-group">
              <input type="text" className="input" value={javaPath} onChange={(e) => onJavaPathChange(e.target.value)} placeholder={javaManaged ? "Managed by app — auto-detected" : "java"} />
              <StatusDot status={javaStatus} />
              <button className="btn btn-icon" onClick={onCheckJava} title="Detect Java"><Search size={16} /></button>
              {javaStatus === "not-found" && (
                <button className="btn btn-small" onClick={onSetupJava} disabled={downloadingJava} title="Download Java JRE">
                  {downloadingJava ? <Loader2 size={14} className="spin" /> : <Download size={14} />} Download
                </button>
              )}
            </div>
            {javaVersion && <p className="hint hint-success"><Check size={12} /> {javaVersion}</p>}
          </div>
          <div className="setting-row">
            <label className="field-label">bundletool.jar</label>
            <div className="input-group">
              <input type="text" className="input" value={bundletoolPath} onChange={(e) => onBundletoolPathChange(e.target.value, e.target.value ? "found" : "not-found")} placeholder="Path to bundletool.jar..." />
              <StatusDot status={bundletoolStatus} />
              <button className="btn btn-icon" onClick={onDetectBundletool} title="Detect bundletool"><Search size={16} /></button>
              <button className="btn btn-small" onClick={onSetupBundletool} disabled={downloadingBundletool} title="Download latest from GitHub">
                {downloadingBundletool ? <Loader2 size={14} className="spin" /> : <Download size={14} />} Download
              </button>
            </div>
          </div>
          <div className="setting-row">
            <label className="field-label">Keystore (optional)</label>
            <div className="input-group">
              <input type="text" className="input" value={keystorePath} onChange={(e) => onKeystorePathChange(e.target.value)} placeholder="Path to .jks / .keystore (leave empty for debug key)" />
              <button className="btn btn-icon" onClick={onBrowseKeystore} title="Browse"><FolderOpen size={16} /></button>
            </div>
            {!keystorePath && recentKeystores.length > 0 && (
              <div className="recent-list recent-list-compact">
                <div className="recent-header"><Clock size={12} /> Recent Keystores</div>
                {recentKeystores.map((f) => (
                  <div key={f.path} className="recent-item" onClick={() => onSelectRecentKeystore(f.path)} title={f.path}>
                    <Key size={14} className="recent-icon" />
                    <span className="recent-name">{f.name}</span>
                    <span className="recent-path">{f.path}</span>
                    <button className="btn btn-icon btn-ghost recent-remove" onClick={(e) => { e.stopPropagation(); onRemoveRecentKeystore(f.path); }} title="Remove">
                      <X size={12} />
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>
          {keystorePath && (
            <>
              <div className="setting-row indent">
                <label className="field-label">Keystore Password</label>
                <input type="password" className="input" value={keystorePass} onChange={(e) => onKeystorePassChange(e.target.value)} placeholder="Keystore password" />
              </div>
              <div className="setting-row indent">
                <label className="field-label"><Key size={14} /> Key Alias</label>
                <div className="input-group">
                  {keyAliases.length > 0 ? (
                    <select className="select" value={keyAlias} onChange={(e) => onKeyAliasChange(e.target.value)}>
                      <option value="">— Select alias —</option>
                      {keyAliases.map((a) => (
                        <option key={a} value={a}>{a}</option>
                      ))}
                    </select>
                  ) : (
                    <input type="text" className="input" value={keyAlias} onChange={(e) => onKeyAliasChange(e.target.value)} placeholder={loadingAliases ? "Loading aliases..." : "Key alias (enter password to list)"} />
                  )}
                  {loadingAliases && <Loader2 size={14} className="spin" />}
                  {keystorePass && !loadingAliases && (
                    <button className="btn btn-icon" onClick={onFetchKeyAliases} title="Refresh aliases">
                      <RefreshCw size={14} />
                    </button>
                  )}
                </div>
              </div>
              <div className="setting-row indent">
                <label className="field-label">Key Password</label>
                <input type="password" className="input" value={keyPass} onChange={(e) => onKeyPassChange(e.target.value)} placeholder="Key password" />
              </div>
              {signingProfiles && onSaveProfile && onDeleteProfile && onSelectProfile && (
                <div className="setting-row indent">
                  <label className="field-label"><User size={14} /> Signing Profiles</label>
                  <div className="input-group">
                    <select className="select" value={activeProfileName ?? ""} onChange={(e) => onSelectProfile(e.target.value || null)}>
                      <option value="">— No profile —</option>
                      {signingProfiles.map((p) => (
                        <option key={p.name} value={p.name}>{p.name}</option>
                      ))}
                    </select>
                    {activeProfileName && (
                      <button className="btn btn-icon btn-ghost" onClick={() => onDeleteProfile(activeProfileName)} title="Delete profile">
                        <Trash2 size={14} />
                      </button>
                    )}
                    <button className="btn btn-small" onClick={() => setShowSaveInput(!showSaveInput)} title="Save current settings as profile">
                      <Save size={14} /> Save
                    </button>
                  </div>
                  {showSaveInput && (
                    <div className="input-group" style={{ marginTop: 4 }}>
                      <input type="text" className="input" value={newProfileName} onChange={(e) => setNewProfileName(e.target.value)} placeholder="Profile name..." onKeyDown={(e) => { if (e.key === "Enter" && newProfileName.trim()) { onSaveProfile(newProfileName.trim()); setNewProfileName(""); setShowSaveInput(false); } }} />
                      <button className="btn btn-small" disabled={!newProfileName.trim()} onClick={() => { onSaveProfile(newProfileName.trim()); setNewProfileName(""); setShowSaveInput(false); }}>
                        <Check size={14} />
                      </button>
                    </div>
                  )}
                </div>
              )}
            </>
          )}
        </div>
      )}
    </section>
  );
}

