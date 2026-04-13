// ─── Wireless ADB Hook ───────────────────────────────────────────────────────
import { useState, useCallback, useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import type { DeviceInfo, LogEntry, MdnsService } from "../types";
import type { ToastLevel } from "../components/Toast";
import * as api from "../api";
import type { QrPairingInfo, QrPairingResult } from "../api";

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
export interface DeduplicatedDevice extends DeviceInfo {
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
export function deduplicateDevices(devices: DeviceInfo[]): DeduplicatedDevice[] {
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
        let preferred: DeviceInfo;
        let discarded: DeviceInfo;

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

/**
 * Enrich deduplicated devices with `alternateSerial` from mDNS discovery data.
 *
 * When a device only shows ONE transport in `adb devices` (e.g. only IP:port,
 * or only mDNS), this function tries to fill in the missing alternate serial
 * by cross-referencing with the mDNS services list from a scan.
 *
 * - IP:port device → finds matching mDNS service by IP → constructs mDNS serial
 * - mDNS device → finds matching service by name → fills IP:port from service
 */
export function enrichWithDiscoveredServices(
  devices: DeduplicatedDevice[],
  discoveredServices: MdnsService[],
): DeduplicatedDevice[] {
  if (discoveredServices.length === 0) return devices;

  return devices.map((d) => {
    // Already has an alternate or isn't wireless — nothing to do
    if (d.alternateSerial || !isWirelessDevice(d.serial)) return d;

    if (isIpPortDevice(d.serial)) {
      // IP:port device → find mDNS service with matching IP address
      const deviceIp = d.serial.split(":")[0];
      const svc = discoveredServices.find(
        (s) => s.service_type.includes("connect") && s.ip_port.split(":")[0] === deviceIp,
      );
      if (svc) {
        // Construct the mDNS serial from the service name
        return { ...d, alternateSerial: `${svc.name}._adb-tls-connect._tcp` };
      }
    } else if (isMdnsDevice(d.serial)) {
      // mDNS device → find matching service by name to get IP:port
      const namePart = d.serial.split("._adb-tls")[0];
      const svc = discoveredServices.find(
        (s) => s.name === namePart && s.service_type.includes("connect"),
      );
      if (svc) {
        return { ...d, alternateSerial: svc.ip_port };
      }
    }

    return d;
  });
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
  disconnect: (serial: string, alsoDisconnect?: string) => Promise<void>;
  /** Silently connect to an IP:port target without touching form state. */
  connectDirect: (target: string) => Promise<boolean>;
  cancelWirelessOp: () => Promise<void>;
  // pairing prompt after connect failure
  needsPairing: boolean;
  promptPairing: () => Promise<void>;
  // mDNS discovery
  discoveredDevices: MdnsService[];
  isScanning: boolean;
  mdnsSupported: boolean | null;
  scan: () => Promise<void>;
  selectDiscovered: (svc: MdnsService) => void;
  // QR code pairing
  qrPairingInfo: QrPairingInfo | null;
  isQrPairing: boolean;
  startQrPairing: () => Promise<void>;
  cancelQrPairing: () => Promise<void>;
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

  // QR code pairing state
  const [qrPairingInfo, setQrPairingInfo] = useState<QrPairingInfo | null>(null);
  const [isQrPairing, setIsQrPairing] = useState(false);

  // Store callbacks in refs so the event-listener effect never re-registers.
  // This avoids a race where the async unlisten overlaps with a new listener,
  // causing duplicate event handling (double toasts, double log entries).
  const addLogRef = useRef(addLog);
  const addToastRef = useRef(addToast);
  const onDeviceChangeRef = useRef(onDeviceChange);
  addLogRef.current = addLog;
  addToastRef.current = addToast;
  onDeviceChangeRef.current = onDeviceChange;

  // Listen for QR pairing result events from the backend
  useEffect(() => {
    const unlistenResult = listen<QrPairingResult>("qr-pairing-result", (event) => {
      const result = event.payload;
      setIsQrPairing(false);
      setQrPairingInfo(null);
      if (result.success) {
        addLogRef.current("success", `QR pairing successful${result.device_ip ? ` with ${result.device_ip}` : ""}!`);
        addToastRef.current("Device paired via QR code", "success");
        setTimeout(() => onDeviceChangeRef.current?.(), 3000);
      } else {
        const err = result.error || "Unknown error";
        if (err.includes("cancelled")) {
          addLogRef.current("info", "QR pairing cancelled.");
        } else {
          addLogRef.current("error", `QR pairing failed: ${err}`);
          addToastRef.current(`QR pairing failed: ${err}`, "error");
        }
      }
    });
    // Forward backend pairing progress to the log panel
    const unlistenLog = listen<string>("qr-pairing-log", (event) => {
      addLogRef.current("info", `[QR] ${event.payload}`);
    });
    return () => {
      unlistenResult.then((fn) => fn());
      unlistenLog.then((fn) => fn());
    };
  }, []);

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
          setWifiExpanded(true);
        }
      }
    } finally {
      setIsConnecting(false);
    }
  }, [canConnect, connectIp, connectPort, adbPath, addLog, addToast, onDeviceChange]);

  const disconnect = useCallback(async (serial: string, alsoDisconnect?: string) => {
    setIsDisconnecting(true);
    await api.setCancelFlag(false);
    addLog("info", `Disconnecting ${serial}...`);
    try {
      const result = await api.adbDisconnect(adbPath, serial);
      addLog("success", result);
      // Also disconnect the alternate transport (twin) if provided
      if (alsoDisconnect) {
        try {
          await api.adbDisconnect(adbPath, alsoDisconnect);
          addLog("info", `Also disconnected alternate transport.`);
        } catch (e) { console.warn("Alternate transport disconnect failed (non-critical):", e); }
      }
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

  /** Silently connect to an IP:port target without touching UI form state.
   *  Used for auto-connecting alternate transports when switching install modes. */
  const connectDirect = useCallback(async (target: string): Promise<boolean> => {
    if (!adbPath) return false;
    try {
      await api.adbConnect(adbPath, target);
      onDeviceChange?.();
      return true;
    } catch (e) {
      console.warn("Direct connect failed:", e);
      return false;
    }
  }, [adbPath, onDeviceChange]);

  const scan = useCallback(async () => {
    if (!adbPath || isScanning) return;
    setIsScanning(true);
    setDiscoveredDevices([]); // clear stale results before scanning
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
      addLog("info", "Scanning for wireless devices...");
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

  /** Pre-fill pairing fields from the connect IP, looking up the correct
   *  pairing port from mDNS discovery (pairing port ≠ connect port).
   *  If no pairing service is cached, triggers a quick scan first. */
  const promptPairing = useCallback(async () => {
    setPairIp(connectIp);
    setPairingCode("");
    setNeedsPairing(false);

    // Look up the pairing service for this IP from already-discovered services
    let pairingSvc = discoveredDevices.find(
      (s) => s.service_type.includes("pairing") && s.ip_port.split(":")[0] === connectIp,
    );

    // If not found, do a quick mDNS scan to discover it
    if (!pairingSvc && adbPath) {
      try {
        addLog("info", "Scanning for pairing service...");
        const services = await api.adbMdnsServices(adbPath);
        setDiscoveredDevices(services);
        pairingSvc = services.find(
          (s) => s.service_type.includes("pairing") && s.ip_port.split(":")[0] === connectIp,
        );
      } catch (e) {
        console.warn("Auto-scan for pairing service failed:", e);
      }
    }

    if (pairingSvc) {
      const port = pairingSvc.ip_port.split(":")[1] || "";
      setPairPort(port);
      addLog("info", `Pairing fields pre-filled with ${connectIp}:${port} from network discovery. Enter the pairing code from your device.`);
    } else {
      setPairPort("");
      addLog("info", `Pairing IP pre-filled with ${connectIp}. Enter the pairing port and code from your device (or scan for devices to auto-detect the port).`);
    }
  }, [connectIp, discoveredDevices, adbPath, addLog]);

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

  /** Start QR code pairing — shows a QR code for the phone to scan. */
  const startQrPairing = useCallback(async () => {
    if (!adbPath || isQrPairing) return;
    setIsQrPairing(true);
    setQrPairingInfo(null);
    addLog("info", "Starting QR code pairing...");
    try {
      const info = await api.startQrPairing(adbPath);
      setQrPairingInfo(info);
      addLog("info", `QR code ready — scan with your Android phone (Settings → Developer Options → Wireless debugging → Pair device with QR code).`);
    } catch (e) {
      setIsQrPairing(false);
      addLog("error", `QR pairing failed to start: ${e}`);
      addToast(`QR pairing failed: ${e}`, "error");
    }
  }, [adbPath, isQrPairing, addLog, addToast]);

  /** Cancel the current QR pairing session. */
  const cancelQrPairing = useCallback(async () => {
    try {
      await api.cancelQrPairing();
    } catch (e) {
      console.warn("cancelQrPairing failed:", e);
    }
    setIsQrPairing(false);
    setQrPairingInfo(null);
    addLog("info", "QR pairing cancelled.");
  }, [addLog]);

  return {
    wifiExpanded, setWifiExpanded,
    pairIp, setPairIp, pairPort, setPairPort, pairingCode, setPairingCode,
    connectIp, setConnectIp, connectPort, setConnectPort,
    isPairing, isConnecting, isDisconnecting,
    canPair, canConnect,
    pair, connect, disconnect, connectDirect, cancelWirelessOp,
    needsPairing, promptPairing,
    discoveredDevices, isScanning, mdnsSupported, scan, selectDiscovered,
    qrPairingInfo, isQrPairing, startQrPairing, cancelQrPairing,
  };
}

