import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { PubkyInput } from "../PubkyInput";

describe("PubkyInput", () => {
  it("should render with placeholder text", () => {
    render(<PubkyInput value="" onChange={() => {}} />);

    expect(
      screen.getByPlaceholderText("Enter your pubky..."),
    ).toBeInTheDocument();
  });

  it("should render with custom placeholder", () => {
    render(
      <PubkyInput
        value=""
        onChange={() => {}}
        placeholder="g1b6wp8bhhxt..."
      />,
    );

    expect(screen.getByPlaceholderText("g1b6wp8bhhxt...")).toBeInTheDocument();
  });

  it("should display the current value", () => {
    render(
      <PubkyInput
        value="test-pubky-value"
        onChange={() => {}}
        suggestions={[]}
      />,
    );

    const input = screen.getByPlaceholderText("Enter your pubky...");
    expect(input).toHaveValue("test-pubky-value");
  });

  it("should call onChange when input value changes", () => {
    const handleChange = vi.fn();
    render(<PubkyInput value="" onChange={handleChange} suggestions={[]} />);

    const input = screen.getByPlaceholderText("Enter your pubky...");
    fireEvent.change(input, { target: { value: "new-value" } });

    expect(handleChange).toHaveBeenCalledWith("new-value");
  });

  it("should render suggestions in datalist when provided", () => {
    const suggestions = ["pubky-1", "pubky-2", "pubky-3"];
    render(
      <PubkyInput value="" onChange={() => {}} suggestions={suggestions} />,
    );

    const datalist = document.getElementById("previous-keys");
    expect(datalist).toBeInTheDocument();
    expect(datalist?.querySelectorAll("option")).toHaveLength(3);
  });

  it("should not render datalist when no suggestions", () => {
    render(<PubkyInput value="" onChange={() => {}} suggestions={[]} />);

    const datalist = document.getElementById("previous-keys");
    expect(datalist).not.toBeInTheDocument();
  });

  it("should have dashed border when empty", () => {
    const { container } = render(<PubkyInput value="" onChange={() => {}} />);

    const wrapper = container.firstChild as HTMLElement;
    expect(wrapper).toHaveClass("border-dashed");
  });

  it("should have solid border when has value", () => {
    const { container } = render(
      <PubkyInput value="some-value" onChange={() => {}} />,
    );

    const wrapper = container.firstChild as HTMLElement;
    expect(wrapper).toHaveClass("border-solid");
  });

  it("should hide placeholder on focus", () => {
    render(<PubkyInput value="" onChange={() => {}} suggestions={[]} />);

    const input = screen.getByPlaceholderText("Enter your pubky...");
    fireEvent.focus(input);

    expect(input).toHaveAttribute("placeholder", "");
  });

  it("should show placeholder on blur when empty", () => {
    render(<PubkyInput value="" onChange={() => {}} suggestions={[]} />);

    const input = screen.getByPlaceholderText("Enter your pubky...");
    fireEvent.focus(input);
    fireEvent.blur(input);

    expect(input).toHaveAttribute("placeholder", "Enter your pubky...");
  });

  it("should apply custom className", () => {
    const { container } = render(
      <PubkyInput value="" onChange={() => {}} className="custom-class" />,
    );

    const wrapper = container.firstChild as HTMLElement;
    expect(wrapper).toHaveClass("custom-class");
  });

  it("should have autocomplete off", () => {
    render(<PubkyInput value="" onChange={() => {}} suggestions={[]} />);

    const input = screen.getByPlaceholderText("Enter your pubky...");
    expect(input).toHaveAttribute("autocomplete", "off");
  });

  it("should be linked to datalist", () => {
    render(
      <PubkyInput
        value=""
        onChange={() => {}}
        suggestions={["suggestion-1"]}
      />,
    );

    // When there are suggestions, the input becomes a combobox
    const input = screen.getByRole("combobox");
    expect(input).toHaveAttribute("list", "previous-keys");
  });
});
