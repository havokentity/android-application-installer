import {
  Smartphone, RefreshCw, Download, Play, Rocket,
  AlertTriangle, Search, Loader2, ChevronDown, ChevronRight, Trash2,
} from "lucide-react";
import { Settings } from "lucide-react";
import { StatusDot } from "./StatusIndicators";
import { shortcutLabel } from "../helpers";
import type { DeviceInfo, DetectionStatus } from "../types";

interface DeviceSectionProps {
  devices: DeviceInfo[];
  selectedDevice: string;
  onSelectDevice: (serial: string) => void;
  loadingDevices: boolean;
  onRefreshDevices: () => void;
  adbPath: string;
  adbStatus: DetectionStatus;
  adbManaged: boolean;
  onAdbPathChange: (path: string, status: DetectionStatus) => void;
  onDetectAdb: () => void;
  expanded: boolean;
  onToggleExpanded: () => void;
  installAllDevices: boolean;
  onInstallAllDevicesChange: (checked: boolean) => void;
  isInstalling: boolean;
  canInstall: boolean | string | null;
  packageName: string;
  onInstall: (andRun: boolean) => void;
  onLaunch: () => void;
  onUninstall: () => void;
}

export function DeviceSection({
  devices, selectedDevice, onSelectDevice, loadingDevices, onRefreshDevices,
  adbPath, adbStatus, adbManaged, onAdbPathChange, onDetectAdb,
  expanded, onToggleExpanded,
  installAllDevices, onInstallAllDevicesChange,
  isInstalling, canInstall, packageName,
  onInstall, onLaunch, onUninstall,
}: DeviceSectionProps) {
  const selectedDeviceInfo = devices.find((d) => d.serial === selectedDevice);
  const deviceConnected = selectedDevice && devices.length > 0;
  const deviceLabel = deviceConnected ? (selectedDeviceInfo?.model || selectedDevice) : null;
  const canLaunchOrUninstall = !!packageName && !!selectedDevice && !isInstalling;

  return (
    <section className={`section collapsible ${!deviceConnected ? "device-attention" : ""}`}>
      <div className="section-header clickable device-header" onClick={onToggleExpanded}>
        <div className="device-header-left">
          {expanded ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
          <Settings size={16} /><span>Device</span>
          {deviceConnected && <span className="tool-badge badge-green">{deviceLabel}</span>}
          {!deviceConnected && <span className="tool-badge badge-yellow">No device</span>}
        </div>
        <div className="device-actions" onClick={(e) => e.stopPropagation()}>
          <button className="btn btn-primary btn-small" disabled={!canInstall} onClick={() => onInstall(false)} title={`Install (${shortcutLabel("I")})`}>
            {isInstalling ? <Loader2 size={14} className="spin" /> : <Download size={14} />}
            {isInstalling ? "Installing..." : "Install"}
          </button>
          <button className="btn btn-accent btn-small" disabled={!canInstall} onClick={() => onInstall(true)} title={`Install & Run (${shortcutLabel("I", true)})`}><Play size={14} /> Install & Run</button>
          <button className="btn btn-secondary btn-small" disabled={!canLaunchOrUninstall} onClick={onLaunch} title={`Launch (${shortcutLabel("L")})`}><Rocket size={14} /> Launch</button>
          <button className="btn btn-danger btn-small" disabled={!canLaunchOrUninstall} onClick={onUninstall} title={`Uninstall (${shortcutLabel("U")})`}><Trash2 size={14} /> Uninstall</button>
        </div>
      </div>
      {expanded && (
        <div className="collapsible-content">
          <div className="adb-row">
            <label className="field-label">ADB Path</label>
            <div className="input-group">
              <input type="text" className="input" value={adbPath}
                onChange={(e) => onAdbPathChange(e.target.value, e.target.value ? "found" : "not-found")}
                placeholder={adbManaged ? "Managed by app — auto-detected" : "Path to adb binary..."} />
              <StatusDot status={adbStatus} />
              <button className="btn btn-icon" onClick={onDetectAdb} title="Auto-detect ADB"><Search size={16} /></button>
            </div>
          </div>
          <div className="device-row">
            <label className="field-label"><Smartphone size={14} /> Connected Device</label>
            <div className="input-group">
              <select className="select" value={selectedDevice} onChange={(e) => onSelectDevice(e.target.value)} disabled={devices.length === 0}>
                {devices.length === 0 && <option value="">No devices connected</option>}
                {devices.map((d) => (
                  <option key={d.serial} value={d.serial}>
                    {d.model ? `${d.model} (${d.serial})` : d.serial}
                    {d.state !== "device" ? ` — ${d.state}` : ""}
                  </option>
                ))}
              </select>
              <button className="btn btn-icon" onClick={onRefreshDevices} disabled={loadingDevices || !adbPath} title="Refresh devices">
                <RefreshCw size={16} className={loadingDevices ? "spin" : ""} />
              </button>
            </div>
            {selectedDeviceInfo?.state === "unauthorized" && (
              <p className="hint hint-warning"><AlertTriangle size={12} /> Accept the USB debugging prompt on your device.</p>
            )}
            {devices.length > 1 && (
              <div className="multi-device-row">
                <label className="multi-device-label">
                  <input
                    type="checkbox"
                    checked={installAllDevices}
                    onChange={(e) => onInstallAllDevicesChange(e.target.checked)}
                  />
                  Install to all {devices.filter(d => d.state === "device").length} connected devices
                </label>
              </div>
            )}
          </div>
        </div>
      )}
    </section>
  );
}

