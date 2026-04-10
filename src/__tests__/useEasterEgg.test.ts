// ─── Tests for src/hooks/useEasterEgg.ts ──────────────────────────────────────
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useEasterEgg } from "../hooks/useEasterEgg";

describe("useEasterEgg", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("starts with easterEggVisible = false", () => {
    const { result } = renderHook(() => useEasterEgg());
    expect(result.current.easterEggVisible).toBe(false);
  });

  it("starts with easterEggIndex = 0", () => {
    const { result } = renderHook(() => useEasterEgg());
    expect(result.current.easterEggIndex).toBe(0);
  });

  it("provides an array of verses", () => {
    const { result } = renderHook(() => useEasterEgg());
    expect(result.current.easterEggVerses).toBeInstanceOf(Array);
    expect(result.current.easterEggVerses.length).toBeGreaterThan(0);
    expect(result.current.easterEggVerses[0]).toHaveProperty("text");
    expect(result.current.easterEggVerses[0]).toHaveProperty("ref");
  });

  it("does not show easter egg on fewer than 7 clicks", () => {
    const { result } = renderHook(() => useEasterEgg());
    for (let i = 0; i < 6; i++) {
      act(() => result.current.handleTitleClick());
    }
    expect(result.current.easterEggVisible).toBe(false);
  });

  it("shows easter egg after 7 rapid clicks", () => {
    const { result } = renderHook(() => useEasterEgg());
    for (let i = 0; i < 7; i++) {
      act(() => result.current.handleTitleClick());
    }
    expect(result.current.easterEggVisible).toBe(true);
  });

  it("hides easter egg after 6500ms", () => {
    const { result } = renderHook(() => useEasterEgg());
    for (let i = 0; i < 7; i++) {
      act(() => result.current.handleTitleClick());
    }
    expect(result.current.easterEggVisible).toBe(true);

    act(() => {
      vi.advanceTimersByTime(6500);
    });
    expect(result.current.easterEggVisible).toBe(false);
  });

  it("cycles to next verse index after dismissal", () => {
    const { result } = renderHook(() => useEasterEgg());

    // First trigger
    for (let i = 0; i < 7; i++) {
      act(() => result.current.handleTitleClick());
    }
    act(() => {
      vi.advanceTimersByTime(6500);
    });
    expect(result.current.easterEggIndex).toBe(1);
  });

  it("resets click counter if clicks are too slow (>2s apart)", () => {
    const { result } = renderHook(() => useEasterEgg());

    // Click 5 times
    for (let i = 0; i < 5; i++) {
      act(() => result.current.handleTitleClick());
    }

    // Wait >2s for timer to reset clicks
    act(() => {
      vi.advanceTimersByTime(2100);
    });

    // Click 2 more times (total would have been 7 if counter not reset)
    for (let i = 0; i < 2; i++) {
      act(() => result.current.handleTitleClick());
    }

    expect(result.current.easterEggVisible).toBe(false);
  });
});

