// ─── Device State Hook ────────────────────────────────────────────────────────
import { useState, useCallback, useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import type { LogEntry, DeviceInfo, DeviceDetails, DetectionStatus } from "../types";
import { deduplicateDevices } from "./useWirelessAdb";
import type { DeduplicatedDevice } from "./useWirelessAdb";
import * as api from "../api";

export type InstallMode = "direct" | "verified";

export function useDeviceState(
  adbPath: string,
  adbStatus: DetectionStatus,
  addLog: (level: LogEntry["level"], message: string) => void,
) {
  const [devices, setDevices] = useState<DeduplicatedDevice[]>([]);
  const [selectedDevice, setSelectedDevice] = useState("");
  const [loadingDevices, setLoadingDevices] = useState(false);
  const [deviceExpanded, setDeviceExpanded] = useState(true);
  const [installAllDevices, setInstallAllDevices] = useState(false);
  const [installMode, setInstallMode] = useState<InstallMode>(
    () => (localStorage.getItem("installMode") as InstallMode) || "direct",
  );
  const [deviceDetails, setDeviceDetails] = useState<Record<string, DeviceDetails>>({});
  const prevDeviceFingerprint = useRef("");
  const trackingActive = useRef(false);
  const refreshInProgress = useRef(false);

  // Persist install mode preference
  useEffect(() => { localStorage.setItem("installMode", installMode); }, [installMode]);

  // ── Update devices from any source ──────────────────────────────────
  const applyDeviceUpdate = useCallback((devs: DeviceInfo[], logChange: boolean) => {
    // Include both serial AND state so offline→online transitions are detected
    const fingerprint = devs.map((d) => `${d.serial}:${d.state}`).sort().join(",");
    if (fingerprint === prevDeviceFingerprint.current && devs.length > 0) return;
    prevDeviceFingerprint.current = fingerprint;
    const deduped = deduplicateDevices(devs);
    setDevices(deduped);
    if (deduped.length > 0) {
      setSelectedDevice((prev) => {
        if (!prev || !deduped.find((d) => d.serial === prev)) return deduped[0].serial;
        return prev;
      });
      if (logChange) addLog("info", `Device update: ${deduped.length} device(s) connected`);
    } else {
      setSelectedDevice("");
      if (logChange) addLog("info", "All devices disconnected.");
    }
  }, [addLog]);

  // ── Refresh (verbose, manual) ───────────────────────────────────────
  const refreshDevices = useCallback(async () => {
    if (!adbPath || refreshInProgress.current) return;
    refreshInProgress.current = true;
    setLoadingDevices(true);
    try {
      const rawDevs = await api.getDevices(adbPath);
      const deduped = deduplicateDevices(rawDevs);
      setDevices(deduped);
      prevDeviceFingerprint.current = deduped.map((d) => `${d.serial}:${d.state}`).sort().join(",");
      if (deduped.length > 0) {
        setSelectedDevice((prev) => {
          if (!prev || !deduped.find((d) => d.serial === prev)) return deduped[0].serial;
          return prev;
        });
        addLog("info", `Found ${deduped.length} device(s)`);
      } else {
        setSelectedDevice("");
        addLog("warning", "No devices connected. Enable USB debugging on your phone and connect via USB.");
      }
    } catch (e) {
      setDevices([]);
      setSelectedDevice("");
      addLog("error", `Failed to list devices: ${e}`);
    } finally {
      refreshInProgress.current = false;
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
    } catch (e) { console.warn("Quiet device refresh failed:", e); }
  }, [adbPath, applyDeviceUpdate]);

  // Sync ref
  useEffect(() => {
    prevDeviceFingerprint.current = devices.map((d) => `${d.serial}:${d.state}`).sort().join(",");
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
      } catch (e) {
        console.warn("Device tracking failed, falling back to polling:", e);
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
        api.stopDeviceTracking().catch((e) => console.warn("stopDeviceTracking cleanup failed:", e));
        trackingActive.current = false;
      }
    };
  }, [adbStatus, adbPath, refreshDevicesQuiet, applyDeviceUpdate]);

  // ── Auto-collapse when device is connected ─────────────────────────
  useEffect(() => {
    if (selectedDevice && devices.length > 0) setDeviceExpanded(false);
    else setDeviceExpanded(true);
  }, [selectedDevice, devices.length]);

  // ── Fetch device details (Android version, API level, storage) ────
  const fetchDeviceDetails = useCallback(async (serial: string) => {
    if (!adbPath || deviceDetails[serial]) return;
    try {
      const details = await api.getDeviceDetails(adbPath, serial);
      setDeviceDetails((prev) => ({ ...prev, [serial]: details }));
    } catch (e) { console.warn(`Failed to get details for ${serial}:`, e); }
  }, [adbPath, deviceDetails]);

  // Auto-fetch details for selected device and all online devices
  useEffect(() => {
    if (!adbPath) return;
    const online = devices.filter((d) => d.state === "device");
    for (const d of online) {
      if (!deviceDetails[d.serial]) {
        fetchDeviceDetails(d.serial);
      }
    }
  }, [devices, adbPath, deviceDetails, fetchDeviceDetails]);

  return {
    devices, selectedDevice, setSelectedDevice,
    loadingDevices, refreshDevices, refreshDevicesQuiet,
    deviceExpanded, setDeviceExpanded,
    installAllDevices, setInstallAllDevices,
    installMode, setInstallMode,
    deviceDetails,
  };
}

