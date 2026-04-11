import {
  Smartphone, RefreshCw, Download, Play, Rocket, Square,
  AlertTriangle, Search, Loader2, ChevronDown, ChevronRight, Trash2, X,
  Usb, Wifi, Unplug,
} from "lucide-react";
import { Settings } from "lucide-react";
import { StatusDot } from "./StatusIndicators";
import { shortcutLabel } from "../helpers";
import { isWirelessDevice } from "../hooks/useWirelessAdb";
import type { WirelessAdbState } from "../hooks/useWirelessAdb";
import type { DeviceInfo, DetectionStatus, OperationProgress } from "../types";

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
  onStopApp: () => void;
  onUninstall: () => void;
  operationProgress: OperationProgress | null;
  onCancelOperation: () => void;
  wireless: WirelessAdbState;
}

export function DeviceSection({
  devices, selectedDevice, onSelectDevice, loadingDevices, onRefreshDevices,
  adbPath, adbStatus, adbManaged, onAdbPathChange, onDetectAdb,
  expanded, onToggleExpanded,
  installAllDevices, onInstallAllDevicesChange,
  isInstalling, canInstall, packageName,
  onInstall, onLaunch, onStopApp, onUninstall,
  operationProgress, onCancelOperation,
  wireless,
}: DeviceSectionProps) {
  const selectedDeviceInfo = devices.find((d) => d.serial === selectedDevice);
  const deviceConnected = selectedDevice && devices.length > 0;
  const deviceLabel = deviceConnected ? (selectedDeviceInfo?.model || selectedDevice) : null;
  const canLaunchOrUninstall = !!packageName && !!selectedDevice && !isInstalling;
  const hasWirelessDevices = devices.some((d) => isWirelessDevice(d.serial));

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
          <button className="btn btn-primary btn-small" disabled={!canInstall} onClick={() => onInstall(true)} title={`Install & Run (${shortcutLabel("I", true)})`}>
            {isInstalling ? <Loader2 size={14} className="spin" /> : <Play size={14} />}
            {isInstalling ? "Installing..." : "Install & Run"}
          </button>
          <button className="btn btn-accent btn-small" disabled={!canInstall} onClick={() => onInstall(false)} title={`Install (${shortcutLabel("I")})`}>
            <Download size={14} /> Install
          </button>
          <button className="btn btn-secondary btn-small" disabled={!canLaunchOrUninstall} onClick={onLaunch} title={`Launch (${shortcutLabel("L")})`}><Rocket size={14} /> Launch</button>
          <button className="btn btn-warning btn-small" disabled={!canLaunchOrUninstall} onClick={onStopApp} title={`Stop (${shortcutLabel("K")})`}><Square size={14} /> Stop</button>
          <button className="btn btn-danger btn-small" disabled={!canLaunchOrUninstall} onClick={onUninstall} title={`Uninstall (${shortcutLabel("U")})`}><Trash2 size={14} /> Uninstall</button>
        </div>
      </div>

      {/* Operation progress bar — always visible when active */}
      {operationProgress && (operationProgress.status === "running" || operationProgress.status === "done") && (
        <div className="operation-progress">
          <div className="operation-progress-bar">
            <div className={`operation-progress-fill ${operationProgress.status === "done" ? "done" : "indeterminate"}`} />
          </div>
          <div className="operation-progress-info">
            <span className="operation-progress-message">
              {operationProgress.message}
              {operationProgress.total_steps != null && operationProgress.total_steps > 1 && operationProgress.step != null && (
                <span className="operation-progress-step">
                  {` (step ${operationProgress.step}/${operationProgress.total_steps})`}
                </span>
              )}
            </span>
            {operationProgress.cancellable && operationProgress.status === "running" && (
              <button
                className="btn btn-ghost btn-small operation-cancel"
                onClick={(e) => { e.stopPropagation(); onCancelOperation(); }}
                title="Cancel operation"
              >
                <X size={12} /> Cancel
              </button>
            )}
          </div>
        </div>
      )}

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
              <button
                className={`btn btn-icon ${wireless.wifiExpanded ? "btn-active" : ""}`}
                onClick={() => wireless.setWifiExpanded(!wireless.wifiExpanded)}
                disabled={!adbPath}
                title="Wireless ADB (WiFi)"
              >
                <Wifi size={16} />
              </button>
            </div>
            {selectedDeviceInfo?.state === "unauthorized" && (
              <p className="hint hint-warning"><AlertTriangle size={12} /> Accept the USB debugging prompt on your device.</p>
            )}
            {hasWirelessDevices && selectedDevice && isWirelessDevice(selectedDevice) && (
              <button
                className="btn btn-ghost btn-small wifi-disconnect-btn"
                onClick={() => wireless.disconnect(selectedDevice)}
                title="Disconnect wireless device"
              >
                <Unplug size={12} /> Disconnect {selectedDevice}
              </button>
            )}
            {devices.length === 0 && (
              <div className="device-empty-state">
                <Usb size={32} className="device-empty-icon" />
                <p className="device-empty-title">No device connected</p>
                <ol className="device-empty-steps">
                  <li>Enable <strong>USB debugging</strong> in Developer Options</li>
                  <li>Connect your device via USB cable</li>
                  <li>Accept the debugging prompt on your device</li>
                </ol>
                <button className="btn btn-small btn-accent" onClick={onRefreshDevices} disabled={loadingDevices || !adbPath}>
                  <RefreshCw size={14} className={loadingDevices ? "spin" : ""} /> Refresh
                </button>
              </div>
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

          {/* ── Wireless ADB Panel ──────────────────────────────────── */}
          {wireless.wifiExpanded && (
            <div className="wifi-panel">
              <div className="wifi-panel-header">
                <Wifi size={14} />
                <span>Wireless ADB (Android 11+)</span>
              </div>

              <div className="wifi-group">
                <div className="wifi-group-title">1. Pair (first time only)</div>
                <div className="wifi-inputs">
                  <input
                    type="text" className="input" placeholder="IP address"
                    value={wireless.pairIp} onChange={(e) => wireless.setPairIp(e.target.value)}
                  />
                  <input
                    type="text" className="input wifi-port-input" placeholder="Port"
                    value={wireless.pairPort} onChange={(e) => wireless.setPairPort(e.target.value)}
                    maxLength={5}
                  />
                  <input
                    type="text" className="input wifi-code-input" placeholder="Pairing code"
                    value={wireless.pairingCode} onChange={(e) => wireless.setPairingCode(e.target.value)}
                    maxLength={6}
                  />
                  <button
                    className="btn btn-accent btn-small"
                    disabled={!wireless.canPair}
                    onClick={wireless.pair}
                  >
                    {wireless.isPairing ? <Loader2 size={14} className="spin" /> : <Wifi size={14} />}
                    {wireless.isPairing ? "Pairing..." : "Pair"}
                  </button>
                </div>
                <p className="hint">Find pairing code in Settings → Developer Options → Wireless Debugging → Pair device</p>
              </div>

              <div className="wifi-group">
                <div className="wifi-group-title">2. Connect</div>
                <div className="wifi-inputs">
                  <input
                    type="text" className="input" placeholder="IP address"
                    value={wireless.connectIp} onChange={(e) => wireless.setConnectIp(e.target.value)}
                  />
                  <input
                    type="text" className="input wifi-port-input" placeholder="Port"
                    value={wireless.connectPort} onChange={(e) => wireless.setConnectPort(e.target.value)}
                    maxLength={5}
                  />
                  <button
                    className="btn btn-primary btn-small"
                    disabled={!wireless.canConnect}
                    onClick={wireless.connect}
                  >
                    {wireless.isConnecting ? <Loader2 size={14} className="spin" /> : <Wifi size={14} />}
                    {wireless.isConnecting ? "Connecting..." : "Connect"}
                  </button>
                </div>
                <p className="hint">IP & port shown on the Wireless Debugging screen (different port from pairing)</p>
              </div>
            </div>
          )}
        </div>
      )}
    </section>
  );
}

