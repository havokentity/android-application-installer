// ─── Tests for src/components/Toast.tsx ──────────────────────────────────────
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";
import { renderHook } from "@testing-library/react";
import { useToast, ToastContainer } from "../components/Toast";
import type { Toast } from "../components/Toast";

describe("useToast", () => {
  it("starts with an empty toast list", () => {
    const { result } = renderHook(() => useToast());
    expect(result.current.toasts).toEqual([]);
  });

  it("adds a toast with the correct message and level", () => {
    const { result } = renderHook(() => useToast());
    act(() => result.current.addToast("Hello", "success"));
    expect(result.current.toasts).toHaveLength(1);
    expect(result.current.toasts[0].message).toBe("Hello");
    expect(result.current.toasts[0].level).toBe("success");
  });

  it("defaults level to info", () => {
    const { result } = renderHook(() => useToast());
    act(() => result.current.addToast("Default level"));
    expect(result.current.toasts[0].level).toBe("info");
  });

  it("assigns unique ids to each toast", () => {
    const { result } = renderHook(() => useToast());
    act(() => {
      result.current.addToast("First", "info");
      result.current.addToast("Second", "error");
    });
    const ids = result.current.toasts.map((t) => t.id);
    expect(new Set(ids).size).toBe(2);
  });

  it("keeps at most 5 toasts", () => {
    const { result } = renderHook(() => useToast());
    act(() => {
      for (let i = 0; i < 7; i++) result.current.addToast(`Toast ${i}`);
    });
    expect(result.current.toasts.length).toBeLessThanOrEqual(5);
  });

  it("marks a toast as exiting on removeToast", () => {
    const { result } = renderHook(() => useToast());
    act(() => result.current.addToast("Bye", "warning"));
    const id = result.current.toasts[0].id;
    act(() => result.current.removeToast(id));
    expect(result.current.toasts[0].exiting).toBe(true);
  });

  it("fully removes a toast after the exit animation delay", () => {
    vi.useFakeTimers();
    const { result } = renderHook(() => useToast());
    act(() => result.current.addToast("Gone", "error"));
    const id = result.current.toasts[0].id;
    act(() => result.current.removeToast(id));
    act(() => vi.advanceTimersByTime(350));
    expect(result.current.toasts).toHaveLength(0);
    vi.useRealTimers();
  });

  it("auto-removes a toast after the duration", () => {
    vi.useFakeTimers();
    const { result } = renderHook(() => useToast(1000));
    act(() => result.current.addToast("Auto-dismiss", "info"));
    expect(result.current.toasts).toHaveLength(1);
    // After duration, it should start exiting
    act(() => vi.advanceTimersByTime(1000));
    // After exit animation
    act(() => vi.advanceTimersByTime(350));
    expect(result.current.toasts).toHaveLength(0);
    vi.useRealTimers();
  });
});

describe("ToastContainer", () => {
  it("renders nothing when there are no toasts", () => {
    const { container } = render(<ToastContainer toasts={[]} onDismiss={vi.fn()} />);
    expect(container.querySelector(".toast-container")).not.toBeInTheDocument();
  });

  it("renders toasts with correct messages", () => {
    const toasts: Toast[] = [
      { id: 1, message: "Success!", level: "success" },
      { id: 2, message: "Error!", level: "error" },
    ];
    render(<ToastContainer toasts={toasts} onDismiss={vi.fn()} />);
    expect(screen.getByText("Success!")).toBeInTheDocument();
    expect(screen.getByText("Error!")).toBeInTheDocument();
  });

  it("applies the correct level class", () => {
    const toasts: Toast[] = [
      { id: 1, message: "Warning!", level: "warning" },
    ];
    const { container } = render(<ToastContainer toasts={toasts} onDismiss={vi.fn()} />);
    expect(container.querySelector(".toast-warning")).toBeInTheDocument();
  });

  it("applies toast-enter class by default", () => {
    const toasts: Toast[] = [{ id: 1, message: "Entering", level: "info" }];
    const { container } = render(<ToastContainer toasts={toasts} onDismiss={vi.fn()} />);
    expect(container.querySelector(".toast-enter")).toBeInTheDocument();
  });

  it("applies toast-exit class when exiting", () => {
    const toasts: Toast[] = [{ id: 1, message: "Exiting", level: "info", exiting: true }];
    const { container } = render(<ToastContainer toasts={toasts} onDismiss={vi.fn()} />);
    expect(container.querySelector(".toast-exit")).toBeInTheDocument();
  });

  it("calls onDismiss with correct id when dismiss button is clicked", () => {
    const onDismiss = vi.fn();
    const toasts: Toast[] = [{ id: 42, message: "Dismiss me", level: "info" }];
    render(<ToastContainer toasts={toasts} onDismiss={onDismiss} />);
    fireEvent.click(screen.getByRole("button"));
    expect(onDismiss).toHaveBeenCalledWith(42);
  });

  it("renders all four levels correctly", () => {
    const toasts: Toast[] = [
      { id: 1, message: "S", level: "success" },
      { id: 2, message: "E", level: "error" },
      { id: 3, message: "W", level: "warning" },
      { id: 4, message: "I", level: "info" },
    ];
    const { container } = render(<ToastContainer toasts={toasts} onDismiss={vi.fn()} />);
    expect(container.querySelector(".toast-success")).toBeInTheDocument();
    expect(container.querySelector(".toast-error")).toBeInTheDocument();
    expect(container.querySelector(".toast-warning")).toBeInTheDocument();
    expect(container.querySelector(".toast-info")).toBeInTheDocument();
  });
});

