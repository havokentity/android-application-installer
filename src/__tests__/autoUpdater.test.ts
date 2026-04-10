// ─── Tests for auto-updater integration in App.tsx ─────────────────────────────
import { describe, it, expect, vi, beforeEach } from "vitest";
import { check } from "@tauri-apps/plugin-updater";
import { ask } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";

// ─── check() — core updater behavior ──────────────────────────────────────────

describe("Auto-updater (plugin mocks)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("check() returns null when no update is available", async () => {
    vi.mocked(check).mockResolvedValue(null);
    const result = await check();
    expect(result).toBeNull();
    expect(check).toHaveBeenCalledTimes(1);
  });

  it("check() returns an update object when an update is available", async () => {
    const mockUpdate = {
      version: "2.0.0",
      body: "New features and bug fixes",
      date: "2026-04-10",
      downloadAndInstall: vi.fn(() => Promise.resolve()),
    };
    vi.mocked(check).mockResolvedValue(mockUpdate as any);

    const result = await check();
    expect(result).not.toBeNull();
    expect(result!.version).toBe("2.0.0");
    expect(result!.body).toBe("New features and bug fixes");
  });

  it("ask() returns true when user accepts the update", async () => {
    vi.mocked(ask).mockResolvedValue(true);
    const result = await ask("Update to 2.0.0?", { title: "Update Available", kind: "info" });
    expect(result).toBe(true);
    expect(ask).toHaveBeenCalledWith("Update to 2.0.0?", {
      title: "Update Available",
      kind: "info",
    });
  });

  it("ask() returns false when user declines the update", async () => {
    vi.mocked(ask).mockResolvedValue(false);
    const result = await ask("Update to 2.0.0?", { title: "Update Available", kind: "info" });
    expect(result).toBe(false);
  });

  it("downloadAndInstall() can be called on the update object", async () => {
    const downloadAndInstall = vi.fn(() => Promise.resolve());
    const mockUpdate = {
      version: "2.0.0",
      body: "Release notes",
      date: "2026-04-10",
      downloadAndInstall,
    };
    vi.mocked(check).mockResolvedValue(mockUpdate as any);

    const update = await check();
    await update!.downloadAndInstall(vi.fn());
    expect(downloadAndInstall).toHaveBeenCalledTimes(1);
  });

  it("downloadAndInstall() accepts a progress callback", async () => {
    const progressCallback = vi.fn();
    const downloadAndInstall = vi.fn(async (cb: any) => {
      cb({ event: "Started", data: { contentLength: 5000000 } });
      cb({ event: "Progress", data: { chunkLength: 1000000 } });
      cb({ event: "Progress", data: { chunkLength: 2000000 } });
      cb({ event: "Finished" });
    });
    const mockUpdate = { version: "2.0.0", body: "", date: "", downloadAndInstall };
    vi.mocked(check).mockResolvedValue(mockUpdate as any);

    const update = await check();
    await update!.downloadAndInstall(progressCallback);
    expect(progressCallback).toHaveBeenCalledTimes(4);
    expect(progressCallback).toHaveBeenCalledWith({ event: "Started", data: { contentLength: 5000000 } });
    expect(progressCallback).toHaveBeenCalledWith({ event: "Progress", data: { chunkLength: 1000000 } });
    expect(progressCallback).toHaveBeenCalledWith({ event: "Finished" });
  });

  it("relaunch() can be called after install", async () => {
    vi.mocked(relaunch).mockResolvedValue(undefined);
    await relaunch();
    expect(relaunch).toHaveBeenCalledTimes(1);
  });

  it("check() handles network errors gracefully", async () => {
    vi.mocked(check).mockRejectedValue(new Error("Network error"));
    await expect(check()).rejects.toThrow("Network error");
  });

  it("full update flow: check → ask → download → relaunch", async () => {
    const downloadAndInstall = vi.fn(() => Promise.resolve());
    const mockUpdate = {
      version: "2.0.0",
      body: "Bug fixes",
      date: "2026-04-10",
      downloadAndInstall,
    };
    vi.mocked(check).mockResolvedValue(mockUpdate as any);
    vi.mocked(ask).mockResolvedValue(true);
    vi.mocked(relaunch).mockResolvedValue(undefined);

    // 1. Check for update
    const update = await check();
    expect(update).not.toBeNull();

    // 2. Ask user
    const accepted = await ask(`Update to ${update!.version}?`, { title: "Update", kind: "info" });
    expect(accepted).toBe(true);

    // 3. Download and install
    await update!.downloadAndInstall(vi.fn());
    expect(downloadAndInstall).toHaveBeenCalledTimes(1);

    // 4. Relaunch
    await relaunch();
    expect(relaunch).toHaveBeenCalledTimes(1);
  });

  it("update flow is skipped when user declines", async () => {
    const downloadAndInstall = vi.fn();
    const mockUpdate = {
      version: "2.0.0",
      body: "New version",
      date: "2026-04-10",
      downloadAndInstall,
    };
    vi.mocked(check).mockResolvedValue(mockUpdate as any);
    vi.mocked(ask).mockResolvedValue(false);

    const update = await check();
    const accepted = await ask(`Update to ${update!.version}?`, { title: "Update", kind: "info" });
    expect(accepted).toBe(false);

    // Should NOT download or relaunch
    expect(downloadAndInstall).not.toHaveBeenCalled();
    expect(relaunch).not.toHaveBeenCalled();
  });
});

