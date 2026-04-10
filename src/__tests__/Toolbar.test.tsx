// ─── Tests for src/components/Toolbar.tsx ─────────────────────────────────────
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { Toolbar } from "../components/Toolbar";

describe("Toolbar", () => {
  const defaults = {
    layout: "portrait" as const,
    theme: "dark" as const,
    onToggleLayout: vi.fn(),
    onSetTheme: vi.fn(),
    onCheckForUpdates: vi.fn(),
    checkingForUpdates: false,
  };

  it("renders Portrait and Landscape buttons", () => {
    render(<Toolbar {...defaults} />);
    expect(screen.getByText("Portrait")).toBeInTheDocument();
    expect(screen.getByText("Landscape")).toBeInTheDocument();
  });

  it("marks Portrait button as active when layout is portrait", () => {
    render(<Toolbar {...defaults} layout="portrait" />);
    const portraitBtn = screen.getByTitle("Portrait layout");
    expect(portraitBtn.className).toContain("active");
  });

  it("marks Landscape button as active when layout is landscape", () => {
    render(<Toolbar {...defaults} layout="landscape" />);
    const landscapeBtn = screen.getByTitle("Landscape layout");
    expect(landscapeBtn.className).toContain("active");
  });

  it("calls onToggleLayout with 'portrait' when Portrait is clicked", () => {
    const fn = vi.fn();
    render(<Toolbar {...defaults} onToggleLayout={fn} />);
    fireEvent.click(screen.getByTitle("Portrait layout"));
    expect(fn).toHaveBeenCalledWith("portrait");
  });

  it("calls onToggleLayout with 'landscape' when Landscape is clicked", () => {
    const fn = vi.fn();
    render(<Toolbar {...defaults} onToggleLayout={fn} />);
    fireEvent.click(screen.getByTitle("Landscape layout"));
    expect(fn).toHaveBeenCalledWith("landscape");
  });

  it("marks Light theme button as active when theme is light", () => {
    render(<Toolbar {...defaults} theme="light" />);
    expect(screen.getByTitle("Light theme").className).toContain("active");
    expect(screen.getByTitle("Dark theme").className).not.toContain("active");
  });

  it("marks Dark theme button as active when theme is dark", () => {
    render(<Toolbar {...defaults} theme="dark" />);
    expect(screen.getByTitle("Dark theme").className).toContain("active");
    expect(screen.getByTitle("Light theme").className).not.toContain("active");
  });

  it("calls onSetTheme with 'light' when Light is clicked", () => {
    const fn = vi.fn();
    render(<Toolbar {...defaults} onSetTheme={fn} />);
    fireEvent.click(screen.getByTitle("Light theme"));
    expect(fn).toHaveBeenCalledWith("light");
  });

  it("calls onSetTheme with 'dark' when Dark is clicked", () => {
    const fn = vi.fn();
    render(<Toolbar {...defaults} onSetTheme={fn} />);
    fireEvent.click(screen.getByTitle("Dark theme"));
    expect(fn).toHaveBeenCalledWith("dark");
  });

  it("renders the Updates button", () => {
    render(<Toolbar {...defaults} />);
    expect(screen.getByTitle("Check for updates")).toBeInTheDocument();
    expect(screen.getByText("Updates")).toBeInTheDocument();
  });

  it("calls onCheckForUpdates when Updates button is clicked", () => {
    const fn = vi.fn();
    render(<Toolbar {...defaults} onCheckForUpdates={fn} />);
    fireEvent.click(screen.getByTitle("Check for updates"));
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("disables Updates button and shows 'Checking…' when checkingForUpdates is true", () => {
    render(<Toolbar {...defaults} checkingForUpdates={true} />);
    const btn = screen.getByTitle("Check for updates");
    expect(btn).toBeDisabled();
    expect(screen.getByText("Checking…")).toBeInTheDocument();
  });
});

