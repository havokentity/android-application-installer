// ─── Tests for src/components/ToolsSection.tsx ────────────────────────────────
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ToolsSection, StaleBanner } from "../components/ToolsSection";
import type { ToolsStatus, StaleTool, DownloadProgress } from "../types";

// ─── StaleBanner ──────────────────────────────────────────────────────────────

describe("StaleBanner", () => {
  const staleTool: StaleTool = {
    tool: "platform-tools",
    label: "ADB Platform-Tools",
    last_updated_secs: 0,
    age_days: 45,
  };

  it("renders nothing when staleTools is empty", () => {
    const { container } = render(<StaleBanner staleTools={[]} dismissed={false} onDismiss={vi.fn()} />);
    expect(container.innerHTML).toBe("");
  });

  it("renders nothing when dismissed is true", () => {
    const { container } = render(<StaleBanner staleTools={[staleTool]} dismissed={true} onDismiss={vi.fn()} />);
    expect(container.innerHTML).toBe("");
  });

  it("shows 'Updates available' when there are stale tools", () => {
    render(<StaleBanner staleTools={[staleTool]} dismissed={false} onDismiss={vi.fn()} />);
    expect(screen.getByText("Updates available")).toBeInTheDocument();
  });

  it("shows tool label in the banner", () => {
    render(<StaleBanner staleTools={[staleTool]} dismissed={false} onDismiss={vi.fn()} />);
    expect(screen.getByText(/ADB Platform-Tools/)).toBeInTheDocument();
  });

  it("uses singular 'hasn't' for one tool", () => {
    render(<StaleBanner staleTools={[staleTool]} dismissed={false} onDismiss={vi.fn()} />);
    expect(screen.getByText(/hasn't/)).toBeInTheDocument();
  });

  it("uses plural 'haven't' for multiple tools", () => {
    const tools: StaleTool[] = [
      staleTool,
      { tool: "bundletool", label: "bundletool", last_updated_secs: 0, age_days: 60 },
    ];
    render(<StaleBanner staleTools={tools} dismissed={false} onDismiss={vi.fn()} />);
    expect(screen.getByText(/haven't/)).toBeInTheDocument();
  });

  it("calls onDismiss when dismiss button is clicked", () => {
    const onDismiss = vi.fn();
    render(<StaleBanner staleTools={[staleTool]} dismissed={false} onDismiss={onDismiss} />);
    fireEvent.click(screen.getByTitle("Dismiss"));
    expect(onDismiss).toHaveBeenCalledOnce();
  });
});

// ─── ToolsSection ─────────────────────────────────────────────────────────────

describe("ToolsSection", () => {
  const allInstalled: ToolsStatus = {
    adb_installed: true,
    adb_path: "/path/to/adb",
    bundletool_installed: true,
    bundletool_path: "/path/to/bundletool.jar",
    java_installed: true,
    java_path: "/path/to/java",
    data_dir: "/data",
  };

  const noneInstalled: ToolsStatus = {
    adb_installed: false,
    adb_path: "",
    bundletool_installed: false,
    bundletool_path: "",
    java_installed: false,
    java_path: "",
    data_dir: "/data",
  };

  const defaults = {
    toolsStatus: noneInstalled,
    downloadingAdb: false,
    downloadingBundletool: false,
    downloadingJava: false,
    adbProgress: null as DownloadProgress | null,
    btProgress: null as DownloadProgress | null,
    javaProgress: null as DownloadProgress | null,
    onSetupAdb: vi.fn(),
    onSetupBundletool: vi.fn(),
    onSetupJava: vi.fn(),
  };

  it("renders the 'Required Tools' header", () => {
    render(<ToolsSection {...defaults} />);
    expect(screen.getByText("Required Tools")).toBeInTheDocument();
  });

  it("shows all three tool names", () => {
    render(<ToolsSection {...defaults} />);
    expect(screen.getByText("ADB (Android Debug Bridge)")).toBeInTheDocument();
    expect(screen.getByText("bundletool")).toBeInTheDocument();
    expect(screen.getByText("Java (Eclipse Temurin JRE 21)")).toBeInTheDocument();
  });

  it("shows 'Not installed' badges when tools are not installed", () => {
    render(<ToolsSection {...defaults} />);
    expect(screen.getAllByText("Not installed").length).toBe(3);
  });

  it("shows 'Installed' badges when all tools are installed", () => {
    render(<ToolsSection {...defaults} toolsStatus={allInstalled} />);
    expect(screen.getAllByText("Installed").length).toBe(3);
  });

  it("calls onSetupAdb when ADB Download button is clicked", () => {
    const onSetup = vi.fn();
    render(<ToolsSection {...defaults} onSetupAdb={onSetup} />);
    fireEvent.click(screen.getByText("Download ADB"));
    expect(onSetup).toHaveBeenCalledOnce();
  });

  it("shows download progress bar when downloading", () => {
    const progress: DownloadProgress = {
      tool: "platform-tools",
      downloaded: 5000000,
      total: 10000000,
      percent: 50,
      status: "downloading",
    };
    render(<ToolsSection {...defaults} downloadingAdb={true} adbProgress={progress} />);
    expect(screen.getByText(/50%/)).toBeInTheDocument();
  });

  it("shows 'Extracting...' during extraction phase", () => {
    const progress: DownloadProgress = {
      tool: "platform-tools",
      downloaded: 10000000,
      total: 10000000,
      percent: 100,
      status: "extracting",
    };
    render(<ToolsSection {...defaults} downloadingAdb={true} adbProgress={progress} />);
    expect(screen.getByText("Extracting...")).toBeInTheDocument();
  });

  it("disables download buttons while downloading", () => {
    render(<ToolsSection {...defaults} downloadingAdb={true} />);
    expect(screen.getByText("Downloading...").closest("button")).toBeDisabled();
  });

  it("shows 'Setup needed' badge when not all tools are installed (collapsible)", () => {
    render(<ToolsSection {...defaults} collapsible />);
    expect(screen.getByText("Setup needed")).toBeInTheDocument();
  });

  it("shows 'All installed' badge when all tools are installed (collapsible)", () => {
    render(<ToolsSection {...defaults} toolsStatus={allInstalled} collapsible />);
    expect(screen.getByText("All installed")).toBeInTheDocument();
  });

  it("shows Update button when a tool is already installed", () => {
    render(<ToolsSection {...defaults} toolsStatus={allInstalled} />);
    expect(screen.getAllByText("Update").length).toBe(3);
  });

  it("applies needs-attention class", () => {
    const { container } = render(<ToolsSection {...defaults} needsAttention />);
    expect(container.querySelector(".tools-attention")).toBeInTheDocument();
  });

  it("applies compact class", () => {
    const { container } = render(<ToolsSection {...defaults} compact />);
    expect(container.querySelector(".tools-compact")).toBeInTheDocument();
  });

  it("renders null toolsStatus gracefully", () => {
    render(<ToolsSection {...defaults} toolsStatus={null} />);
    expect(screen.getAllByText("Not installed").length).toBe(3);
  });
});

