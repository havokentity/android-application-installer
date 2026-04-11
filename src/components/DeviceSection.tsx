import {
  Smartphone, RefreshCw, Download, Play, Rocket, Square,
  AlertTriangle, Search, Loader2, ChevronDown, ChevronRight, Trash2, X,
  Usb, Wifi, Unplug, Radio, Zap, ShieldCheck,
} from "lucide-react";
import { Settings } from "lucide-react";
import { StatusDot } from "./StatusIndicators";
import { shortcutLabel } from "../helpers";
import { isWirelessDevice, isIpPortDevice, isMdnsDevice, deduplicateDevices } from "../hooks/useWirelessAdb";
import type { DeduplicatedDevice } from "../hooks/useWirelessAdb";
import type { WirelessAdbState } from "../hooks/useWirelessAdb";
import type { InstallMode } from "../hooks/useDeviceState";
import type { DetectionStatus, MdnsService, OperationProgress } from "../types";

/** Extract a short display name from an mDNS service name (e.g. "adb-PIXEL7-abc" → "PIXEL7"). */
function shortDeviceName(name: string): string {
  // mDNS names are usually "adb-<serial>-<suffix>" or "adb-<serial>"
  const match = name.match(/^adb-(.+?)(?:-[a-zA-Z0-9]{4,})?$/);
  return match ? match[1] : name;
}

/** Resolve a device serial to a display-friendly string.
 *  For mDNS serials, looks up the IP:port from discovered mDNS services.
 *  Falls back to a shortened device ID if no discovery data is available. */
function getDisplaySerial(serial: string, discoveredDevices: MdnsService[]): string {
  if (!isMdnsDevice(serial)) return serial;

  // mDNS serial: "adb-10BF190RC9001UZ-jvFPtf._adb-tls-connect._tcp"
  // mDNS service name: "adb-10BF190RC9001UZ-jvFPtf"
  const namePart = serial.split("._adb-tls")[0];
  const svc = discoveredDevices.find(
    (s) => s.name === namePart && s.service_type.includes("connect"),
  );
  if (svc) return svc.ip_port;

  // Fallback: extract the device ID from the mDNS serial
  const match = serial.match(/^adb-(.+?)(?:-[a-zA-Z0-9]{4,})?\._.*/);
  return match ? `${match[1]} (wireless)` : serial;
}

/** Group raw mDNS services by device name, merging connect + pairing entries. */
interface GroupedDevice {
  name: string;
  displayName: string;
  connectService: MdnsService | null;
  pairService: MdnsService | null;
}

function groupMdnsServices(services: MdnsService[]): GroupedDevice[] {
  const map = new Map<string, GroupedDevice>();
  for (const svc of services) {
    const base = shortDeviceName(svc.name);
    if (!map.has(base)) {
      map.set(base, { name: svc.name, displayName: base, connectService: null, pairService: null });
    }
    const entry = map.get(base)!;
    if (svc.service_type.includes("pairing")) {
      entry.pairService = svc;
    } else {
      entry.connectService = svc;
    }
  }
  return Array.from(map.values());
}

