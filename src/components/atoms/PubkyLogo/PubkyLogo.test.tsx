import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { PubkyLogo } from "./PubkyLogo";

describe("PubkyLogo", () => {
  it("should render the Pubky text", () => {
    render(<PubkyLogo />);
    expect(screen.getByText("Pubky")).toBeInTheDocument();
  });

  it("should render the Backup text", () => {
    render(<PubkyLogo />);
    expect(screen.getByText("Backup")).toBeInTheDocument();
  });

  it("should render an SVG logo", () => {
    const { container } = render(<PubkyLogo />);
    const svg = container.querySelector("svg");
    expect(svg).toBeInTheDocument();
  });

  it("should have correct container dimensions", () => {
    const { container } = render(<PubkyLogo />);
    const logoContainer = container.firstChild;
    expect(logoContainer).toHaveClass("w-[272px]");
    expect(logoContainer).toHaveClass("h-12");
  });

  it("should have Pubky text with bold weight", () => {
    render(<PubkyLogo />);
    const pubkyText = screen.getByText("Pubky");
    expect(pubkyText).toHaveClass("font-bold");
  });

  it("should have Backup text with light weight", () => {
    render(<PubkyLogo />);
    const backupText = screen.getByText("Backup");
    expect(backupText).toHaveClass("font-light");
  });

  it("should use white text color", () => {
    render(<PubkyLogo />);
    const pubkyText = screen.getByText("Pubky");
    const backupText = screen.getByText("Backup");
    expect(pubkyText).toHaveClass("text-white");
    expect(backupText).toHaveClass("text-white");
  });

  it("should render with flex layout", () => {
    const { container } = render(<PubkyLogo />);
    const logoContainer = container.firstChild;
    expect(logoContainer).toHaveClass("flex");
    expect(logoContainer).toHaveClass("items-center");
    expect(logoContainer).toHaveClass("justify-center");
  });
});
