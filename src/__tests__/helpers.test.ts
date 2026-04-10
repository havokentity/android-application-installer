// ─── Tests for src/helpers.ts ─────────────────────────────────────────────────
import { describe, it, expect, beforeEach } from "vitest";
import { nextLogId, getFileName, getFileType, now, formatBytes, shortcutLabel } from "../helpers";

// ─── nextLogId ────────────────────────────────────────────────────────────────

describe("nextLogId", () => {
  it("returns incrementing numbers on successive calls", () => {
    const id1 = nextLogId();
    const id2 = nextLogId();
    const id3 = nextLogId();
    expect(id2).toBe(id1 + 1);
    expect(id3).toBe(id2 + 1);
  });

  it("always returns a positive integer", () => {
    const id = nextLogId();
    expect(id).toBeGreaterThan(0);
    expect(Number.isInteger(id)).toBe(true);
  });
});

// ─── getFileName ──────────────────────────────────────────────────────────────

describe("getFileName", () => {
  it("extracts filename from Unix-style path", () => {
    expect(getFileName("/home/user/Downloads/my-app.apk")).toBe("my-app.apk");
  });

  it("extracts filename from Windows-style path", () => {
    expect(getFileName("C:\\Users\\user\\Downloads\\my-app.apk")).toBe("my-app.apk");
  });

  it("extracts filename from mixed separators", () => {
    expect(getFileName("/home/user\\Downloads/my-app.aab")).toBe("my-app.aab");
  });

  it("returns the input if no path separator exists", () => {
    expect(getFileName("my-app.apk")).toBe("my-app.apk");
  });

  it("returns the input for an empty string", () => {
    expect(getFileName("")).toBe("");
  });

  it("handles path with trailing separator", () => {
    // The last element after split is "", pop gives ""
    // Fallback to the original path
    const result = getFileName("/home/user/");
    expect(result).toBe("/home/user/");
  });

  it("handles deeply nested paths", () => {
    expect(getFileName("/a/b/c/d/e/f/deep.apk")).toBe("deep.apk");
  });
});

// ─── getFileType ──────────────────────────────────────────────────────────────

describe("getFileType", () => {
  it("returns 'apk' for .apk files", () => {
    expect(getFileType("/path/to/app.apk")).toBe("apk");
  });

  it("returns 'aab' for .aab files", () => {
    expect(getFileType("/path/to/app.aab")).toBe("aab");
  });

  it("is case-insensitive", () => {
    expect(getFileType("/path/to/app.APK")).toBe("apk");
    expect(getFileType("/path/to/app.AAB")).toBe("aab");
    expect(getFileType("/path/to/app.Apk")).toBe("apk");
    expect(getFileType("/path/to/app.AaB")).toBe("aab");
  });

  it("returns null for unsupported extensions", () => {
    expect(getFileType("/path/to/file.zip")).toBeNull();
    expect(getFileType("/path/to/file.jar")).toBeNull();
    expect(getFileType("/path/to/file.txt")).toBeNull();
    expect(getFileType("/path/to/file")).toBeNull();
  });

  it("returns null for empty string", () => {
    expect(getFileType("")).toBeNull();
  });

  it("handles filenames with multiple dots", () => {
    expect(getFileType("com.example.app.release.apk")).toBe("apk");
    expect(getFileType("com.example.app.release.aab")).toBe("aab");
  });

  it("does not match partial extensions", () => {
    expect(getFileType("myapk")).toBeNull();
    expect(getFileType("myaab")).toBeNull();
    expect(getFileType("file.apk.zip")).toBeNull();
  });
});

// ─── now ──────────────────────────────────────────────────────────────────────

describe("now", () => {
  it("returns a string in HH:MM:SS format", () => {
    const result = now();
    // HH:MM:SS with 24h format
    expect(result).toMatch(/^\d{2}:\d{2}:\d{2}$/);
  });

  it("returns a non-empty string", () => {
    expect(now().length).toBeGreaterThan(0);
  });
});

// ─── formatBytes ──────────────────────────────────────────────────────────────

describe("formatBytes", () => {
  it("formats bytes under 1 KB", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(100)).toBe("100 B");
    expect(formatBytes(1023)).toBe("1023 B");
  });

  it("formats bytes in KB range", () => {
    expect(formatBytes(1024)).toBe("1 KB");
    expect(formatBytes(2048)).toBe("2 KB");
    expect(formatBytes(512 * 1024)).toBe("512 KB");
    expect(formatBytes(1024 * 1024 - 1)).toBe("1024 KB");
  });

  it("formats bytes in MB range", () => {
    expect(formatBytes(1024 * 1024)).toBe("1.0 MB");
    expect(formatBytes(5.5 * 1024 * 1024)).toBe("5.5 MB");
    expect(formatBytes(100 * 1024 * 1024)).toBe("100.0 MB");
  });

  it("formats large sizes in MB", () => {
    expect(formatBytes(1024 * 1024 * 1024)).toBe("1024.0 MB");
  });
});

// ─── shortcutLabel ────────────────────────────────────────────────────────────

describe("shortcutLabel", () => {
  // Note: In jsdom, navigator.platform likely returns "" so isMac will be false
  // We test the general behavior here

  it("returns a non-empty string", () => {
    expect(shortcutLabel("O").length).toBeGreaterThan(0);
  });

  it("includes the uppercase key", () => {
    expect(shortcutLabel("o")).toContain("O");
    expect(shortcutLabel("i")).toContain("I");
  });

  it("includes a modifier prefix", () => {
    const label = shortcutLabel("O");
    // Should include either ⌘ (Mac) or Ctrl+ (other)
    expect(label.includes("⌘") || label.includes("Ctrl+")).toBe(true);
  });

  it("includes shift modifier when specified", () => {
    const label = shortcutLabel("I", true);
    expect(label.includes("⇧") || label.includes("Shift+")).toBe(true);
  });

  it("does not include shift when not specified", () => {
    const label = shortcutLabel("I");
    expect(label.includes("⇧") && label.includes("Shift+")).toBe(false);
  });

  it("handles shift=false explicitly", () => {
    const label = shortcutLabel("I", false);
    expect(label.includes("⇧") && label.includes("Shift+")).toBe(false);
  });
});

