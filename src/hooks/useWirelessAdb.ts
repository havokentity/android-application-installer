// ─── Wireless ADB Hook ───────────────────────────────────────────────────────
import { useState, useCallback } from "react";
import type { LogEntry, MdnsService } from "../types";
import type { ToastLevel } from "../components/Toast";
import * as api from "../api";

/** Check if a device serial is an IP:port format wireless connection. */
export function isIpPortDevice(serial: string): boolean {
  return /^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}:\d{1,5}$/.test(serial);
}

/** Check if a device serial is an mDNS auto-discovered wireless connection. */
export function isMdnsDevice(serial: string): boolean {
  return /^adb-.*\._adb-tls-.*\._tcp/.test(serial);
}

/** Check if a device serial is any kind of wireless connection (IP:port or mDNS). */
export function isWirelessDevice(serial: string): boolean {
  return isIpPortDevice(serial) || isMdnsDevice(serial);
}

/** A device entry that may carry an alternate serial from deduplication. */
export interface DeduplicatedDevice extends import("../types").DeviceInfo {
  /** The alternate serial (mDNS or IP:port twin) if both transports are available. */
  alternateSerial?: string;
}

/**
 * Deduplicate devices that represent the same physical device connected via
 * multiple wireless transports (e.g. IP:port AND mDNS service name).
 * Groups by model+product when both are non-empty and both entries are wireless.
 * Returns one representative per physical device.
 *
 * Selection priority:
 *  1. Prefer the entry that is "device" (online) over one that is offline/unauthorized.
 *  2. When both have the same state, prefer IP:port (direct install, no Play Protect).
 *
 * The discarded twin's serial is stored as `alternateSerial` on the surviving entry.
 */
export function deduplicateDevices(devices: import("../types").DeviceInfo[]): DeduplicatedDevice[] {
  const result: DeduplicatedDevice[] = [];
  const consumed = new Set<string>();

  for (const d of devices) {
    if (consumed.has(d.serial)) continue;

    if (isWirelessDevice(d.serial) && d.model && d.product) {
      const twin = devices.find(
        (o) =>
          o.serial !== d.serial &&
          !consumed.has(o.serial) &&
          isWirelessDevice(o.serial) &&
          o.model === d.model &&
          o.product === d.product,
      );

      if (twin) {
        let preferred: import("../types").DeviceInfo;
        let discarded: import("../types").DeviceInfo;

        // Priority 1: prefer the online ("device") entry
        if (d.state === "device" && twin.state !== "device") {
          preferred = d;
          discarded = twin;
        } else if (twin.state === "device" && d.state !== "device") {
          preferred = twin;
          discarded = d;
        } else {
          // Priority 2: same state — prefer IP:port (direct install)
          preferred = isIpPortDevice(d.serial) ? d : twin;
          discarded = isIpPortDevice(d.serial) ? twin : d;
        }

        consumed.add(d.serial);
        consumed.add(twin.serial);
        result.push({ ...preferred, alternateSerial: discarded.serial });
        continue;
      }
    }

    consumed.add(d.serial);
    result.push({ ...d });
  }

  return result;
}

/** Validate an IPv4 address. */
export function isValidIp(ip: string): boolean {
  const parts = ip.split(".");
  if (parts.length !== 4) return false;
  return parts.every((p) => {
    const n = parseInt(p, 10);
    return !isNaN(n) && n >= 0 && n <= 255 && p === String(n);
  });
}

/** Validate a port number (1–65535). */
export function isValidPort(port: string): boolean {
  const n = parseInt(port, 10);
  return !isNaN(n) && n >= 1 && n <= 65535 && port === String(n);
}

/** Validate a 6-digit pairing code. */
export function isValidPairingCode(code: string): boolean {
  return /^\d{6}$/.test(code);
}

