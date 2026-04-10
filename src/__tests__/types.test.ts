// ─── Tests for src/types.ts ───────────────────────────────────────────────────
// These tests verify that type interfaces work correctly at runtime by
// ensuring objects conforming to the interfaces have the right shape.
import { describe, it, expect } from "vitest";
import type {
  DeviceInfo, LogEntry, ToolsStatus, DownloadProgress,
  StaleTool, DetectionStatus, RecentFile, RecentFilesConfig,
} from "../types";

describe("Types", () => {
  describe("DeviceInfo", () => {
    it("accepts a valid device object", () => {
      const device: DeviceInfo = {
        serial: "ABC123",
        state: "device",
        model: "Pixel 7",
        product: "panther",
        transport_id: "1",
      };
      expect(device.serial).toBe("ABC123");
      expect(device.state).toBe("device");
      expect(device.model).toBe("Pixel 7");
      expect(device.product).toBe("panther");
      expect(device.transport_id).toBe("1");
    });
  });

  describe("LogEntry", () => {
    it("accepts all valid log levels", () => {
      const levels: LogEntry["level"][] = ["info", "success", "error", "warning"];
      levels.forEach((level) => {
        const entry: LogEntry = { id: 1, time: "12:00:00", level, message: "test" };
        expect(entry.level).toBe(level);
      });
    });
  });

  describe("ToolsStatus", () => {
    it("has all required fields", () => {
      const status: ToolsStatus = {
        adb_installed: true,
        adb_path: "/path/adb",
        bundletool_installed: false,
        bundletool_path: "",
        java_installed: true,
        java_path: "/path/java",
        data_dir: "/data",
      };
      expect(status.adb_installed).toBe(true);
      expect(status.bundletool_installed).toBe(false);
      expect(status.java_installed).toBe(true);
    });
  });

  describe("DownloadProgress", () => {
    it("has all required fields", () => {
      const progress: DownloadProgress = {
        tool: "platform-tools",
        downloaded: 5000,
        total: 10000,
        percent: 50,
        status: "downloading",
      };
      expect(progress.percent).toBe(50);
      expect(progress.status).toBe("downloading");
    });
  });

  describe("StaleTool", () => {
    it("has all required fields", () => {
      const stale: StaleTool = {
        tool: "bundletool",
        label: "bundletool",
        last_updated_secs: 1000,
        age_days: 45,
      };
      expect(stale.age_days).toBe(45);
    });
  });

  describe("DetectionStatus", () => {
    it("accepts all valid values", () => {
      const statuses: DetectionStatus[] = ["unknown", "found", "not-found"];
      expect(statuses).toEqual(["unknown", "found", "not-found"]);
    });
  });

  describe("RecentFile", () => {
    it("has all required fields", () => {
      const file: RecentFile = {
        path: "/path/to/file.apk",
        name: "file.apk",
        last_used: Date.now(),
      };
      expect(file.name).toBe("file.apk");
    });
  });

  describe("RecentFilesConfig", () => {
    it("has packages and keystores arrays", () => {
      const config: RecentFilesConfig = {
        packages: [],
        keystores: [],
      };
      expect(config.packages).toEqual([]);
      expect(config.keystores).toEqual([]);
    });

    it("can hold multiple recent files", () => {
      const config: RecentFilesConfig = {
        packages: [
          { path: "/a.apk", name: "a.apk", last_used: 1 },
          { path: "/b.aab", name: "b.aab", last_used: 2 },
        ],
        keystores: [
          { path: "/k.jks", name: "k.jks", last_used: 3 },
        ],
      };
      expect(config.packages.length).toBe(2);
      expect(config.keystores.length).toBe(1);
    });
  });
});

