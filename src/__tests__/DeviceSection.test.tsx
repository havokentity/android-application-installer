// ─── Tests for src/components/DeviceSection.tsx ───────────────────────────────
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { DeviceSection } from "../components/DeviceSection";
import type { DeviceInfo } from "../types";
import type { WirelessAdbState } from "../hooks/useWirelessAdb";

const device1: DeviceInfo = {
  serial: "ABC123",
  state: "device",
  model: "Pixel 7",
  product: "panther",
  transport_id: "1",
};

const device2: DeviceInfo = {
  serial: "DEF456",
  state: "device",
  model: "Galaxy S24",
  product: "galaxy",
  transport_id: "2",
};

const unauthorizedDevice: DeviceInfo = {
  serial: "UNA789",
  state: "unauthorized",
  model: "",
  product: "",
  transport_id: "3",
};

const wirelessDevice: DeviceInfo = {
  serial: "192.168.1.100:5555",
  state: "device",
  model: "Pixel 7",
  product: "panther",
  transport_id: "4",
};

const wirelessDefaults: WirelessAdbState = {
  wifiExpanded: false,
  setWifiExpanded: vi.fn(),
  pairIp: "",
  setPairIp: vi.fn(),
  pairPort: "",
  setPairPort: vi.fn(),
  pairingCode: "",
  setPairingCode: vi.fn(),
  connectIp: "",
  setConnectIp: vi.fn(),
  connectPort: "",
  setConnectPort: vi.fn(),
  isPairing: false,
  isConnecting: false,
  canPair: false,
  canConnect: false,
  pair: vi.fn(),
  connect: vi.fn(),
  disconnect: vi.fn(),
  discoveredDevices: [],
  isScanning: false,
  mdnsSupported: null,
  scan: vi.fn(),
  selectDiscovered: vi.fn(),
};

const defaults = {
  devices: [] as DeviceInfo[],
  selectedDevice: "",
  onSelectDevice: vi.fn(),
  loadingDevices: false,
  onRefreshDevices: vi.fn(),
  adbPath: "/usr/bin/adb",
  adbStatus: "found" as const,
  adbManaged: false,
  onAdbPathChange: vi.fn(),
  onDetectAdb: vi.fn(),
  expanded: true,
  onToggleExpanded: vi.fn(),
  installAllDevices: false,
  onInstallAllDevicesChange: vi.fn(),
  isInstalling: false,
  canInstall: false as boolean | string | null,
  packageName: "",
  onInstall: vi.fn(),
  onLaunch: vi.fn(),
  onStopApp: vi.fn(),
  onUninstall: vi.fn(),
  operationProgress: null,
  onCancelOperation: vi.fn(),
  wireless: wirelessDefaults,
};

