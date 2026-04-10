// ─── Tests for src/hooks/useKeyboardShortcuts.ts ──────────────────────────────
import { describe, it, expect, vi, afterEach } from "vitest";
import { renderHook } from "@testing-library/react";
import { useKeyboardShortcuts } from "../hooks/useKeyboardShortcuts";

function fireKeydown(key: string, opts: Partial<KeyboardEvent> = {}) {
  const event = new KeyboardEvent("keydown", {
    key,
    bubbles: true,
    ...opts,
  });
  window.dispatchEvent(event);
}

describe("useKeyboardShortcuts", () => {
  const makeActions = (overrides = {}) => ({
    browseFile: vi.fn(),
    install: vi.fn(),
    launchApp: vi.fn(),
    uninstallApp: vi.fn(),
    canInstall: true as boolean | string | null,
    canLaunch: true,
    canUninstall: true,
    ...overrides,
  });

  afterEach(() => {
    // Clean up by unmounting hooks via re-renders
  });

  it("calls browseFile on Cmd/Ctrl+O", () => {
    const actions = makeActions();
    renderHook(() => useKeyboardShortcuts(actions));
    fireKeydown("o", { metaKey: true });
    expect(actions.browseFile).toHaveBeenCalledOnce();
  });

  it("calls browseFile on Ctrl+O (non-Mac)", () => {
    const actions = makeActions();
    renderHook(() => useKeyboardShortcuts(actions));
    fireKeydown("o", { ctrlKey: true });
    expect(actions.browseFile).toHaveBeenCalledOnce();
  });

  it("calls install(false) on Cmd/Ctrl+I", () => {
    const actions = makeActions();
    renderHook(() => useKeyboardShortcuts(actions));
    fireKeydown("i", { metaKey: true });
    expect(actions.install).toHaveBeenCalledWith(false);
  });

  it("calls install(true) on Cmd/Ctrl+Shift+I", () => {
    const actions = makeActions();
    renderHook(() => useKeyboardShortcuts(actions));
    fireKeydown("i", { metaKey: true, shiftKey: true });
    expect(actions.install).toHaveBeenCalledWith(true);
  });

  it("does not call install when canInstall is false", () => {
    const actions = makeActions({ canInstall: false });
    renderHook(() => useKeyboardShortcuts(actions));
    fireKeydown("i", { metaKey: true });
    expect(actions.install).not.toHaveBeenCalled();
  });

  it("does not call install when canInstall is null", () => {
    const actions = makeActions({ canInstall: null });
    renderHook(() => useKeyboardShortcuts(actions));
    fireKeydown("i", { metaKey: true });
    expect(actions.install).not.toHaveBeenCalled();
  });

  it("calls launchApp on Cmd/Ctrl+L", () => {
    const actions = makeActions();
    renderHook(() => useKeyboardShortcuts(actions));
    fireKeydown("l", { metaKey: true });
    expect(actions.launchApp).toHaveBeenCalledOnce();
  });

  it("does not call launchApp when canLaunch is false", () => {
    const actions = makeActions({ canLaunch: false });
    renderHook(() => useKeyboardShortcuts(actions));
    fireKeydown("l", { metaKey: true });
    expect(actions.launchApp).not.toHaveBeenCalled();
  });

  it("calls uninstallApp on Cmd/Ctrl+U", () => {
    const actions = makeActions();
    renderHook(() => useKeyboardShortcuts(actions));
    fireKeydown("u", { metaKey: true });
    expect(actions.uninstallApp).toHaveBeenCalledOnce();
  });

  it("does not call uninstallApp when canUninstall is false", () => {
    const actions = makeActions({ canUninstall: false });
    renderHook(() => useKeyboardShortcuts(actions));
    fireKeydown("u", { metaKey: true });
    expect(actions.uninstallApp).not.toHaveBeenCalled();
  });

  it("does nothing when no modifier key is held", () => {
    const actions = makeActions();
    renderHook(() => useKeyboardShortcuts(actions));
    fireKeydown("o");
    fireKeydown("i");
    fireKeydown("l");
    fireKeydown("u");
    expect(actions.browseFile).not.toHaveBeenCalled();
    expect(actions.install).not.toHaveBeenCalled();
    expect(actions.launchApp).not.toHaveBeenCalled();
    expect(actions.uninstallApp).not.toHaveBeenCalled();
  });

  it("does nothing for unrecognized keys", () => {
    const actions = makeActions();
    renderHook(() => useKeyboardShortcuts(actions));
    fireKeydown("z", { metaKey: true });
    fireKeydown("x", { ctrlKey: true });
    expect(actions.browseFile).not.toHaveBeenCalled();
    expect(actions.install).not.toHaveBeenCalled();
  });
});

