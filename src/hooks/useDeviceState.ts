// ─── Device State Hook ────────────────────────────────────────────────────────
import { useState, useCallback, useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
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
  const trackingActive = useRef(false);

  // ── Update devices from any source ──────────────────────────────────
  const applyDeviceUpdate = useCallback((devs: DeviceInfo[], logChange: boolean) => {
    const newSerials = devs.map((d) => d.serial).sort().join(",");
    if (newSerials === prevDeviceSerials.current && devs.length > 0) return;
    prevDeviceSerials.current = newSerials;
    setDevices(devs);
    if (devs.length > 0) {
      setSelectedDevice((prev) => {
        if (!prev || !devs.find((d) => d.serial === prev)) return devs[0].serial;
        return prev;
      });
      if (logChange) addLog("info", `Device update: ${devs.length} device(s) connected`);
    } else {
      setSelectedDevice("");
      if (logChange) addLog("info", "All devices disconnected.");
    }
  }, [addLog]);

  // ── Refresh (verbose, manual) ───────────────────────────────────────
  const refreshDevices = useCallback(async () => {
    if (!adbPath) return;
    setLoadingDevices(true);
    try {
      const devs = await api.getDevices(adbPath);
      setDevices(devs);
      prevDeviceSerials.current = devs.map((d) => d.serial).sort().join(",");
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

  // ── Silent refresh for polling fallback ─────────────────────────────
  const refreshDevicesQuiet = useCallback(async () => {
    if (!adbPath) return;
    try {
      const devs = await api.getDevices(adbPath);
      applyDeviceUpdate(devs, true);
    } catch { /* silent */ }
  }, [adbPath, applyDeviceUpdate]);

  // Sync ref
  useEffect(() => {
    prevDeviceSerials.current = devices.map((d) => d.serial).sort().join(",");
  }, [devices]);

  // ── Push-based tracking with polling fallback ───────────────────────
  useEffect(() => {
    if (adbStatus !== "found" || !adbPath) return;

    let intervalId: ReturnType<typeof setInterval> | null = null;
    let unlistenFn: (() => void) | null = null;

    const startTracking = async () => {
      // Listen for push events
      const unlisten = await listen<DeviceInfo[]>("device-list-changed", (event) => {
        applyDeviceUpdate(event.payload, true);
      });
      unlistenFn = unlisten;

      try {
        await api.startDeviceTracking(adbPath);
        trackingActive.current = true;
      } catch {
        // Tracking failed — fall back to polling
        trackingActive.current = false;
        intervalId = setInterval(refreshDevicesQuiet, 8000);
      }
    };

    startTracking();

    // Also refresh on window focus
    const onFocus = () => refreshDevicesQuiet();
    window.addEventListener("focus", onFocus);

    return () => {
      window.removeEventListener("focus", onFocus);
      if (intervalId) clearInterval(intervalId);
      if (unlistenFn) unlistenFn();
      if (trackingActive.current) {
        api.stopDeviceTracking().catch(() => {});
        trackingActive.current = false;
      }
    };
  }, [adbStatus, adbPath, refreshDevicesQuiet, applyDeviceUpdate]);

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

