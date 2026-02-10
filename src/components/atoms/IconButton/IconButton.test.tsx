import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { IconButton } from "./IconButton";

describe("IconButton", () => {
  it("should render children", () => {
    render(<IconButton>Click me</IconButton>);
    expect(screen.getByText("Click me")).toBeInTheDocument();
  });

  it("should render as a button element", () => {
    render(<IconButton>Test</IconButton>);
    expect(screen.getByRole("button")).toBeInTheDocument();
  });

  it("should have type='button' by default", () => {
    render(<IconButton>Test</IconButton>);
    expect(screen.getByRole("button")).toHaveAttribute("type", "button");
  });

  it("should call onClick when clicked", () => {
    const handleClick = vi.fn();
    render(<IconButton onClick={handleClick}>Click</IconButton>);

    fireEvent.click(screen.getByRole("button"));
    expect(handleClick).toHaveBeenCalledOnce();
  });

  it("should be disabled when disabled prop is true", () => {
    render(<IconButton disabled>Disabled</IconButton>);
    expect(screen.getByRole("button")).toBeDisabled();
  });

  it("should not call onClick when disabled", () => {
    const handleClick = vi.fn();
    render(
      <IconButton disabled onClick={handleClick}>
        Disabled
      </IconButton>,
    );

    fireEvent.click(screen.getByRole("button"));
    expect(handleClick).not.toHaveBeenCalled();
  });

  it("should apply default variant styles by default", () => {
    render(<IconButton>Default</IconButton>);
    const button = screen.getByRole("button");
    expect(button).toHaveClass("w-7");
    expect(button).toHaveClass("h-7");
  });

  it("should apply inline variant styles when variant is inline", () => {
    render(<IconButton variant="inline">Inline</IconButton>);
    const button = screen.getByRole("button");
    expect(button).toHaveClass("inline-flex");
    expect(button).toHaveClass("p-0");
  });

  it("should apply custom className", () => {
    render(<IconButton className="custom-class">Custom</IconButton>);
    expect(screen.getByRole("button")).toHaveClass("custom-class");
  });

  it("should forward additional props to button element", () => {
    render(<IconButton title="Test title">Props</IconButton>);
    expect(screen.getByRole("button")).toHaveAttribute("title", "Test title");
  });

  it("should render with icon content", () => {
    render(
      <IconButton>
        <svg data-testid="icon" />
      </IconButton>,
    );
    expect(screen.getByTestId("icon")).toBeInTheDocument();
  });
});
