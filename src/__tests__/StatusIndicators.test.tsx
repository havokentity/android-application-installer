// ─── Tests for src/components/StatusIndicators.tsx ────────────────────────────
import { describe, it, expect } from "vitest";
import { render } from "@testing-library/react";
import { StatusDot, LogIcon } from "../components/StatusIndicators";

describe("StatusDot", () => {
  it("renders a green dot when status is 'found'", () => {
    const { container } = render(<StatusDot status="found" />);
    expect(container.querySelector("span")).toHaveClass("status-dot", "green");
  });

  it("renders a red dot when status is 'not-found'", () => {
    const { container } = render(<StatusDot status="not-found" />);
    expect(container.querySelector("span")).toHaveClass("status-dot", "red");
  });

  it("renders a gray dot when status is 'unknown'", () => {
    const { container } = render(<StatusDot status="unknown" />);
    expect(container.querySelector("span")).toHaveClass("status-dot", "gray");
  });
});

describe("LogIcon", () => {
  it("renders green icon for 'success'", () => {
    const { container } = render(<LogIcon level="success" />);
    expect(container.querySelector("svg")).toHaveClass("log-icon", "green");
  });

  it("renders red icon for 'error'", () => {
    const { container } = render(<LogIcon level="error" />);
    expect(container.querySelector("svg")).toHaveClass("log-icon", "red");
  });

  it("renders yellow icon for 'warning'", () => {
    const { container } = render(<LogIcon level="warning" />);
    expect(container.querySelector("svg")).toHaveClass("log-icon", "yellow");
  });

  it("renders blue icon for 'info'", () => {
    const { container } = render(<LogIcon level="info" />);
    expect(container.querySelector("svg")).toHaveClass("log-icon", "blue");
  });
});