describe("DeviceSection", () => {
  it("renders the Device section header", () => {
    render(<DeviceSection {...defaults} />);
    expect(screen.getByText("Device")).toBeInTheDocument();
  });

  it("shows 'No device' badge when no devices are connected", () => {
    render(<DeviceSection {...defaults} />);
    expect(screen.getByText("No device")).toBeInTheDocument();
  });

  it("shows device model as badge when a device is connected", () => {
    render(<DeviceSection {...defaults} devices={[device1]} selectedDevice="ABC123" />);
    expect(screen.getByText("Pixel 7")).toBeInTheDocument();
  });

  it("shows ADB path input when expanded", () => {
    render(<DeviceSection {...defaults} expanded={true} />);
    expect(screen.getByDisplayValue("/usr/bin/adb")).toBeInTheDocument();
  });

  it("hides content when collapsed", () => {
    render(<DeviceSection {...defaults} expanded={false} />);
    expect(screen.queryByText("ADB Path")).not.toBeInTheDocument();
  });

  it("calls onToggleExpanded when header is clicked", () => {
    const onToggle = vi.fn();
    render(<DeviceSection {...defaults} onToggleExpanded={onToggle} />);
    fireEvent.click(screen.getByText("Device"));
    expect(onToggle).toHaveBeenCalledOnce();
  });

  it("shows 'No devices connected' when device list is empty", () => {
    render(<DeviceSection {...defaults} expanded={true} />);
    expect(screen.getByText("No devices connected")).toBeInTheDocument();
  });

  it("renders device options in the select dropdown", () => {
    render(<DeviceSection {...defaults} devices={[device1, device2]} selectedDevice="ABC123" expanded={true} />);
    expect(screen.getByText("Pixel 7 (ABC123)")).toBeInTheDocument();
    expect(screen.getByText("Galaxy S24 (DEF456)")).toBeInTheDocument();
  });

  it("calls onSelectDevice when a device is selected from dropdown", () => {
    const onSelect = vi.fn();
    render(<DeviceSection {...defaults} devices={[device1, device2]} selectedDevice="ABC123" onSelectDevice={onSelect} expanded={true} />);
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "DEF456" } });
    expect(onSelect).toHaveBeenCalledWith("DEF456");
  });

  it("shows unauthorized warning for unauthorized device", () => {
    render(<DeviceSection {...defaults} devices={[unauthorizedDevice]} selectedDevice="UNA789" expanded={true} />);
    expect(screen.getByText(/Accept the USB debugging prompt/)).toBeInTheDocument();
  });

  it("shows multi-device checkbox when multiple devices are connected", () => {
    render(<DeviceSection {...defaults} devices={[device1, device2]} selectedDevice="ABC123" expanded={true} />);
    expect(screen.getByText(/Install to all 2 connected devices/)).toBeInTheDocument();
  });

  it("hides multi-device checkbox for single device", () => {
    render(<DeviceSection {...defaults} devices={[device1]} selectedDevice="ABC123" expanded={true} />);
    expect(screen.queryByText(/Install to all/)).not.toBeInTheDocument();
  });

  it("calls onInstallAllDevicesChange when checkbox is toggled", () => {
    const onChange = vi.fn();
    render(<DeviceSection {...defaults} devices={[device1, device2]} selectedDevice="ABC123" onInstallAllDevicesChange={onChange} expanded={true} />);
    fireEvent.click(screen.getByRole("checkbox"));
    expect(onChange).toHaveBeenCalled();
  });

  // ── Action buttons ────────────────────────────────────────────────────

  it("disables Install button when canInstall is false", () => {
    render(<DeviceSection {...defaults} canInstall={false} />);
    expect(screen.getByText("Install").closest("button")).toBeDisabled();
  });

  it("enables Install button when canInstall is truthy", () => {
    render(<DeviceSection {...defaults} canInstall={true} />);
    expect(screen.getByText("Install").closest("button")).not.toBeDisabled();
  });

  it("calls onInstall(false) when Install is clicked", () => {
    const onInstall = vi.fn();
    render(<DeviceSection {...defaults} canInstall={true} onInstall={onInstall} />);
    fireEvent.click(screen.getByText("Install"));
    expect(onInstall).toHaveBeenCalledWith(false);
  });

  it("calls onInstall(true) when Install & Run is clicked", () => {
    const onInstall = vi.fn();
    render(<DeviceSection {...defaults} canInstall={true} onInstall={onInstall} />);
    fireEvent.click(screen.getByText("Install & Run"));
    expect(onInstall).toHaveBeenCalledWith(true);
  });

  it("shows 'Installing...' text when isInstalling is true", () => {
    render(<DeviceSection {...defaults} isInstalling={true} />);
    expect(screen.getByText("Installing...")).toBeInTheDocument();
  });

  it("disables Launch/Stop/Uninstall when no package name and no device", () => {
    render(<DeviceSection {...defaults} packageName="" selectedDevice="" />);
    expect(screen.getByText("Launch").closest("button")).toBeDisabled();
    expect(screen.getByText("Stop").closest("button")).toBeDisabled();
    expect(screen.getByText("Uninstall").closest("button")).toBeDisabled();
  });

  it("enables Launch/Stop/Uninstall when package name and device are set", () => {
    render(<DeviceSection {...defaults} packageName="com.example" selectedDevice="ABC123" devices={[device1]} />);
    expect(screen.getByText("Launch").closest("button")).not.toBeDisabled();
    expect(screen.getByText("Stop").closest("button")).not.toBeDisabled();
    expect(screen.getByText("Uninstall").closest("button")).not.toBeDisabled();
  });

  it("calls onLaunch when Launch is clicked", () => {
    const onLaunch = vi.fn();
    render(<DeviceSection {...defaults} packageName="com.example" selectedDevice="ABC123" devices={[device1]} onLaunch={onLaunch} />);
    fireEvent.click(screen.getByText("Launch"));
    expect(onLaunch).toHaveBeenCalledOnce();
  });

  it("calls onStopApp when Stop is clicked", () => {
    const onStopApp = vi.fn();
    render(<DeviceSection {...defaults} packageName="com.example" selectedDevice="ABC123" devices={[device1]} onStopApp={onStopApp} />);
    fireEvent.click(screen.getByText("Stop"));
    expect(onStopApp).toHaveBeenCalledOnce();
  });

  it("calls onUninstall when Uninstall is clicked", () => {
    const onUninstall = vi.fn();
    render(<DeviceSection {...defaults} packageName="com.example" selectedDevice="ABC123" devices={[device1]} onUninstall={onUninstall} />);
    fireEvent.click(screen.getByText("Uninstall"));
    expect(onUninstall).toHaveBeenCalledOnce();
  });

  it("calls onRefreshDevices when refresh button is clicked", () => {
    const onRefresh = vi.fn();
    render(<DeviceSection {...defaults} onRefreshDevices={onRefresh} expanded={true} />);
    fireEvent.click(screen.getByTitle("Refresh devices"));
    expect(onRefresh).toHaveBeenCalledOnce();
  });

  it("calls onDetectAdb when auto-detect button is clicked", () => {
    const onDetect = vi.fn();
    render(<DeviceSection {...defaults} onDetectAdb={onDetect} expanded={true} />);
    fireEvent.click(screen.getByTitle("Auto-detect ADB"));
    expect(onDetect).toHaveBeenCalledOnce();
  });

  it("disables refresh button when loadingDevices is true", () => {
    render(<DeviceSection {...defaults} loadingDevices={true} expanded={true} />);
    expect(screen.getByTitle("Refresh devices")).toBeDisabled();
  });

  it("disables refresh button when adbPath is empty", () => {
    render(<DeviceSection {...defaults} adbPath="" expanded={true} />);
    expect(screen.getByTitle("Refresh devices")).toBeDisabled();
  });

  // ── Wireless ADB ──────────────────────────────────────────────────────

  it("shows WiFi toggle button when expanded", () => {
    render(<DeviceSection {...defaults} expanded={true} />);
    expect(screen.getByTitle("Wireless ADB (WiFi)")).toBeInTheDocument();
  });

  it("does not show WiFi panel when wifiExpanded is false", () => {
    render(<DeviceSection {...defaults} expanded={true} />);
    expect(screen.queryByText("Wireless ADB (Android 11+)")).not.toBeInTheDocument();
  });

  it("shows WiFi panel when wifiExpanded is true", () => {
    const wireless = { ...wirelessDefaults, wifiExpanded: true };
    render(<DeviceSection {...defaults} expanded={true} wireless={wireless} />);
    expect(screen.getByText("Wireless ADB (Android 11+)")).toBeInTheDocument();
  });

  it("shows Pair and Connect sections in WiFi panel", () => {
    const wireless = { ...wirelessDefaults, wifiExpanded: true };
    render(<DeviceSection {...defaults} expanded={true} wireless={wireless} />);
    expect(screen.getByText("1. Pair (first time only)")).toBeInTheDocument();
    expect(screen.getByText("2. Connect")).toBeInTheDocument();
  });

  it("shows Pair button disabled when canPair is false", () => {
    const wireless = { ...wirelessDefaults, wifiExpanded: true, canPair: false };
    render(<DeviceSection {...defaults} expanded={true} wireless={wireless} />);
    expect(screen.getByText("Pair").closest("button")).toBeDisabled();
  });

  it("shows Connect button disabled when canConnect is false", () => {
    const wireless = { ...wirelessDefaults, wifiExpanded: true, canConnect: false };
    render(<DeviceSection {...defaults} expanded={true} wireless={wireless} />);
    expect(screen.getByText("Connect").closest("button")).toBeDisabled();
  });

  it("enables Pair button when canPair is true", () => {
    const wireless = { ...wirelessDefaults, wifiExpanded: true, canPair: true };
    render(<DeviceSection {...defaults} expanded={true} wireless={wireless} />);
    expect(screen.getByText("Pair").closest("button")).not.toBeDisabled();
  });

  it("enables Connect button when canConnect is true", () => {
    const wireless = { ...wirelessDefaults, wifiExpanded: true, canConnect: true };
    render(<DeviceSection {...defaults} expanded={true} wireless={wireless} />);
    expect(screen.getByText("Connect").closest("button")).not.toBeDisabled();
  });

  it("calls pair when Pair button is clicked", () => {
    const pair = vi.fn();
    const wireless = { ...wirelessDefaults, wifiExpanded: true, canPair: true, pair };
    render(<DeviceSection {...defaults} expanded={true} wireless={wireless} />);
    fireEvent.click(screen.getByText("Pair"));
    expect(pair).toHaveBeenCalledOnce();
  });

  it("calls connect when Connect button is clicked", () => {
    const connect = vi.fn();
    const wireless = { ...wirelessDefaults, wifiExpanded: true, canConnect: true, connect };
    render(<DeviceSection {...defaults} expanded={true} wireless={wireless} />);
    fireEvent.click(screen.getByText("Connect"));
    expect(connect).toHaveBeenCalledOnce();
  });

  it("shows Disconnect button for wireless devices", () => {
    const wireless = { ...wirelessDefaults, disconnect: vi.fn() };
    render(<DeviceSection {...defaults} devices={[wirelessDevice]} selectedDevice="192.168.1.100:5555" expanded={true} wireless={wireless} />);
    expect(screen.getByText(/Disconnect 192\.168\.1\.100:5555/)).toBeInTheDocument();
  });

  it("calls disconnect when Disconnect button is clicked", () => {
    const disconnect = vi.fn();
    const wireless = { ...wirelessDefaults, disconnect };
    render(<DeviceSection {...defaults} devices={[wirelessDevice]} selectedDevice="192.168.1.100:5555" expanded={true} wireless={wireless} />);
    fireEvent.click(screen.getByText(/Disconnect 192\.168\.1\.100:5555/));
    expect(disconnect).toHaveBeenCalledWith("192.168.1.100:5555");
  });

  it("does not show Disconnect button for USB devices", () => {
    render(<DeviceSection {...defaults} devices={[device1]} selectedDevice="ABC123" expanded={true} />);
    expect(screen.queryByText(/Disconnect/)).not.toBeInTheDocument();
  });

  it("shows 'Pairing...' when isPairing is true", () => {
    const wireless = { ...wirelessDefaults, wifiExpanded: true, isPairing: true };
    render(<DeviceSection {...defaults} expanded={true} wireless={wireless} />);
    expect(screen.getByText("Pairing...")).toBeInTheDocument();
  });

  it("shows 'Connecting...' when isConnecting is true", () => {
    const wireless = { ...wirelessDefaults, wifiExpanded: true, isConnecting: true };
    render(<DeviceSection {...defaults} expanded={true} wireless={wireless} />);
    expect(screen.getByText("Connecting...")).toBeInTheDocument();
  });

  it("calls setWifiExpanded when WiFi toggle is clicked", () => {
    const setWifiExpanded = vi.fn();
    const wireless = { ...wirelessDefaults, setWifiExpanded };
    render(<DeviceSection {...defaults} expanded={true} wireless={wireless} />);
    fireEvent.click(screen.getByTitle("Wireless ADB (WiFi)"));
    expect(setWifiExpanded).toHaveBeenCalledWith(true);
  });

  // ── Network Discovery ─────────────────────────────────────────────────

  it("shows Scan button in WiFi panel", () => {
    const wireless = { ...wirelessDefaults, wifiExpanded: true };
    render(<DeviceSection {...defaults} expanded={true} wireless={wireless} />);
    expect(screen.getByText("Scan")).toBeInTheDocument();
  });

  it("calls scan when Scan button is clicked", () => {
    const scan = vi.fn();
    const wireless = { ...wirelessDefaults, wifiExpanded: true, scan };
    render(<DeviceSection {...defaults} expanded={true} wireless={wireless} />);
    fireEvent.click(screen.getByText("Scan"));
    expect(scan).toHaveBeenCalledOnce();
  });

  it("shows 'Scanning...' when isScanning is true", () => {
    const wireless = { ...wirelessDefaults, wifiExpanded: true, isScanning: true };
    render(<DeviceSection {...defaults} expanded={true} wireless={wireless} />);
    expect(screen.getByText("Scanning...")).toBeInTheDocument();
  });

  it("shows mDNS not supported warning when mdnsSupported is false", () => {
    const wireless = { ...wirelessDefaults, wifiExpanded: true, mdnsSupported: false as boolean | null };
    render(<DeviceSection {...defaults} expanded={true} wireless={wireless} />);
    expect(screen.getByText(/mDNS not supported/)).toBeInTheDocument();
  });

  it("shows discovered devices when available", () => {
    const wireless = {
      ...wirelessDefaults,
      wifiExpanded: true,
      discoveredDevices: [
        { name: "adb-PIXEL7", service_type: "_adb-tls-connect._tcp.", ip_port: "192.168.1.42:43567" },
      ],
    };
    render(<DeviceSection {...defaults} expanded={true} wireless={wireless} />);
    expect(screen.getByText("adb-PIXEL7")).toBeInTheDocument();
    expect(screen.getByText("192.168.1.42:43567")).toBeInTheDocument();
  });

  it("shows type badges for connect and pairing services", () => {
    const wireless = {
      ...wirelessDefaults,
      wifiExpanded: true,
      discoveredDevices: [
        { name: "adb-PIXEL7", service_type: "_adb-tls-connect._tcp.", ip_port: "192.168.1.42:43567" },
        { name: "adb-GALAXY", service_type: "_adb-tls-pairing._tcp.", ip_port: "192.168.1.42:37215" },
      ],
    };
    render(<DeviceSection {...defaults} expanded={true} wireless={wireless} />);
    const badges = screen.getAllByText(/^(Connect|Pair)$/);
    const badgeTexts = badges.map((b) => b.textContent);
    expect(badgeTexts).toContain("Connect");
    expect(badgeTexts).toContain("Pair");
  });

  it("calls selectDiscovered when Use button is clicked", () => {
    const selectDiscovered = vi.fn();
    const svc = { name: "adb-PIXEL7", service_type: "_adb-tls-connect._tcp.", ip_port: "192.168.1.42:43567" };
    const wireless = {
      ...wirelessDefaults,
      wifiExpanded: true,
      discoveredDevices: [svc],
      selectDiscovered,
    };
    render(<DeviceSection {...defaults} expanded={true} wireless={wireless} />);
    fireEvent.click(screen.getByText("Use"));
    expect(selectDiscovered).toHaveBeenCalledWith(svc);
  });

  it("shows hint text when no devices discovered and mdns not yet checked", () => {
    const wireless = { ...wirelessDefaults, wifiExpanded: true, discoveredDevices: [], mdnsSupported: null };
    render(<DeviceSection {...defaults} expanded={true} wireless={wireless} />);
    expect(screen.getByText(/Click Scan to discover/)).toBeInTheDocument();
  });
});

