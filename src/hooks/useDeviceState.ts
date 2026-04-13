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
  const lastLoggedDedupFp = useRef("");
  const trackingActive = useRef(false);
  const refreshInProgress = useRef(false);
  const fetchingDetailsFor = useRef<Set<string>>(new Set());

  // Persist install mode preference
  useEffect(() => { localStorage.setItem("installMode", installMode); }, [installMode]);

  // ── Update devices from any source ──────────────────────────────────
  const applyDeviceUpdate = useCallback((devs: DeviceInfo[], logChange: boolean) => {
    // Include both serial AND state so offline→online transitions are detected
    const fingerprint = devs.map((d) => `${d.serial}:${d.state}`).sort().join(",");
    if (fingerprint === prevDeviceFingerprint.current) return;
    prevDeviceFingerprint.current = fingerprint;
    const deduped = deduplicateDevices(devs);
    setDevices(deduped);

    // Only log when the DEDUPLICATED result actually changed from
    // what we last told the user.  We compare device-count + states
    // (not exact serials) because deduplication can flip the chosen serial
    // when a twin transport appears (mDNS → IP:port) — same device, different serial.
    // This ref is ONLY updated when we log, so intermediate state syncs can't reset it.
    const dedupedFp = `${deduped.length}:${deduped.map((d) => d.state).sort().join(",")}`;
    const dedupedChanged = dedupedFp !== lastLoggedDedupFp.current;

    if (deduped.length > 0) {
      setSelectedDevice((prev) => {
        if (!prev || !deduped.find((d) => d.serial === prev)) return deduped[0].serial;
        return prev;
      });
      if (logChange && dedupedChanged) {
        lastLoggedDedupFp.current = dedupedFp;
        addLog("info", `Device update: ${deduped.length} device(s) connected`);
      }
    } else {
      setSelectedDevice("");
      if (logChange && dedupedChanged) {
        lastLoggedDedupFp.current = dedupedFp;
        addLog("info", "All devices disconnected.");
      }
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
  // (handled by the tracking effect below — tracker push provides initial list)

  // ── Silent refresh for polling fallback ─────────────────────────────
  const refreshDevicesQuiet = useCallback(async () => {
    if (!adbPath) return;
    try {
      const devs = await api.getDevices(adbPath);
      applyDeviceUpdate(devs, true);
    } catch (e) { console.warn("Quiet device refresh failed:", e); }
  }, [adbPath, applyDeviceUpdate]);

  // Sync raw fingerprint ref when devices state changes externally
  useEffect(() => {
    prevDeviceFingerprint.current = devices.map((d) => `${d.serial}:${d.state}`).sort().join(",");
  }, [devices]);

  // ── Push-based tracking with one-shot fallback ─────────────────────
  useEffect(() => {
    if (adbStatus !== "found" || !adbPath) return;

    let intervalId: ReturnType<typeof setInterval> | null = null;
    let unlistenFn: (() => void) | null = null;
    let cancelled = false;

    setLoadingDevices(true);

    const startTracking = async () => {
      // Listen for push events — this will provide the initial device list
      const unlisten = await listen<DeviceInfo[]>("device-list-changed", (event) => {
        if (cancelled) return;
        setLoadingDevices(false);
        applyDeviceUpdate(event.payload, true);
      });
      unlistenFn = unlisten;

      try {
        await api.startDeviceTracking(adbPath);
        trackingActive.current = true;

        // The tracker emits the current device list immediately.
        // Give it a moment, then clear loading if no event arrived
        // (happens when zero devices are connected).
        setTimeout(() => {
          if (!cancelled) setLoadingDevices(false);
        }, 3000);
      } catch (e) {
        console.warn("Device tracking failed, falling back to polling:", e);
        trackingActive.current = false;

        // Tracking failed — do a one-shot device refresh + start polling
        try {
          const rawDevs = await api.getDevices(adbPath);
          if (!cancelled) {
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
          }
        } catch (e2) {
          if (!cancelled) {
            setDevices([]);
            setSelectedDevice("");
            addLog("error", `Failed to list devices: ${e2}`);
          }
        }

        if (!cancelled) {
          setLoadingDevices(false);
          intervalId = setInterval(refreshDevicesQuiet, 8000);
        }
      }
    };

    startTracking();

    // Also refresh on window focus
    const onFocus = () => refreshDevicesQuiet();
    window.addEventListener("focus", onFocus);

    return () => {
      cancelled = true;
      window.removeEventListener("focus", onFocus);
      if (intervalId) clearInterval(intervalId);
      if (unlistenFn) unlistenFn();
      if (trackingActive.current) {
        api.stopDeviceTracking().catch((e) => console.warn("stopDeviceTracking cleanup failed:", e));
        trackingActive.current = false;
      }
    };
  }, [adbStatus, adbPath, refreshDevicesQuiet, applyDeviceUpdate, addLog]);

  // ── Auto-collapse when device is connected ─────────────────────────
  useEffect(() => {
    if (selectedDevice && devices.length > 0) setDeviceExpanded(false);
    else setDeviceExpanded(true);
  }, [selectedDevice, devices.length]);

  // ── Fetch device details (Android version, API level, storage) ────
  const fetchDeviceDetails = useCallback(async (serial: string) => {
    if (!adbPath) return;
    // Skip if already fetched or currently in-flight
    if (fetchingDetailsFor.current.has(serial)) return;
    fetchingDetailsFor.current.add(serial);
    try {
      const details = await api.getDeviceDetails(adbPath, serial);
      setDeviceDetails((prev) => ({ ...prev, [serial]: details }));
    } catch (e) { console.warn(`Failed to get details for ${serial}:`, e); }
    finally { fetchingDetailsFor.current.delete(serial); }
  }, [adbPath]);

  // Auto-fetch details for selected device and all online devices
  useEffect(() => {
    if (!adbPath) return;
    const online = devices.filter((d) => d.state === "device");
    for (const d of online) {
      fetchDeviceDetails(d.serial);
    }
  }, [devices, adbPath, fetchDeviceDetails]);

  return {
    devices, selectedDevice, setSelectedDevice,
    loadingDevices, refreshDevices, refreshDevicesQuiet,
    deviceExpanded, setDeviceExpanded,
    installAllDevices, setInstallAllDevices,
    installMode, setInstallMode,
    deviceDetails,
  };
}