interface DeviceSectionProps {
  devices: DeduplicatedDevice[];
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
  installMode: InstallMode;
  onInstallModeChange: (mode: InstallMode) => void;
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
  installMode, onInstallModeChange,
  isInstalling, canInstall, packageName,
  onInstall, onLaunch, onStopApp, onUninstall,
  operationProgress, onCancelOperation,
  wireless,
}: DeviceSectionProps) {
  const selectedDeviceInfo = devices.find((d) => d.serial === selectedDevice);
  const deviceConnected = selectedDevice && devices.length > 0;
  const deviceLabel = deviceConnected
    ? (selectedDeviceInfo?.model || getDisplaySerial(selectedDevice, wireless.discoveredDevices))
    : null;
  const canLaunchOrUninstall = !!packageName && !!selectedDevice && !isInstalling;
  const wirelessDevices = devices.filter((d) => isWirelessDevice(d.serial));
  const hasWirelessDevices = wirelessDevices.length > 0;
  const activeDevices = devices.filter((d) => d.state === "device");
  const uniqueActiveDevices = deduplicateDevices(activeDevices);
  // Show mode toggle when the selected device is wireless
  const selectedIsWireless = !!(selectedDeviceInfo && isWirelessDevice(selectedDeviceInfo.serial));
  const selectedHasAlternate = !!(selectedDeviceInfo && selectedDeviceInfo.alternateSerial);
  // Determine which modes are available
  const hasDirectMode = selectedIsWireless && (
    isIpPortDevice(selectedDeviceInfo!.serial) ||
    (selectedDeviceInfo!.alternateSerial && isIpPortDevice(selectedDeviceInfo!.alternateSerial))
  );
  const hasVerifiedMode = selectedIsWireless && (
    isMdnsDevice(selectedDeviceInfo!.serial) ||
    (selectedDeviceInfo!.alternateSerial && isMdnsDevice(selectedDeviceInfo!.alternateSerial))
  );

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
                {devices.map((d) => {
                  const displaySerial = getDisplaySerial(d.serial, wireless.discoveredDevices);
                  return (
                    <option key={d.serial} value={d.serial}>
                      {d.model ? `${d.model} (${displaySerial})` : displaySerial}
                      {d.state !== "device" ? ` — ${d.state}` : ""}
                    </option>
                  );
                })}
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
            {selectedIsWireless && (
              <div className="install-mode-toggle">
                <span className="install-mode-label">Install mode:</span>
                <div className="install-mode-pills">
                  <button
                    className={`install-mode-pill ${installMode === "direct" || (!hasVerifiedMode && hasDirectMode) ? "active" : ""}`}
                    onClick={() => onInstallModeChange("direct")}
                    disabled={!hasDirectMode}
                    title={hasDirectMode
                      ? "Direct install via IP — bypasses Google Play Protect scanning"
                      : "Connect via IP:port to enable direct mode"}
                  >
                    <Zap size={11} /> Direct
                  </button>
                  <button
                    className={`install-mode-pill ${installMode === "verified" || (!hasDirectMode && hasVerifiedMode) ? "active" : ""}`}
                    onClick={() => onInstallModeChange("verified")}
                    disabled={!hasVerifiedMode}
                    title={hasVerifiedMode
                      ? "Verified install via mDNS — goes through Google Play Protect"
                      : "Scan for devices to enable verified mode"}
                  >
                    <ShieldCheck size={11} /> Verified
                  </button>
                </div>
              </div>
            )}
            {hasWirelessDevices && selectedDevice && isWirelessDevice(selectedDevice) && (
              <button
                className="btn btn-ghost btn-small wifi-disconnect-btn"
                onClick={() => wireless.disconnect(selectedDevice)}
                title="Disconnect wireless device"
              >
                <Unplug size={12} /> Disconnect {getDisplaySerial(selectedDevice, wireless.discoveredDevices)}
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
            {uniqueActiveDevices.length > 1 && (
              <div className="multi-device-row">
                <label className="multi-device-label">
                  <input
                    type="checkbox"
                    checked={installAllDevices}
                    onChange={(e) => onInstallAllDevicesChange(e.target.checked)}
                  />
                  Install to all {uniqueActiveDevices.length} connected devices
                  {uniqueActiveDevices.length !== activeDevices.length && (
                    <span className="hint" style={{ marginLeft: 6, display: "inline" }}>
                      ({activeDevices.length - uniqueActiveDevices.length} duplicate connection{activeDevices.length - uniqueActiveDevices.length > 1 ? "s" : ""} excluded)
                    </span>
                  )}
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
                  {wireless.isPairing && (
                    <button className="btn btn-ghost btn-small wifi-cancel-btn" onClick={wireless.cancelWirelessOp} title="Cancel pairing">
                      <X size={12} />
                    </button>
                  )}
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
                  {wireless.isConnecting && (
                    <button className="btn btn-ghost btn-small wifi-cancel-btn" onClick={wireless.cancelWirelessOp} title="Cancel connection">
                      <X size={12} />
                    </button>
                  )}
                </div>
                <p className="hint">IP & port shown on the Wireless Debugging screen (different port from pairing)</p>
                {wireless.needsPairing && (
                  <div className="wifi-pair-prompt">
                    <AlertTriangle size={12} />
                    <span>Device doesn't appear to be paired.</span>
                    <button
                      className="btn btn-accent btn-small"
                      onClick={wireless.promptPairing}
                    >
                      <Wifi size={12} /> Pair this device
                    </button>
                  </div>
                )}
              </div>

              {/* ── Network Discovery ──────────────────────────────── */}
              <div className="wifi-group">
                <div className="wifi-group-title wifi-discover-header">
                  <span>Devices on network</span>
                  <button
                    className="btn btn-ghost btn-small"
                    onClick={wireless.scan}
                    disabled={wireless.isScanning}
                    title="Scan for devices via mDNS"
                  >
                    {wireless.isScanning ? <Loader2 size={12} className="spin" /> : <Radio size={12} />}
                    {wireless.isScanning ? "Scanning..." : "Scan"}
                  </button>
                </div>
                {wireless.mdnsSupported === false && (
                  <p className="hint hint-warning"><AlertTriangle size={12} /> mDNS not supported. Update ADB platform-tools to 31+.</p>
                )}
                {wireless.discoveredDevices.length === 0 && wireless.mdnsSupported !== false && (
                  <p className="hint">Click Scan to discover Android devices with Wireless Debugging enabled.</p>
                )}
                {wireless.discoveredDevices.length > 0 && (
                  <ul className="wifi-discovered-list">
                    {groupMdnsServices(wireless.discoveredDevices).map((dev) => (
                      <li key={dev.displayName} className="wifi-discovered-item">
                        <div className="wifi-discovered-info">
                          <Smartphone size={14} className="wifi-discovered-icon" />
                          <span className="wifi-discovered-name">{dev.displayName}</span>
                        </div>
                        <div className="wifi-discovered-actions">
                          {dev.pairService && (
                            <button
                              className="btn btn-ghost btn-small"
                              onClick={() => wireless.selectDiscovered(dev.pairService!)}
                              title={`Fill pairing fields (${dev.pairService.ip_port})`}
                            >
                              <span className="wifi-discovered-type badge-yellow">Pair</span>
                              <span className="wifi-discovered-addr">{dev.pairService.ip_port}</span>
                            </button>
                          )}
                          {dev.connectService && (
                            <button
                              className="btn btn-ghost btn-small"
                              onClick={() => wireless.selectDiscovered(dev.connectService!)}
                              title={`Fill connect fields (${dev.connectService.ip_port})`}
                            >
                              <span className="wifi-discovered-type badge-green">Connect</span>
                              <span className="wifi-discovered-addr">{dev.connectService.ip_port}</span>
                            </button>
                          )}
                        </div>
                      </li>
                    ))}
                  </ul>
                )}
              </div>

              {/* ── Connected Wireless Devices ─────────────────────── */}
              {wirelessDevices.length > 0 && (
                <div className="wifi-group">
                  <div className="wifi-group-title">Connected wireless devices</div>
                  <ul className="wifi-discovered-list">
                    {wirelessDevices.map((d) => (
                      <li key={d.serial} className="wifi-discovered-item">
                        <div className="wifi-discovered-info">
                          <Wifi size={14} className="wifi-discovered-icon" />
                          <span className="wifi-discovered-name">{d.model || getDisplaySerial(d.serial, wireless.discoveredDevices)}</span>
                          {d.model && <span className="wifi-discovered-addr">{getDisplaySerial(d.serial, wireless.discoveredDevices)}</span>}
                          <span className={`wifi-discovered-type ${d.state === "device" ? "badge-green" : "badge-yellow"}`}>
                            {d.state === "device" ? "Online" : d.state}
                          </span>
                        </div>
                        <button
                          className="btn btn-ghost btn-small wifi-disconnect-inline"
                          onClick={() => wireless.disconnect(d.serial)}
                          title={`Disconnect ${getDisplaySerial(d.serial, wireless.discoveredDevices)}`}
                        >
                          <Unplug size={12} />
                        </button>
                      </li>
                    ))}
                  </ul>
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </section>
  );
}

