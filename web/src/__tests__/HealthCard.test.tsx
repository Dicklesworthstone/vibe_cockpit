import { render, screen } from "@testing-library/react";
import React from "react";
import { HealthCard } from "@/components/HealthCard";

describe("HealthCard", () => {
  it("renders title and value", () => {
    render(<HealthCard title="Fleet Health" value="92%" />);
    expect(screen.getByText("Fleet Health")).toBeTruthy();
    expect(screen.getByText("92%")).toBeTruthy();
  });

  it("renders subtitle when provided", () => {
    render(<HealthCard title="Machines" value="3/4" subtitle="online" />);
    expect(screen.getByText("online")).toBeTruthy();
  });

  it("does not render subtitle when not provided", () => {
    render(<HealthCard title="Alerts" value={5} />);
    expect(screen.getByText("5")).toBeTruthy();
    expect(screen.getByText("Alerts")).toBeTruthy();
  });

  it("applies custom border color", () => {
    const { container } = render(<HealthCard title="Test" value="1" color="#dc2626" />);
    const card = container.firstElementChild as HTMLElement;
    expect(card.style.border).toContain("#dc2626");
  });

  it("uses default blue border", () => {
    const { container } = render(<HealthCard title="Test" value="1" />);
    const card = container.firstElementChild as HTMLElement;
    expect(card.style.border).toContain("#3b82f6");
  });
});
