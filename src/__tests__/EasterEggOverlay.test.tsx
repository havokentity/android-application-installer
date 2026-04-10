// ─── Tests for src/components/EasterEggOverlay.tsx ────────────────────────────
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { EasterEggOverlay } from "../components/EasterEggOverlay";

const verse = { text: "Test verse text", ref: "Test 1:1" };

describe("EasterEggOverlay", () => {
  it("renders nothing when visible is false", () => {
    const { container } = render(<EasterEggOverlay visible={false} verse={verse} />);
    expect(container.innerHTML).toBe("");
  });

  it("renders the verse text when visible is true", () => {
    render(<EasterEggOverlay visible={true} verse={verse} />);
    expect(screen.getByText(/Test verse text/)).toBeInTheDocument();
  });

  it("renders the verse reference when visible", () => {
    render(<EasterEggOverlay visible={true} verse={verse} />);
    expect(screen.getByText(/Test 1:1/)).toBeInTheDocument();
  });

  it("renders with the overlay class", () => {
    const { container } = render(<EasterEggOverlay visible={true} verse={verse} />);
    expect(container.querySelector(".easter-egg-overlay")).toBeInTheDocument();
  });

  it("wraps text in quotes", () => {
    render(<EasterEggOverlay visible={true} verse={verse} />);
    const textEl = screen.getByText(/Test verse text/);
    expect(textEl.textContent).toContain("\u201c"); // left double quote
    expect(textEl.textContent).toContain("\u201d"); // right double quote
  });

  it("prefixes reference with em dash", () => {
    render(<EasterEggOverlay visible={true} verse={verse} />);
    const refEl = screen.getByText(/Test 1:1/);
    expect(refEl.textContent).toContain("—");
  });
});

