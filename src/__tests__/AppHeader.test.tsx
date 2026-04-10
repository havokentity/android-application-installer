// ─── Tests for src/components/AppHeader.tsx ───────────────────────────────────
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { AppHeader } from "../components/AppHeader";

describe("AppHeader", () => {
  it("renders the title", () => {
    render(<AppHeader appVersion="1.0.0" onTitleClick={vi.fn()} />);
    expect(screen.getByText("Android Application Installer")).toBeInTheDocument();
  });

  it("renders the subtitle", () => {
    render(<AppHeader appVersion="" onTitleClick={vi.fn()} />);
    expect(screen.getByText(/Install APK & AAB files/)).toBeInTheDocument();
  });

  it("shows version badge when appVersion is provided", () => {
    render(<AppHeader appVersion="1.3.2" onTitleClick={vi.fn()} />);
    expect(screen.getByText("v1.3.2")).toBeInTheDocument();
  });

  it("does not show version badge when appVersion is empty", () => {
    render(<AppHeader appVersion="" onTitleClick={vi.fn()} />);
    expect(screen.queryByText(/^v/)).not.toBeInTheDocument();
  });

  it("calls onTitleClick when the title h1 is clicked", () => {
    const onClick = vi.fn();
    render(<AppHeader appVersion="1.0.0" onTitleClick={onClick} />);
    fireEvent.click(screen.getByText("Android Application Installer"));
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("calls onTitleClick multiple times on multiple clicks", () => {
    const onClick = vi.fn();
    render(<AppHeader appVersion="1.0.0" onTitleClick={onClick} />);
    const title = screen.getByText("Android Application Installer");
    fireEvent.click(title);
    fireEvent.click(title);
    fireEvent.click(title);
    expect(onClick).toHaveBeenCalledTimes(3);
  });
});

