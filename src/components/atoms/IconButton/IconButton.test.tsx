import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { IconButton } from "./IconButton";

describe("IconButton", () => {
  it("should render children", () => {
    render(<IconButton>Click me</IconButton>);
    expect(screen.getByText("Click me")).toBeInTheDocument();
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

  it("should render with icon content", () => {
    render(
      <IconButton>
        <svg data-testid="icon" />
      </IconButton>,
    );
    expect(screen.getByTestId("icon")).toBeInTheDocument();
  });
});
