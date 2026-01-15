import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { SyncMessage } from "./SyncMessage";

describe("SyncMessage", () => {
  it("should display the provided message", () => {
    render(<SyncMessage message="Data synchronized" />);
    expect(screen.getByText("Data synchronized")).toBeInTheDocument();
  });

  it("should apply green styling for synced status", () => {
    render(<SyncMessage status="synced" message="All synced" />);
    const text = screen.getByText("All synced");
    expect(text).toHaveClass("text-pubky-green");
  });

  it("should apply purple styling for syncing status", () => {
    render(<SyncMessage status="syncing" message="Syncing data..." />);
    const text = screen.getByText("Syncing data...");
    expect(text).toHaveClass("text-pubky-purple");
  });

  it("should apply red styling for error status", () => {
    render(<SyncMessage status="error" message="Sync failed" />);
    const text = screen.getByText("Sync failed");
    expect(text).toHaveClass("text-pubky-red");
  });

  it("should apply blue styling for snapshot-success status", () => {
    render(<SyncMessage status="snapshot-success" message="Snapshot created" />);
    const text = screen.getByText("Snapshot created");
    expect(text).toHaveClass("text-pubky-blue");
  });

  it("should show spinner icon when syncing", () => {
    const { container } = render(
      <SyncMessage status="syncing" message="Syncing..." />,
    );
    // Spinner has animate-spin class
    const spinner = container.querySelector(".animate-spin");
    expect(spinner).toBeInTheDocument();
  });

  it("should show check icon when not syncing", () => {
    const { container } = render(
      <SyncMessage status="synced" message="Done" />,
    );
    // Check for SVG with check path (CheckIcon)
    const svg = container.querySelector("svg");
    expect(svg).toBeInTheDocument();
    // Should not have spinner
    const spinner = container.querySelector(".animate-spin");
    expect(spinner).not.toBeInTheDocument();
  });

  it("should apply custom className", () => {
    const { container } = render(
      <SyncMessage message="Test" className="custom-class" />,
    );
    expect(container.firstChild).toHaveClass("custom-class");
  });

  it("should apply correct background for synced status", () => {
    const { container } = render(
      <SyncMessage status="synced" message="Synced" />,
    );
    expect(container.firstChild).toHaveClass("bg-status-synced-bg");
  });

  it("should apply correct background for syncing status", () => {
    const { container } = render(
      <SyncMessage status="syncing" message="Syncing" />,
    );
    expect(container.firstChild).toHaveClass("bg-status-syncing-bg");
  });

  it("should apply correct background for error status", () => {
    const { container } = render(
      <SyncMessage status="error" message="Error" />,
    );
    expect(container.firstChild).toHaveClass("bg-status-error-bg");
  });

  it("should apply correct background for snapshot-success status", () => {
    const { container } = render(
      <SyncMessage status="snapshot-success" message="Snapshot" />,
    );
    expect(container.firstChild).toHaveClass("bg-status-snapshot-bg");
  });
});
