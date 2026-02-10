import { describe, it, expect } from "vitest";
import { render } from "@testing-library/react";
import { Spinner } from "./Spinner";

describe("Spinner", () => {
  it("should render an SVG element", () => {
    const { container } = render(<Spinner />);
    const svg = container.querySelector("svg");
    expect(svg).toBeInTheDocument();
  });

  it("should have animate-spin class by default", () => {
    const { container } = render(<Spinner />);
    const svg = container.querySelector("svg");
    expect(svg).toHaveClass("animate-spin");
  });

  it("should use default size of 16", () => {
    const { container } = render(<Spinner />);
    const svg = container.querySelector("svg");
    expect(svg).toHaveAttribute("width", "16");
    expect(svg).toHaveAttribute("height", "16");
  });

  it("should accept custom size", () => {
    const { container } = render(<Spinner size={24} />);
    const svg = container.querySelector("svg");
    expect(svg).toHaveAttribute("width", "24");
    expect(svg).toHaveAttribute("height", "24");
  });

  it("should accept custom className", () => {
    const { container } = render(<Spinner className="text-red-500" />);
    const svg = container.querySelector("svg");
    expect(svg).toHaveClass("animate-spin");
    expect(svg).toHaveClass("text-red-500");
  });

  it("should use currentColor for stroke", () => {
    const { container } = render(<Spinner />);
    const path = container.querySelector("path");
    expect(path).toHaveAttribute("stroke", "currentColor");
  });
});
