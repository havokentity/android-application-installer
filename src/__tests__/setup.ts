// ─── Vitest Test Setup ────────────────────────────────────────────────────────
import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";

// ─── Mock Tauri APIs (these don't exist in a jsdom test environment) ──────────

// Mock @tauri-apps/api/core
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

// Mock @tauri-apps/api/event
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

// Mock @tauri-apps/plugin-dialog
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

// Mock @tauri-apps/api/window
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    setTitle: vi.fn(),
    setSize: vi.fn(() => Promise.resolve()),
    setMinSize: vi.fn(() => Promise.resolve()),
    center: vi.fn(() => Promise.resolve()),
    setTheme: vi.fn(() => Promise.resolve()),
    onDragDropEvent: vi.fn(() => Promise.resolve(() => {})),
  })),
  LogicalSize: class LogicalSize {
    width: number;
    height: number;
    constructor(w: number, h: number) {
      this.width = w;
      this.height = h;
    }
  },
}));

// Mock @tauri-apps/api/app
vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn(() => Promise.resolve("1.3.2")),
}));

// ─── Mock navigator.clipboard ─────────────────────────────────────────────────
Object.assign(navigator, {
  clipboard: {
    writeText: vi.fn(() => Promise.resolve()),
  },
});

// ─── Mock localStorage ────────────────────────────────────────────────────────
const localStorageMock: Storage = (() => {
  let store: Record<string, string> = {};
  return {
    getItem: vi.fn((key: string) => store[key] ?? null),
    setItem: vi.fn((key: string, value: string) => { store[key] = value; }),
    removeItem: vi.fn((key: string) => { delete store[key]; }),
    clear: vi.fn(() => { store = {}; }),
    get length() { return Object.keys(store).length; },
    key: vi.fn((index: number) => Object.keys(store)[index] ?? null),
  };
})();

Object.defineProperty(window, "localStorage", { value: localStorageMock });

