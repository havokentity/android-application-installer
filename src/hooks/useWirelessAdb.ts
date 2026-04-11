// ─── Wireless ADB Hook ───────────────────────────────────────────────────────
import { useState, useCallback } from "react";
import type { LogEntry, MdnsService } from "../types";
import type { ToastLevel } from "../components/Toast";
import * as api from "../api";

/** Check if a device serial looks like a wireless device (IP:port format). */
export function isWirelessDevice(serial: string): boolean {
  return /^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}:\d{1,5}$/.test(serial);
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
}

export function useWirelessAdb({ adbPath, addLog, addToast }: UseWirelessAdbOptions): WirelessAdbState {
  const [wifiExpanded, setWifiExpanded] = useState(false);
  const [pairIp, setPairIp] = useState("");
  const [pairPort, setPairPort] = useState("");
  const [pairingCode, setPairingCode] = useState("");
  const [connectIp, setConnectIp] = useState("");
  const [connectPort, setConnectPort] = useState("");
  const [isPairing, setIsPairing] = useState(false);
  const [isConnecting, setIsConnecting] = useState(false);
  const [isDisconnecting, setIsDisconnecting] = useState(false);
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
  }, [canPair, pairIp, pairPort, pairingCode, adbPath, addLog, addToast]);

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
    } catch (e) {
      const msg = String(e);
      if (msg.includes("cancelled")) {
        addLog("warning", "Connection cancelled.");
        addToast("Connection cancelled", "warning");
      } else {
        addLog("error", `Connection failed: ${e}`);
        addToast(`Connection failed: ${e}`, "error");
      }
    } finally {
      setIsConnecting(false);
    }
  }, [canConnect, connectIp, connectPort, adbPath, addLog, addToast]);

  const disconnect = useCallback(async (serial: string) => {
    setIsDisconnecting(true);
    await api.setCancelFlag(false);
    addLog("info", `Disconnecting ${serial}...`);
    try {
      const result = await api.adbDisconnect(adbPath, serial);
      addLog("success", result);
      addToast("Device disconnected", "info");
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
  }, [adbPath, addLog, addToast]);

  const cancelWirelessOp = useCallback(async () => {
    await api.setCancelFlag(true);
    addLog("info", "Cancelling wireless operation...");
  }, [addLog]);

  const scan = useCallback(async () => {
    if (!adbPath || isScanning) return;
    setIsScanning(true);
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
      if (services.length === 0) {
        addLog("info", "No wireless devices discovered on the network.");
      } else {
        addLog("info", `Found ${services.length} device(s) via mDNS.`);
      }
    } catch (e) {
      addLog("warning", `mDNS scan failed: ${e}`);
      setMdnsSupported(false);
    } finally {
      setIsScanning(false);
    }
  }, [adbPath, isScanning, mdnsSupported, addLog, addToast]);

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
    discoveredDevices, isScanning, mdnsSupported, scan, selectDiscovered,
  };
}

