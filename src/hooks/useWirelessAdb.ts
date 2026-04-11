// ─── Wireless ADB Hook ───────────────────────────────────────────────────────
import { useState, useCallback } from "react";
import type { LogEntry } from "../types";
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
  canPair: boolean;
  canConnect: boolean;
  pair: () => Promise<void>;
  connect: () => Promise<void>;
  disconnect: (serial: string) => Promise<void>;
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

  const canPair = !!(adbPath && isValidIp(pairIp) && isValidPort(pairPort) && isValidPairingCode(pairingCode) && !isPairing);
  const canConnect = !!(adbPath && isValidIp(connectIp) && isValidPort(connectPort) && !isConnecting);

  const pair = useCallback(async () => {
    if (!canPair) return;
    const ipPort = `${pairIp}:${pairPort}`;
    setIsPairing(true);
    addLog("info", `Pairing with ${ipPort}...`);
    try {
      const result = await api.adbPair(adbPath, ipPort, pairingCode);
      addLog("success", result);
      addToast("Device paired successfully", "success");
      // Auto-fill connect IP from pair IP
      setConnectIp(pairIp);
      // Clear pairing code after success
      setPairingCode("");
    } catch (e) {
      addLog("error", `Pairing failed: ${e}`);
      addToast(`Pairing failed: ${e}`, "error");
    } finally {
      setIsPairing(false);
    }
  }, [canPair, pairIp, pairPort, pairingCode, adbPath, addLog, addToast]);

  const connect = useCallback(async () => {
    if (!canConnect) return;
    const ipPort = `${connectIp}:${connectPort}`;
    setIsConnecting(true);
    addLog("info", `Connecting to ${ipPort}...`);
    try {
      const result = await api.adbConnect(adbPath, ipPort);
      addLog("success", result);
      addToast("Connected wirelessly", "success");
    } catch (e) {
      addLog("error", `Connection failed: ${e}`);
      addToast(`Connection failed: ${e}`, "error");
    } finally {
      setIsConnecting(false);
    }
  }, [canConnect, connectIp, connectPort, adbPath, addLog, addToast]);

  const disconnect = useCallback(async (serial: string) => {
    addLog("info", `Disconnecting ${serial}...`);
    try {
      const result = await api.adbDisconnect(adbPath, serial);
      addLog("success", result);
      addToast("Device disconnected", "info");
    } catch (e) {
      addLog("error", `Disconnect failed: ${e}`);
      addToast(`Disconnect failed: ${e}`, "error");
    }
  }, [adbPath, addLog, addToast]);

  return {
    wifiExpanded, setWifiExpanded,
    pairIp, setPairIp, pairPort, setPairPort, pairingCode, setPairingCode,
    connectIp, setConnectIp, connectPort, setConnectPort,
    isPairing, isConnecting,
    canPair, canConnect,
    pair, connect, disconnect,
  };
}

