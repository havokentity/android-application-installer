// ─── Tests for src/types.ts ───────────────────────────────────────────────────
// These tests verify that type interfaces work correctly at runtime by
// ensuring objects conforming to the interfaces have the right shape.
import { describe, it, expect } from "vitest";
import type {
  DeviceInfo, LogEntry, ToolsStatus, DownloadProgress,
  StaleTool, DetectionStatus, RecentFile, RecentFilesConfig,
  SigningProfile, PackageMetadata, OperationState, DeviceDetails,
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

  describe("SigningProfile", () => {
    it("has all required fields", () => {
      const profile: SigningProfile = {
        name: "Release",
        keystorePath: "/path/to/release.jks",
        keystorePass: "storepass",
        keyAlias: "releaseKey",
        keyPass: "keypass",
      };
      expect(profile.name).toBe("Release");
      expect(profile.keystorePath).toBe("/path/to/release.jks");
      expect(profile.keystorePass).toBe("storepass");
      expect(profile.keyAlias).toBe("releaseKey");
      expect(profile.keyPass).toBe("keypass");
    });
  });

  describe("PackageMetadata", () => {
    it("has all required fields with values", () => {
      const meta: PackageMetadata = {
        packageName: "com.example.app",
        versionName: "2.1.0",
        versionCode: "42",
        minSdk: "21",
        targetSdk: "34",
        permissions: ["android.permission.INTERNET", "android.permission.CAMERA"],
        fileSize: 44347801,
      };
      expect(meta.packageName).toBe("com.example.app");
      expect(meta.versionName).toBe("2.1.0");
      expect(meta.versionCode).toBe("42");
      expect(meta.minSdk).toBe("21");
      expect(meta.targetSdk).toBe("34");
      expect(meta.permissions).toHaveLength(2);
      expect(meta.fileSize).toBe(44347801);
    });

    it("accepts null values for optional fields", () => {
      const meta: PackageMetadata = {
        packageName: null,
        versionName: null,
        versionCode: null,
        minSdk: null,
        targetSdk: null,
        permissions: [],
        fileSize: 0,
      };
      expect(meta.packageName).toBeNull();
      expect(meta.permissions).toEqual([]);
    });
  });

  describe("OperationState", () => {
    it("represents idle state", () => {
      const state: OperationState = { type: "idle" };
      expect(state.type).toBe("idle");
    });

    it("represents installing state with cancel token", () => {
      const state: OperationState = { type: "installing", progress: null, cancelToken: "op-1" };
      expect(state.type).toBe("installing");
      expect(state.cancelToken).toBe("op-1");
      expect(state.progress).toBeNull();
    });

    it("represents extracting state with progress", () => {
      const progress = { operation: "extract_apk", device: "", status: "running", message: "Extracting...", step: 1, total_steps: 2, cancellable: true };
      const state: OperationState = { type: "extracting", progress, cancelToken: "op-2" };
      expect(state.type).toBe("extracting");
      expect(state.progress?.operation).toBe("extract_apk");
    });

    it("derives isInstalling correctly", () => {
      const idle: OperationState = { type: "idle" };
      const installing: OperationState = { type: "installing", progress: null, cancelToken: null };
      expect(idle.type === "installing").toBe(false);
      expect(installing.type === "installing").toBe(true);
    });
  });

  describe("DeviceDetails", () => {
    it("has all required fields", () => {
      const details: DeviceDetails = {
        android_version: "14",
        api_level: "34",
        free_storage: "25.3 GB",
      };
      expect(details.android_version).toBe("14");
      expect(details.api_level).toBe("34");
      expect(details.free_storage).toBe("25.3 GB");
    });
  });
});

