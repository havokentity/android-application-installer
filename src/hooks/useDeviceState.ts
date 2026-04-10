// ─── Device State Hook ────────────────────────────────────────────────────────
import { useState, useCallback, useEffect, useRef } from "react";
import type { LogEntry, DeviceInfo, DetectionStatus } from "../types";
import * as api from "../api";

export function useDeviceState(
  adbPath: string,
  adbStatus: DetectionStatus,
  addLog: (level: LogEntry["level"], message: string) => void,
) {
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [selectedDevice, setSelectedDevice] = useState("");
  const [loadingDevices, setLoadingDevices] = useState(false);
  const [deviceExpanded, setDeviceExpanded] = useState(true);
  const [installAllDevices, setInstallAllDevices] = useState(false);
  const prevDeviceSerials = useRef("");

  // ── Refresh (verbose) ──────────────────────────────────────────────
  const refreshDevices = useCallback(async () => {
    if (!adbPath) return;
    setLoadingDevices(true);
    try {
      const devs = await api.getDevices(adbPath);
      setDevices(devs);
      if (devs.length > 0) {
        setSelectedDevice((prev) => {
          if (!prev || !devs.find((d) => d.serial === prev)) return devs[0].serial;
          return prev;
        });
        addLog("info", `Found ${devs.length} device(s)`);
      } else {
        setSelectedDevice("");
        addLog("warning", "No devices connected. Enable USB debugging on your phone and connect via USB.");
      }
    } catch (e) {
      setDevices([]);
      setSelectedDevice("");
      addLog("error", `Failed to list devices: ${e}`);
    } finally {
      setLoadingDevices(false);
    }
  }, [adbPath, addLog]);

  // ── Initial refresh when ADB is found ──────────────────────────────
  useEffect(() => {
    if (adbStatus === "found") refreshDevices();
  }, [adbStatus]); // eslint-disable-line react-hooks/exhaustive-deps

  // ── Silent refresh (only logs on change) ────────────────────────────
  const refreshDevicesQuiet = useCallback(async () => {
    if (!adbPath) return;
    try {
      const devs = await api.getDevices(adbPath);
      const newSerials = devs.map((d) => d.serial).sort().join(",");
      if (newSerials === prevDeviceSerials.current) return;
      prevDeviceSerials.current = newSerials;
      setDevices(devs);
      if (devs.length > 0) {
        setSelectedDevice((prev) => {
          if (!prev || !devs.find((d) => d.serial === prev)) return devs[0].serial;
          return prev;
        });
        addLog("info", `Device update: ${devs.length} device(s) connected`);
      } else {
        setSelectedDevice("");
        addLog("info", "All devices disconnected.");
      }
    } catch { /* silent */ }
  }, [adbPath, addLog]);

  // Sync ref
  useEffect(() => {
    prevDeviceSerials.current = devices.map((d) => d.serial).sort().join(",");
  }, [devices]);

  // ── Auto-refresh interval ──────────────────────────────────────────
  useEffect(() => {
    if (adbStatus !== "found" || !adbPath) return;
    const interval = setInterval(refreshDevicesQuiet, 8000);
    const onFocus = () => refreshDevicesQuiet();
    window.addEventListener("focus", onFocus);
    return () => {
      clearInterval(interval);
      window.removeEventListener("focus", onFocus);
    };
  }, [adbStatus, adbPath, refreshDevicesQuiet]);

  // ── Auto-collapse when device is connected ─────────────────────────
  useEffect(() => {
    if (selectedDevice && devices.length > 0) setDeviceExpanded(false);
    else setDeviceExpanded(true);
  }, [selectedDevice, devices.length]);

  return {
    devices, selectedDevice, setSelectedDevice,
    loadingDevices, refreshDevices,
    deviceExpanded, setDeviceExpanded,
    installAllDevices, setInstallAllDevices,
  };
}

