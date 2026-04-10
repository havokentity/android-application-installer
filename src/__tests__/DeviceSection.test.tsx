// ─── Tests for src/components/DeviceSection.tsx ───────────────────────────────
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { DeviceSection } from "../components/DeviceSection";
import type { DeviceInfo } from "../types";

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
  onUninstall: vi.fn(),
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

  it("disables Launch/Uninstall when no package name and no device", () => {
    render(<DeviceSection {...defaults} packageName="" selectedDevice="" />);
    expect(screen.getByText("Launch").closest("button")).toBeDisabled();
    expect(screen.getByText("Uninstall").closest("button")).toBeDisabled();
  });

  it("enables Launch/Uninstall when package name and device are set", () => {
    render(<DeviceSection {...defaults} packageName="com.example" selectedDevice="ABC123" devices={[device1]} />);
    expect(screen.getByText("Launch").closest("button")).not.toBeDisabled();
    expect(screen.getByText("Uninstall").closest("button")).not.toBeDisabled();
  });

  it("calls onLaunch when Launch is clicked", () => {
    const onLaunch = vi.fn();
    render(<DeviceSection {...defaults} packageName="com.example" selectedDevice="ABC123" devices={[device1]} onLaunch={onLaunch} />);
    fireEvent.click(screen.getByText("Launch"));
    expect(onLaunch).toHaveBeenCalledOnce();
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
});

