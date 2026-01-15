import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { StatusBadge } from "../StatusBadge";

describe("StatusBadge", () => {
  it("should display SYNCED by default", () => {
    render(<StatusBadge />);
    expect(screen.getByText("SYNCED")).toBeInTheDocument();
  });

  it("should display SYNCED when status is synced", () => {
    render(<StatusBadge status="synced" />);
    expect(screen.getByText("SYNCED")).toBeInTheDocument();
  });

  it("should display SYNCING when status is syncing", () => {
    render(<StatusBadge status="syncing" />);
    expect(screen.getByText("SYNCING")).toBeInTheDocument();
  });

  it("should display SNAPSHOT! when status is snapshot", () => {
    render(<StatusBadge status="snapshot" />);
    expect(screen.getByText("SNAPSHOT!")).toBeInTheDocument();
  });

  it("should apply green background for synced status", () => {
    render(<StatusBadge status="synced" />);
    const badge = screen.getByText("SYNCED");
    expect(badge).toHaveClass("bg-pubky-green");
  });

  it("should apply purple background for syncing status", () => {
    render(<StatusBadge status="syncing" />);
    const badge = screen.getByText("SYNCING");
    expect(badge).toHaveClass("bg-pubky-purple");
  });

  it("should apply blue background for snapshot status", () => {
    render(<StatusBadge status="snapshot" />);
    const badge = screen.getByText("SNAPSHOT!");
    expect(badge).toHaveClass("bg-pubky-blue");
  });

  it("should apply custom className", () => {
    render(<StatusBadge status="synced" className="custom-class" />);
    const badge = screen.getByText("SYNCED");
    expect(badge).toHaveClass("custom-class");
  });
});
