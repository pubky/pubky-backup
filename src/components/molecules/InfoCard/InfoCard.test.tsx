import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { InfoCard } from "./InfoCard";

describe("InfoCard", () => {
  const mockIcon = <span data-testid="mock-icon">Icon</span>;

  it("should render label and value", () => {
    render(<InfoCard icon={mockIcon} label="Test Label" value="Test Value" />);

    expect(screen.getByText("Test Label")).toBeInTheDocument();
    expect(screen.getByText("Test Value")).toBeInTheDocument();
  });

  it("should render the icon", () => {
    render(<InfoCard icon={mockIcon} label="Label" value="Value" />);

    expect(screen.getByTestId("mock-icon")).toBeInTheDocument();
  });

  it("should render action slot when provided", () => {
    const action = <button data-testid="action-button">Action</button>;
    render(
      <InfoCard icon={mockIcon} label="Label" value="Value" action={action} />,
    );

    expect(screen.getByTestId("action-button")).toBeInTheDocument();
  });

  it("should not render action slot when not provided", () => {
    render(<InfoCard icon={mockIcon} label="Label" value="Value" />);

    expect(screen.queryByTestId("action-button")).not.toBeInTheDocument();
  });

  it("should apply flex-1 class when fullWidth is false (default)", () => {
    const { container } = render(
      <InfoCard icon={mockIcon} label="Label" value="Value" />,
    );

    expect(container.firstChild).toHaveClass("flex-1");
    expect(container.firstChild).not.toHaveClass("w-full");
  });

  it("should apply full width classes when fullWidth is true", () => {
    const { container } = render(
      <InfoCard icon={mockIcon} label="Label" value="Value" fullWidth />,
    );

    expect(container.firstChild).toHaveClass("w-full");
    expect(container.firstChild).not.toHaveClass("flex-1");
  });

  it("should apply text ellipsis to value when fullWidth is true", () => {
    render(
      <InfoCard icon={mockIcon} label="Label" value="Very Long Value" fullWidth />,
    );

    const value = screen.getByText("Very Long Value");
    expect(value).toHaveClass("overflow-hidden");
    expect(value).toHaveClass("text-ellipsis");
    expect(value).toHaveClass("whitespace-nowrap");
  });

  it("should not apply text ellipsis to value when fullWidth is false", () => {
    render(<InfoCard icon={mockIcon} label="Label" value="Value" />);

    const value = screen.getByText("Value");
    expect(value).not.toHaveClass("overflow-hidden");
  });

  it("should apply custom className", () => {
    const { container } = render(
      <InfoCard
        icon={mockIcon}
        label="Label"
        value="Value"
        className="custom-class"
      />,
    );

    expect(container.firstChild).toHaveClass("custom-class");
  });

  it("should display different values", () => {
    const { rerender } = render(
      <InfoCard icon={mockIcon} label="Size" value="1.5 MB" />,
    );

    expect(screen.getByText("1.5 MB")).toBeInTheDocument();

    rerender(<InfoCard icon={mockIcon} label="Size" value="2.3 GB" />);

    expect(screen.getByText("2.3 GB")).toBeInTheDocument();
  });
});
