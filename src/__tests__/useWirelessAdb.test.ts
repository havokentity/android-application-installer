// ─── Tests for src/hooks/useWirelessAdb.ts ────────────────────────────────────
import { describe, it, expect, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import {
  isWirelessDevice,
  isIpPortDevice,
  isMdnsDevice,
  deduplicateDevices,
  isValidIp,
  isValidPort,
  isValidPairingCode,
  useWirelessAdb,
} from "../hooks/useWirelessAdb";
import type { DeviceInfo } from "../types";

// ─── Utility validators ──────────────────────────────────────────────────────

describe("isIpPortDevice", () => {
  it("returns true for IP:port serial", () => {
    expect(isIpPortDevice("192.168.1.100:5555")).toBe(true);
  });

  it("returns false for mDNS serial", () => {
    expect(isIpPortDevice("adb-10BF190RC9001UZ-jvFPtf._adb-tls-connect._tcp")).toBe(false);
  });

  it("returns false for USB serial", () => {
    expect(isIpPortDevice("ABC123DEF456")).toBe(false);
  });
});

describe("isMdnsDevice", () => {
  it("returns true for mDNS connect serial", () => {
    expect(isMdnsDevice("adb-10BF190RC9001UZ-jvFPtf._adb-tls-connect._tcp")).toBe(true);
  });

  it("returns true for mDNS pairing serial", () => {
    expect(isMdnsDevice("adb-PIXEL7-abcd._adb-tls-pairing._tcp")).toBe(true);
  });

  it("returns false for IP:port serial", () => {
    expect(isMdnsDevice("192.168.1.100:5555")).toBe(false);
  });

  it("returns false for USB serial", () => {
    expect(isMdnsDevice("ABC123DEF456")).toBe(false);
  });
});

describe("isWirelessDevice", () => {
  it("returns true for IP:port serial", () => {
    expect(isWirelessDevice("192.168.1.100:5555")).toBe(true);
  });

  it("returns true for another IP:port", () => {
    expect(isWirelessDevice("10.0.0.1:37123")).toBe(true);
  });

  it("returns true for mDNS connect serial", () => {
    expect(isWirelessDevice("adb-10BF190RC9001UZ-jvFPtf._adb-tls-connect._tcp")).toBe(true);
  });

  it("returns true for mDNS pairing serial", () => {
    expect(isWirelessDevice("adb-PIXEL7-abcd._adb-tls-pairing._tcp")).toBe(true);
  });

  it("returns false for USB serial", () => {
    expect(isWirelessDevice("ABC123DEF456")).toBe(false);
  });

  it("returns false for empty string", () => {
    expect(isWirelessDevice("")).toBe(false);
  });

  it("returns false for IP without port", () => {
    expect(isWirelessDevice("192.168.1.100")).toBe(false);
  });

  it("returns false for hostname:port", () => {
    expect(isWirelessDevice("mydevice:5555")).toBe(false);
  });
});

// ─── deduplicateDevices ──────────────────────────────────────────────────────

describe("deduplicateDevices", () => {
  const makeDevice = (serial: string, model = "", product = ""): DeviceInfo => ({
    serial, state: "device", model, product, transport_id: "",
  });

  it("returns same list when no duplicates", () => {
    const devices = [
      makeDevice("ABC123", "Pixel 7", "panther"),
      makeDevice("192.168.1.100:5555", "Galaxy S24", "dm3q"),
    ];
    expect(deduplicateDevices(devices)).toHaveLength(2);
  });

  it("deduplicates IP:port and mDNS entries for the same device", () => {
    const devices = [
      makeDevice("192.168.0.23:38355", "I2401", "ossi"),
      makeDevice("adb-10BF190RC9001UZ-jvFPtf._adb-tls-connect._tcp", "I2401", "ossi"),
    ];
    const result = deduplicateDevices(devices);
    expect(result).toHaveLength(1);
    // Prefers IP:port
    expect(result[0].serial).toBe("192.168.0.23:38355");
  });

  it("prefers IP:port over mDNS regardless of order", () => {
    const devices = [
      makeDevice("adb-SERIAL._adb-tls-connect._tcp", "I2401", "ossi"),
      makeDevice("192.168.0.23:38355", "I2401", "ossi"),
    ];
    const result = deduplicateDevices(devices);
    expect(result).toHaveLength(1);
    expect(result[0].serial).toBe("192.168.0.23:38355");
  });

  it("does not deduplicate USB and wireless devices with same model", () => {
    const devices = [
      makeDevice("ABC123", "Pixel 7", "panther"),
      makeDevice("192.168.1.100:5555", "Pixel 7", "panther"),
    ];
    const result = deduplicateDevices(devices);
    expect(result).toHaveLength(2);
  });

  it("does not deduplicate wireless devices with different models", () => {
    const devices = [
      makeDevice("192.168.0.23:38355", "I2401", "ossi"),
      makeDevice("adb-SERIAL._adb-tls-connect._tcp", "Pixel 7", "panther"),
    ];
    expect(deduplicateDevices(devices)).toHaveLength(2);
  });

  it("does not deduplicate if model or product is empty", () => {
    const devices = [
      makeDevice("192.168.0.23:38355", "", ""),
      makeDevice("adb-SERIAL._adb-tls-connect._tcp", "", ""),
    ];
    expect(deduplicateDevices(devices)).toHaveLength(2);
  });

  it("handles mix of USB, IP:port, and mDNS correctly", () => {
    const devices = [
      makeDevice("USB123", "Pixel 7", "panther"),
      makeDevice("192.168.0.23:38355", "I2401", "ossi"),
      makeDevice("adb-10BF._adb-tls-connect._tcp", "I2401", "ossi"),
    ];
    const result = deduplicateDevices(devices);
    expect(result).toHaveLength(2);
    expect(result.map((d) => d.serial)).toContain("USB123");
    expect(result.map((d) => d.serial)).toContain("192.168.0.23:38355");
  });

  it("stores alternateSerial on deduplicated entry (IP:port preferred)", () => {
    const devices = [
      makeDevice("192.168.0.23:38355", "I2401", "ossi"),
      makeDevice("adb-10BF190RC9001UZ-jvFPtf._adb-tls-connect._tcp", "I2401", "ossi"),
    ];
    const result = deduplicateDevices(devices);
    expect(result).toHaveLength(1);
    expect(result[0].serial).toBe("192.168.0.23:38355");
    expect(result[0].alternateSerial).toBe("adb-10BF190RC9001UZ-jvFPtf._adb-tls-connect._tcp");
  });

  it("stores alternateSerial when mDNS comes first", () => {
    const devices = [
      makeDevice("adb-SERIAL._adb-tls-connect._tcp", "I2401", "ossi"),
      makeDevice("192.168.0.23:38355", "I2401", "ossi"),
    ];
    const result = deduplicateDevices(devices);
    expect(result).toHaveLength(1);
    expect(result[0].serial).toBe("192.168.0.23:38355");
    expect(result[0].alternateSerial).toBe("adb-SERIAL._adb-tls-connect._tcp");
  });

  it("does not set alternateSerial when no twin exists", () => {
    const devices = [
      makeDevice("192.168.0.23:38355", "I2401", "ossi"),
    ];
    const result = deduplicateDevices(devices);
    expect(result).toHaveLength(1);
    expect(result[0].alternateSerial).toBeUndefined();
  });

  it("does not set alternateSerial on USB devices", () => {
    const devices = [
      makeDevice("USB123", "Pixel 7", "panther"),
    ];
    const result = deduplicateDevices(devices);
    expect(result).toHaveLength(1);
    expect(result[0].alternateSerial).toBeUndefined();
  });
});

describe("isValidIp", () => {
  it("accepts valid IPv4", () => {
    expect(isValidIp("192.168.1.100")).toBe(true);
  });

  it("accepts 0.0.0.0", () => {
    expect(isValidIp("0.0.0.0")).toBe(true);
  });

  it("accepts 255.255.255.255", () => {
    expect(isValidIp("255.255.255.255")).toBe(true);
  });

  it("rejects too few octets", () => {
    expect(isValidIp("192.168.1")).toBe(false);
  });

  it("rejects octet > 255", () => {
    expect(isValidIp("192.168.1.256")).toBe(false);
  });

  it("rejects non-numeric", () => {
    expect(isValidIp("abc.def.ghi.jkl")).toBe(false);
  });

  it("rejects leading zeros", () => {
    expect(isValidIp("192.168.01.100")).toBe(false);
  });

  it("rejects empty string", () => {
    expect(isValidIp("")).toBe(false);
  });
});

describe("isValidPort", () => {
  it("accepts valid port", () => {
    expect(isValidPort("5555")).toBe(true);
  });

  it("accepts port 1", () => {
    expect(isValidPort("1")).toBe(true);
  });

  it("accepts port 65535", () => {
    expect(isValidPort("65535")).toBe(true);
  });

  it("rejects port 0", () => {
    expect(isValidPort("0")).toBe(false);
  });

  it("rejects port > 65535", () => {
    expect(isValidPort("65536")).toBe(false);
  });

  it("rejects non-numeric", () => {
    expect(isValidPort("abc")).toBe(false);
  });

  it("rejects empty string", () => {
    expect(isValidPort("")).toBe(false);
  });

  it("rejects leading zeros", () => {
    expect(isValidPort("05555")).toBe(false);
  });
});

describe("isValidPairingCode", () => {
  it("accepts 6-digit code", () => {
    expect(isValidPairingCode("123456")).toBe(true);
  });

  it("rejects 5-digit code", () => {
    expect(isValidPairingCode("12345")).toBe(false);
  });

  it("rejects 7-digit code", () => {
    expect(isValidPairingCode("1234567")).toBe(false);
  });

  it("rejects non-numeric", () => {
    expect(isValidPairingCode("abcdef")).toBe(false);
  });

  it("rejects empty string", () => {
    expect(isValidPairingCode("")).toBe(false);
  });
});

// ─── useWirelessAdb hook ─────────────────────────────────────────────────────

describe("useWirelessAdb", () => {
  const defaults = {
    adbPath: "/path/to/adb",
    addLog: vi.fn(),
    addToast: vi.fn(),
  };

  it("starts with WiFi panel collapsed", () => {
    const { result } = renderHook(() => useWirelessAdb(defaults));
    expect(result.current.wifiExpanded).toBe(false);
  });

  it("starts with empty form fields", () => {
    const { result } = renderHook(() => useWirelessAdb(defaults));
    expect(result.current.pairIp).toBe("");
    expect(result.current.pairPort).toBe("");
    expect(result.current.pairingCode).toBe("");
    expect(result.current.connectIp).toBe("");
    expect(result.current.connectPort).toBe("");
  });

  it("canPair is false when fields are empty", () => {
    const { result } = renderHook(() => useWirelessAdb(defaults));
    expect(result.current.canPair).toBe(false);
  });

  it("canConnect is false when fields are empty", () => {
    const { result } = renderHook(() => useWirelessAdb(defaults));
    expect(result.current.canConnect).toBe(false);
  });

  it("canPair is true when all pair fields are valid", () => {
    const { result } = renderHook(() => useWirelessAdb(defaults));
    act(() => {
      result.current.setPairIp("192.168.1.100");
      result.current.setPairPort("37123");
      result.current.setPairingCode("123456");
    });
    expect(result.current.canPair).toBe(true);
  });

  it("canConnect is true when IP and port are valid", () => {
    const { result } = renderHook(() => useWirelessAdb(defaults));
    act(() => {
      result.current.setConnectIp("192.168.1.100");
      result.current.setConnectPort("5555");
    });
    expect(result.current.canConnect).toBe(true);
  });

  it("canPair is false with invalid IP", () => {
    const { result } = renderHook(() => useWirelessAdb(defaults));
    act(() => {
      result.current.setPairIp("999.999.999.999");
      result.current.setPairPort("37123");
      result.current.setPairingCode("123456");
    });
    expect(result.current.canPair).toBe(false);
  });

  it("canPair is false without adbPath", () => {
    const { result } = renderHook(() => useWirelessAdb({ ...defaults, adbPath: "" }));
    act(() => {
      result.current.setPairIp("192.168.1.100");
      result.current.setPairPort("37123");
      result.current.setPairingCode("123456");
    });
    expect(result.current.canPair).toBe(false);
  });

  it("toggles wifiExpanded", () => {
    const { result } = renderHook(() => useWirelessAdb(defaults));
    act(() => result.current.setWifiExpanded(true));
    expect(result.current.wifiExpanded).toBe(true);
    act(() => result.current.setWifiExpanded(false));
    expect(result.current.wifiExpanded).toBe(false);
  });

  it("isPairing and isConnecting start as false", () => {
    const { result } = renderHook(() => useWirelessAdb(defaults));
    expect(result.current.isPairing).toBe(false);
    expect(result.current.isConnecting).toBe(false);
  });

  it("discoveredDevices starts empty", () => {
    const { result } = renderHook(() => useWirelessAdb(defaults));
    expect(result.current.discoveredDevices).toEqual([]);
  });

  it("isScanning starts as false", () => {
    const { result } = renderHook(() => useWirelessAdb(defaults));
    expect(result.current.isScanning).toBe(false);
  });

  it("mdnsSupported starts as null", () => {
    const { result } = renderHook(() => useWirelessAdb(defaults));
    expect(result.current.mdnsSupported).toBeNull();
  });

  it("selectDiscovered fills connect fields for connect service", () => {
    const addLog = vi.fn();
    const { result } = renderHook(() => useWirelessAdb({ ...defaults, addLog }));
    act(() => {
      result.current.selectDiscovered({
        name: "adb-PIXEL7",
        service_type: "_adb-tls-connect._tcp.",
        ip_port: "192.168.1.42:43567",
      });
    });
    expect(result.current.connectIp).toBe("192.168.1.42");
    expect(result.current.connectPort).toBe("43567");
  });

  it("selectDiscovered fills pair fields for pairing service", () => {
    const addLog = vi.fn();
    const { result } = renderHook(() => useWirelessAdb({ ...defaults, addLog }));
    act(() => {
      result.current.selectDiscovered({
        name: "adb-PIXEL7",
        service_type: "_adb-tls-pairing._tcp.",
        ip_port: "192.168.1.42:37215",
      });
    });
    expect(result.current.pairIp).toBe("192.168.1.42");
    expect(result.current.pairPort).toBe("37215");
  });

  it("needsPairing starts as false", () => {
    const { result } = renderHook(() => useWirelessAdb(defaults));
    expect(result.current.needsPairing).toBe(false);
  });

  it("promptPairing copies connectIp into pairIp and clears pair fields", () => {
    const addLog = vi.fn();
    const { result } = renderHook(() => useWirelessAdb({ ...defaults, addLog }));
    act(() => {
      result.current.setConnectIp("192.168.0.23");
      result.current.setConnectPort("38355");
    });
    act(() => {
      result.current.promptPairing();
    });
    expect(result.current.pairIp).toBe("192.168.0.23");
    expect(result.current.pairPort).toBe("");
    expect(result.current.pairingCode).toBe("");
  });
});

