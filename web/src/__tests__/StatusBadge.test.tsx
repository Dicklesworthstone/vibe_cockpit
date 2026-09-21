import { render, screen } from "@testing-library/react";
import React from "react";
import { StatusBadge } from "@/components/StatusBadge";

describe("StatusBadge", () => {
  it("renders status text", () => {
    render(<StatusBadge status="critical" />);
    expect(screen.getByText("critical")).toBeTruthy();
  });

  it("renders custom label", () => {
    render(<StatusBadge status="ok" label="healthy" />);
    expect(screen.getByText("healthy")).toBeTruthy();
  });

  it("applies critical color", () => {
    const { container } = render(<StatusBadge status="critical" />);
    const span = container.querySelector("span");
    expect(span?.style.backgroundColor).toBe("rgb(220, 38, 38)");
  });

  it("applies online color", () => {
    const { container } = render(<StatusBadge status="online" />);
    const span = container.querySelector("span");
    expect(span?.style.backgroundColor).toBe("rgb(22, 163, 74)");
  });

  it("falls back to gray for unknown status", () => {
    const { container } = render(<StatusBadge status="unknown" />);
    const span = container.querySelector("span");
    expect(span?.style.backgroundColor).toBe("rgb(107, 114, 128)");
  });
});