export interface WirelessAdbState {
  wifiExpanded: boolean;
  setWifiExpanded: (v: boolean) => void;
  pairIp: string;
  setPairIp: (v: string) => void;
  pairPort: string;
  setPairPort: (v: string) => void;
  pairingCode: string;
  setPairingCode: (v: string) => void;
  connectIp: string;
  setConnectIp: (v: string) => void;
  connectPort: string;
  setConnectPort: (v: string) => void;
  isPairing: boolean;
  isConnecting: boolean;
  isDisconnecting: boolean;
  canPair: boolean;
  canConnect: boolean;
  pair: () => Promise<void>;
  connect: () => Promise<void>;
  disconnect: (serial: string) => Promise<void>;
  cancelWirelessOp: () => Promise<void>;
  // pairing prompt after connect failure
  needsPairing: boolean;
  promptPairing: () => void;
  // mDNS discovery
  discoveredDevices: MdnsService[];
  isScanning: boolean;
  mdnsSupported: boolean | null;
  scan: () => Promise<void>;
  selectDiscovered: (svc: MdnsService) => void;
}

interface UseWirelessAdbOptions {
  adbPath: string;
  addLog: (level: LogEntry["level"], message: string) => void;
  addToast: (message: string, level: ToastLevel) => void;
  /** Called after a successful connect, pair, or disconnect so the consumer can refresh devices. */
  onDeviceChange?: () => void;
}

