// ─── Tests for src/components/LogPanel.tsx ────────────────────────────────────
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { LogPanel } from "../components/LogPanel";
import type { LogEntry } from "../types";

const makeLogs = (count: number): LogEntry[] =>
  Array.from({ length: count }, (_, i) => ({
    id: i + 1,
    time: `12:00:0${i}`,
    level: (["info", "success", "error", "warning"] as const)[i % 4],
    message: `Log message ${i + 1}`,
  }));

describe("LogPanel", () => {
  beforeEach(() => {
    // Mock scrollIntoView
    Element.prototype.scrollIntoView = vi.fn();
  });

  it("shows empty state when there are no logs", () => {
    render(<LogPanel logs={[]} onClear={vi.fn()} />);
    expect(screen.getByText(/No activity yet/)).toBeInTheDocument();
  });

  it("does not show Copy/Clear buttons when there are no logs", () => {
    render(<LogPanel logs={[]} onClear={vi.fn()} />);
    expect(screen.queryByText("Copy")).not.toBeInTheDocument();
    expect(screen.queryByText("Clear")).not.toBeInTheDocument();
  });

  it("renders all log entries", () => {
    const logs = makeLogs(4);
    render(<LogPanel logs={logs} onClear={vi.fn()} />);
    expect(screen.getByText("Log message 1")).toBeInTheDocument();
    expect(screen.getByText("Log message 2")).toBeInTheDocument();
    expect(screen.getByText("Log message 3")).toBeInTheDocument();
    expect(screen.getByText("Log message 4")).toBeInTheDocument();
  });

  it("renders log timestamps", () => {
    const logs = makeLogs(1);
    render(<LogPanel logs={logs} onClear={vi.fn()} />);
    expect(screen.getByText("12:00:00")).toBeInTheDocument();
  });

  it("applies correct CSS class for each log level", () => {
    const logs: LogEntry[] = [
      { id: 1, time: "00:00:00", level: "info", message: "info msg" },
      { id: 2, time: "00:00:01", level: "success", message: "success msg" },
      { id: 3, time: "00:00:02", level: "error", message: "error msg" },
      { id: 4, time: "00:00:03", level: "warning", message: "warning msg" },
    ];
    const { container } = render(<LogPanel logs={logs} onClear={vi.fn()} />);
    expect(container.querySelector(".log-info")).toBeInTheDocument();
    expect(container.querySelector(".log-success")).toBeInTheDocument();
    expect(container.querySelector(".log-error")).toBeInTheDocument();
    expect(container.querySelector(".log-warning")).toBeInTheDocument();
  });

  it("shows Copy and Clear buttons when logs exist", () => {
    render(<LogPanel logs={makeLogs(1)} onClear={vi.fn()} />);
    expect(screen.getByText("Copy")).toBeInTheDocument();
    expect(screen.getByText("Clear")).toBeInTheDocument();
  });

  it("calls onClear when Clear button is clicked", () => {
    const onClear = vi.fn();
    render(<LogPanel logs={makeLogs(2)} onClear={onClear} />);
    fireEvent.click(screen.getByText("Clear"));
    expect(onClear).toHaveBeenCalledOnce();
  });

  it("copies formatted logs to clipboard", async () => {
    const logs = makeLogs(2);
    render(<LogPanel logs={logs} onClear={vi.fn()} />);
    fireEvent.click(screen.getByText("Copy"));
    // Wait for the async clipboard operation
    await vi.waitFor(() => {
      expect(navigator.clipboard.writeText).toHaveBeenCalled();
    });
    const calledWith = (navigator.clipboard.writeText as ReturnType<typeof vi.fn>).mock.calls[0][0];
    expect(calledWith).toContain("[12:00:00] [INFO] Log message 1");
    expect(calledWith).toContain("[12:00:01] [SUCCESS] Log message 2");
  });

  it("renders the Log section header", () => {
    render(<LogPanel logs={[]} onClear={vi.fn()} />);
    expect(screen.getByText("Log")).toBeInTheDocument();
  });
});

