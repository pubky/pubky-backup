import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { DashboardHeader } from "../DashboardHeader";

describe("DashboardHeader", () => {
  const defaultProps = {
    pubkyDisplay: "abcde...vwxyz",
    status: "synced" as const,
    onBack: vi.fn(),
    onCopy: vi.fn(),
  };

  it("should render the pubky display text", () => {
    render(<DashboardHeader {...defaultProps} />);

    expect(screen.getByText("abcde...vwxyz")).toBeInTheDocument();
  });

  it("should render the status badge with synced status", () => {
    render(<DashboardHeader {...defaultProps} status="synced" />);

    expect(screen.getByText("SYNCED")).toBeInTheDocument();
  });

  it("should render the status badge with syncing status", () => {
    render(<DashboardHeader {...defaultProps} status="syncing" />);

    expect(screen.getByText("SYNCING")).toBeInTheDocument();
  });

  it("should render the status badge with snapshot status", () => {
    render(<DashboardHeader {...defaultProps} status="snapshot" />);

    expect(screen.getByText("SNAPSHOT!")).toBeInTheDocument();
  });

  it("should call onBack when back button is clicked", () => {
    const onBack = vi.fn();
    render(<DashboardHeader {...defaultProps} onBack={onBack} />);

    const backButton = screen.getByTitle("Back");
    fireEvent.click(backButton);

    expect(onBack).toHaveBeenCalledOnce();
  });

  it("should call onCopy when copy button is clicked", () => {
    const onCopy = vi.fn();
    render(<DashboardHeader {...defaultProps} onCopy={onCopy} />);

    const copyButton = screen.getByTitle("Copy full pubky");
    fireEvent.click(copyButton);

    expect(onCopy).toHaveBeenCalledOnce();
  });

  it("should render back and copy buttons", () => {
    render(<DashboardHeader {...defaultProps} />);

    expect(screen.getByTitle("Back")).toBeInTheDocument();
    expect(screen.getByTitle("Copy full pubky")).toBeInTheDocument();
  });

  it("should display different pubky values", () => {
    const { rerender } = render(
      <DashboardHeader {...defaultProps} pubkyDisplay="first...pubky" />,
    );

    expect(screen.getByText("first...pubky")).toBeInTheDocument();

    rerender(
      <DashboardHeader {...defaultProps} pubkyDisplay="second...value" />,
    );

    expect(screen.getByText("second...value")).toBeInTheDocument();
  });
});
