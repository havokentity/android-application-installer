// ─── Tests for src/hooks/useLayout.ts ─────────────────────────────────────────
import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useLayout } from "../hooks/useLayout";

describe("useLayout", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("returns default layout as 'landscape'", () => {
    const { result } = renderHook(() => useLayout());
    expect(result.current.layout).toBe("landscape");
  });

  it("returns default theme as 'dark'", () => {
    const { result } = renderHook(() => useLayout());
    expect(result.current.theme).toBe("dark");
  });

  it("reads layout from localStorage if set", () => {
    localStorage.setItem("layout", "portrait");
    const { result } = renderHook(() => useLayout());
    expect(result.current.layout).toBe("portrait");
  });

  it("reads theme from localStorage if set", () => {
    localStorage.setItem("theme", "light");
    const { result } = renderHook(() => useLayout());
    expect(result.current.theme).toBe("light");
  });

  it("reads side panel width from localStorage", () => {
    localStorage.setItem("landscapeWidth", "500");
    const { result } = renderHook(() => useLayout());
    expect(result.current.sidePanelWidth).toBe(500);
  });

  it("defaults side panel width to 340 when not saved", () => {
    const { result } = renderHook(() => useLayout());
    expect(result.current.sidePanelWidth).toBe(340);
  });

  it("setTheme updates the theme", () => {
    const { result } = renderHook(() => useLayout());
    act(() => {
      result.current.setTheme("light");
    });
    expect(result.current.theme).toBe("light");
  });

  it("toggleLayout switches to portrait", async () => {
    const { result } = renderHook(() => useLayout());
    await act(async () => {
      await result.current.toggleLayout("portrait");
    });
    expect(result.current.layout).toBe("portrait");
  });

  it("toggleLayout switches to landscape", async () => {
    localStorage.setItem("layout", "portrait");
    const { result } = renderHook(() => useLayout());
    await act(async () => {
      await result.current.toggleLayout("landscape");
    });
    expect(result.current.layout).toBe("landscape");
  });

  it("toggleLayout persists layout to localStorage", async () => {
    const { result } = renderHook(() => useLayout());
    await act(async () => {
      await result.current.toggleLayout("portrait");
    });
    expect(localStorage.setItem).toHaveBeenCalledWith("layout", "portrait");
  });

  it("setTheme persists theme to localStorage", () => {
    const { result } = renderHook(() => useLayout());
    act(() => {
      result.current.setTheme("light");
    });
    expect(localStorage.setItem).toHaveBeenCalledWith("theme", "light");
  });

  it("provides an appRef", () => {
    const { result } = renderHook(() => useLayout());
    expect(result.current.appRef).toBeDefined();
    expect(result.current.appRef.current).toBe(null);
  });

  it("provides an onDividerMouseDown handler", () => {
    const { result } = renderHook(() => useLayout());
    expect(typeof result.current.onDividerMouseDown).toBe("function");
  });
});