export function useWirelessAdb({ adbPath, addLog, addToast, onDeviceChange }: UseWirelessAdbOptions): WirelessAdbState {
  const [wifiExpanded, setWifiExpanded] = useState(false);
  const [pairIp, setPairIp] = useState("");
  const [pairPort, setPairPort] = useState("");
  const [pairingCode, setPairingCode] = useState("");
  const [connectIp, setConnectIp] = useState("");
  const [connectPort, setConnectPort] = useState("");
  const [isPairing, setIsPairing] = useState(false);
  const [isConnecting, setIsConnecting] = useState(false);
  const [isDisconnecting, setIsDisconnecting] = useState(false);
  const [needsPairing, setNeedsPairing] = useState(false);
  const [discoveredDevices, setDiscoveredDevices] = useState<MdnsService[]>([]);
  const [isScanning, setIsScanning] = useState(false);
  const [mdnsSupported, setMdnsSupported] = useState<boolean | null>(null);

  const canPair = !!(adbPath && isValidIp(pairIp) && isValidPort(pairPort) && isValidPairingCode(pairingCode) && !isPairing);
  const canConnect = !!(adbPath && isValidIp(connectIp) && isValidPort(connectPort) && !isConnecting);

  const pair = useCallback(async () => {
    if (!canPair) return;
    const ipPort = `${pairIp}:${pairPort}`;
    setIsPairing(true);
    await api.setCancelFlag(false);
    addLog("info", `Pairing with ${ipPort}...`);
    try {
      const result = await api.adbPair(adbPath, ipPort, pairingCode);
      addLog("success", result);
      addToast("Device paired successfully", "success");
      setConnectIp(pairIp);
      setPairingCode("");
      setNeedsPairing(false);
      onDeviceChange?.();
    } catch (e) {
      const msg = String(e);
      if (msg.includes("cancelled")) {
        addLog("warning", "Pairing cancelled.");
        addToast("Pairing cancelled", "warning");
      } else {
        addLog("error", `Pairing failed: ${e}`);
        addToast(`Pairing failed: ${e}`, "error");
      }
    } finally {
      setIsPairing(false);
    }
  }, [canPair, pairIp, pairPort, pairingCode, adbPath, addLog, addToast, onDeviceChange]);

  const connect = useCallback(async () => {
    if (!canConnect) return;
    const ipPort = `${connectIp}:${connectPort}`;
    setIsConnecting(true);
    await api.setCancelFlag(false);
    addLog("info", `Connecting to ${ipPort}...`);
    try {
      const result = await api.adbConnect(adbPath, ipPort);
      addLog("success", result);
      addToast("Connected wirelessly", "success");
      setNeedsPairing(false);
      onDeviceChange?.();
    } catch (e) {
      const msg = String(e);
      if (msg.includes("cancelled")) {
        addLog("warning", "Connection cancelled.");
        addToast("Connection cancelled", "warning");
      } else {
        addLog("error", `Connection failed: ${e}`);
        addToast(`Connection failed: ${e}`, "error");
        // If the failure looks like a pairing issue, prompt user to pair first
        if (msg.includes("failed to connect") || msg.includes("connection refused") || msg.includes("no response")) {
          setNeedsPairing(true);
        }
      }
    } finally {
      setIsConnecting(false);
    }
  }, [canConnect, connectIp, connectPort, adbPath, addLog, addToast, onDeviceChange]);

  const disconnect = useCallback(async (serial: string) => {
    setIsDisconnecting(true);
    await api.setCancelFlag(false);
    addLog("info", `Disconnecting ${serial}...`);
    try {
      const result = await api.adbDisconnect(adbPath, serial);
      addLog("success", result);
      addToast("Device disconnected", "info");
      onDeviceChange?.();
    } catch (e) {
      const msg = String(e);
      if (msg.includes("cancelled")) {
        addLog("warning", "Disconnect cancelled.");
      } else {
        addLog("error", `Disconnect failed: ${e}`);
        addToast(`Disconnect failed: ${e}`, "error");
      }
    } finally {
      setIsDisconnecting(false);
    }
  }, [adbPath, addLog, addToast, onDeviceChange]);

  const cancelWirelessOp = useCallback(async () => {
    await api.setCancelFlag(true);
    addLog("info", "Cancelling wireless operation...");
  }, [addLog]);

  const scan = useCallback(async () => {
    if (!adbPath || isScanning) return;
    setIsScanning(true);
    await api.setCancelFlag(false);
    try {
      // Check mDNS support on first scan
      if (mdnsSupported === null) {
        const supported = await api.adbMdnsCheck(adbPath);
        setMdnsSupported(supported);
        if (!supported) {
          addLog("warning", "mDNS discovery not supported by this ADB version. Update platform-tools to 31+.");
          addToast("mDNS not supported — update ADB to 31+", "warning");
          return;
        }
      }
      const services = await api.adbMdnsServices(adbPath);
      setDiscoveredDevices(services);
      // Count unique devices (by base name) not raw service entries
      const uniqueNames = new Set(services.map((s) => s.name.replace(/-[a-zA-Z0-9]{4,}$/, "")));
      const deviceCount = uniqueNames.size;
      if (deviceCount === 0) {
        addLog("info", "No wireless devices discovered on the network.");
      } else {
        addLog("info", `Found ${deviceCount} device${deviceCount === 1 ? "" : "s"} on the network (${services.length} service${services.length === 1 ? "" : "s"}).`);
      }
      // Refresh devices so any mDNS-discovered entries become visible
      onDeviceChange?.();
    } catch (e) {
      const msg = String(e);
      if (msg.includes("cancelled")) {
        addLog("warning", "Scan cancelled.");
      } else {
        addLog("warning", `mDNS scan failed: ${e}`);
        setMdnsSupported(false);
      }
    } finally {
      setIsScanning(false);
    }
  }, [adbPath, isScanning, mdnsSupported, addLog, addToast, onDeviceChange]);

  /** Copy the connect IP into the pairing fields so the user only needs port + code. */
  const promptPairing = useCallback(() => {
    setPairIp(connectIp);
    setPairPort("");
    setPairingCode("");
    setNeedsPairing(false);
    addLog("info", `Pairing fields pre-filled with ${connectIp}. Enter the pairing port and code from your device.`);
  }, [connectIp, addLog]);

  /** Auto-fill IP and port from a discovered service for pair or connect. */
  const selectDiscovered = useCallback((svc: MdnsService) => {
    const [ip, port] = svc.ip_port.split(":");
    if (svc.service_type.includes("pairing")) {
      setPairIp(ip);
      setPairPort(port || "");
      addLog("info", `Selected pairing service: ${svc.name} (${svc.ip_port})`);
    } else {
      setConnectIp(ip);
      setConnectPort(port || "");
      addLog("info", `Selected connect service: ${svc.name} (${svc.ip_port})`);
    }
  }, [addLog]);

  return {
    wifiExpanded, setWifiExpanded,
    pairIp, setPairIp, pairPort, setPairPort, pairingCode, setPairingCode,
    connectIp, setConnectIp, connectPort, setConnectPort,
    isPairing, isConnecting, isDisconnecting,
    canPair, canConnect,
    pair, connect, disconnect, cancelWirelessOp,
    needsPairing, promptPairing,
    discoveredDevices, isScanning, mdnsSupported, scan, selectDiscovered,
  };
}

