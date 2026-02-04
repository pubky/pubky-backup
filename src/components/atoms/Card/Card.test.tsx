import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { Card } from "./Card";

describe("Card", () => {
  it("should render children", () => {
    render(
      <Card>
        <span>Test content</span>
      </Card>,
    );

    expect(screen.getByText("Test content")).toBeInTheDocument();
  });

  it("should have base card styles", () => {
    render(
      <Card>
        <span>Content</span>
      </Card>,
    );

    const card = screen.getByText("Content").parentElement;
    expect(card).toHaveClass("w-[360px]");
    expect(card).toHaveClass("min-h-[380px]");
    expect(card).toHaveClass("bg-surface-dark");
    expect(card).toHaveClass("border");
    expect(card).toHaveClass("border-border");
    expect(card).toHaveClass("rounded-lg");
  });

  it("should apply custom className", () => {
    render(
      <Card className="custom-class">
        <span>Content</span>
      </Card>,
    );

    const card = screen.getByText("Content").parentElement;
    expect(card).toHaveClass("custom-class");
  });

  it("should have flexbox layout", () => {
    render(
      <Card>
        <span>Content</span>
      </Card>,
    );

    const card = screen.getByText("Content").parentElement;
    expect(card).toHaveClass("flex");
    expect(card).toHaveClass("flex-col");
    expect(card).toHaveClass("justify-center");
    expect(card).toHaveClass("items-stretch");
  });

  it("should have padding and gap", () => {
    render(
      <Card>
        <span>Content</span>
      </Card>,
    );

    const card = screen.getByText("Content").parentElement;
    expect(card).toHaveClass("p-4");
    expect(card).toHaveClass("pb-5");
    expect(card).toHaveClass("px-5");
    expect(card).toHaveClass("gap-3");
  });

  it("should have shadow", () => {
    render(
      <Card>
        <span>Content</span>
      </Card>,
    );

    const card = screen.getByText("Content").parentElement;
    expect(card?.className).toContain("shadow");
  });
});
