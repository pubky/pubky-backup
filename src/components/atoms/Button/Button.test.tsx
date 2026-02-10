import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { Button } from "./Button";

describe("Button", () => {
  it("should render children", () => {
    render(<Button>Click me</Button>);
    expect(screen.getByText("Click me")).toBeInTheDocument();
  });

  it("should call onClick when clicked", () => {
    const handleClick = vi.fn();
    render(<Button onClick={handleClick}>Click</Button>);

    fireEvent.click(screen.getByRole("button"));
    expect(handleClick).toHaveBeenCalledOnce();
  });

  it("should be disabled when disabled prop is true", () => {
    render(<Button disabled>Disabled</Button>);

    const button = screen.getByRole("button");
    expect(button).toBeDisabled();
  });

  it("should have disabled styles", () => {
    render(<Button disabled>Disabled</Button>);

    const button = screen.getByRole("button");
    expect(button).toHaveClass("disabled:cursor-not-allowed");
  });

  it("should have type button by default", () => {
    render(<Button>Default</Button>);

    const button = screen.getByRole("button");
    expect(button).toHaveAttribute("type", "button");
  });

  it("should support submit type", () => {
    render(<Button type="submit">Submit</Button>);

    const button = screen.getByRole("button");
    expect(button).toHaveAttribute("type", "submit");
  });

  describe("variants", () => {
    it("should have primary variant by default", () => {
      render(<Button>Primary</Button>);

      const button = screen.getByRole("button");
      expect(button).toHaveClass("bg-pubky-purple/15");
      expect(button).toHaveClass("border-pubky-purple");
    });

    it("should support primary variant explicitly", () => {
      render(<Button variant="primary">Primary</Button>);

      const button = screen.getByRole("button");
      expect(button).toHaveClass("bg-pubky-purple/15");
      expect(button).toHaveClass("border-pubky-purple");
    });

    it("should support secondary variant", () => {
      render(<Button variant="secondary">Secondary</Button>);

      const button = screen.getByRole("button");
      expect(button).toHaveClass("bg-button-hover-bg");
      expect(button).toHaveClass("border-border");
    });
  });

  it("should have base button styles", () => {
    render(<Button>Base</Button>);

    const button = screen.getByRole("button");
    expect(button).toHaveClass("flex");
    expect(button).toHaveClass("justify-center");
    expect(button).toHaveClass("items-center");
    expect(button).toHaveClass("gap-2");
    expect(button).toHaveClass("py-5");
    expect(button).toHaveClass("px-8");
    expect(button).toHaveClass("rounded-full");
    expect(button).toHaveClass("cursor-pointer");
    expect(button).toHaveClass("transition-all");
    expect(button).toHaveClass("duration-200");
  });

  it("should apply custom className", () => {
    render(<Button className="custom-class">Custom</Button>);

    const button = screen.getByRole("button");
    expect(button).toHaveClass("custom-class");
  });

  it("should not call onClick when disabled", () => {
    const handleClick = vi.fn();
    render(
      <Button onClick={handleClick} disabled>
        Disabled
      </Button>,
    );

    fireEvent.click(screen.getByRole("button"));
    expect(handleClick).not.toHaveBeenCalled();
  });
});
