import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/react";
import { DashboardHeader } from "./DashboardHeader";

describe("DashboardHeader", () => {
  const defaultProps = {
    pubkyDisplay: "abcde...vwxyz",
    status: "synced" as const,
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

  it("should call onCopy when copy button is clicked", () => {
    const onCopy = vi.fn();
    render(<DashboardHeader {...defaultProps} onCopy={onCopy} />);

    const copyButton = screen.getByTitle("Copy full pubky");
    fireEvent.click(copyButton);

    expect(onCopy).toHaveBeenCalledOnce();
  });

  it("should render copy button", () => {
    render(<DashboardHeader {...defaultProps} />);

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

  describe("key dropdown", () => {
    const multiKeyProps = {
      ...defaultProps,
      keys: [
        { pubky: "abc123def456ghi789", isSelected: true },
        { pubky: "xyz987uvw654rst321", isSelected: false },
      ],
      onSelectKey: vi.fn(),
    };

    it("should not show dropdown button when no keys provided", () => {
      render(<DashboardHeader {...defaultProps} />);

      expect(screen.queryByTitle("Switch key")).not.toBeInTheDocument();
    });

    it("should not show dropdown button when only one key", () => {
      render(
        <DashboardHeader
          {...defaultProps}
          keys={[{ pubky: "single-key", isSelected: true }]}
        />,
      );

      expect(screen.queryByTitle("Switch key")).not.toBeInTheDocument();
    });

    it("should show dropdown button when multiple keys provided", () => {
      render(<DashboardHeader {...multiKeyProps} />);

      expect(screen.getByTitle("Switch key")).toBeInTheDocument();
    });

    it("should have aria-expanded=false when dropdown is closed", () => {
      render(<DashboardHeader {...multiKeyProps} />);

      const toggleButton = screen.getByTitle("Switch key");
      expect(toggleButton).toHaveAttribute("aria-expanded", "false");
    });

    it("should have aria-haspopup=listbox on toggle button", () => {
      render(<DashboardHeader {...multiKeyProps} />);

      const toggleButton = screen.getByTitle("Switch key");
      expect(toggleButton).toHaveAttribute("aria-haspopup", "listbox");
    });

    it("should have aria-label on listbox", () => {
      render(<DashboardHeader {...multiKeyProps} />);

      fireEvent.click(screen.getByTitle("Switch key"));

      const listbox = screen.getByRole("listbox");
      expect(listbox).toHaveAttribute("aria-label", "Select key");
    });

    it("should toggle dropdown open/closed on chevron click", () => {
      render(<DashboardHeader {...multiKeyProps} />);

      const toggleButton = screen.getByTitle("Switch key");

      // Initially closed
      expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
      expect(toggleButton).toHaveAttribute("aria-expanded", "false");

      // Open dropdown
      fireEvent.click(toggleButton);
      expect(screen.getByRole("listbox")).toBeInTheDocument();
      expect(toggleButton).toHaveAttribute("aria-expanded", "true");

      // Close dropdown
      fireEvent.click(toggleButton);
      expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
      expect(toggleButton).toHaveAttribute("aria-expanded", "false");
    });

    it("should display all keys in dropdown when open", () => {
      render(<DashboardHeader {...multiKeyProps} />);

      fireEvent.click(screen.getByTitle("Switch key"));

      const listbox = screen.getByRole("listbox");
      const options = within(listbox).getAllByRole("option");
      expect(options).toHaveLength(2);
    });

    it("should show aria-selected for selected key", () => {
      render(<DashboardHeader {...multiKeyProps} />);

      fireEvent.click(screen.getByTitle("Switch key"));

      const options = screen.getAllByRole("option");
      expect(options[0]).toHaveAttribute("aria-selected", "true");
      expect(options[1]).toHaveAttribute("aria-selected", "false");
    });

    it("should call onSelectKey with correct pubky when key clicked", () => {
      const onSelectKey = vi.fn();
      render(
        <DashboardHeader {...multiKeyProps} onSelectKey={onSelectKey} />,
      );

      fireEvent.click(screen.getByTitle("Switch key"));
      const options = screen.getAllByRole("option");
      const secondOption = options[1];
      if (!secondOption) throw new Error("Second option not found");
      fireEvent.click(secondOption); // Click the non-selected key

      expect(onSelectKey).toHaveBeenCalledOnce();
      expect(onSelectKey).toHaveBeenCalledWith("xyz987uvw654rst321");
    });

    it("should close dropdown after selecting a key", () => {
      render(<DashboardHeader {...multiKeyProps} />);

      fireEvent.click(screen.getByTitle("Switch key"));
      expect(screen.getByRole("listbox")).toBeInTheDocument();

      const options = screen.getAllByRole("option");
      const secondOption = options[1];
      if (!secondOption) throw new Error("Second option not found");
      fireEvent.click(secondOption);

      expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    });

    it("should close dropdown when clicking outside", () => {
      render(
        <div>
          <DashboardHeader {...multiKeyProps} />
          <button data-testid="outside">Outside</button>
        </div>,
      );

      fireEvent.click(screen.getByTitle("Switch key"));
      expect(screen.getByRole("listbox")).toBeInTheDocument();

      fireEvent.mouseDown(screen.getByTestId("outside"));
      expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    });

    it("should close dropdown when Escape is pressed", () => {
      render(<DashboardHeader {...multiKeyProps} />);

      fireEvent.click(screen.getByTitle("Switch key"));
      expect(screen.getByRole("listbox")).toBeInTheDocument();

      fireEvent.keyDown(document, { key: "Escape" });
      expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    });

    it("should navigate options with arrow keys", () => {
      render(<DashboardHeader {...multiKeyProps} />);

      fireEvent.click(screen.getByTitle("Switch key"));

      const listbox = screen.getByRole("listbox");
      const options = screen.getAllByRole("option");

      // Initially first option should be focused (selected one)
      expect(document.activeElement).toBe(options[0]);

      // Arrow down to second option
      fireEvent.keyDown(listbox, { key: "ArrowDown" });
      expect(document.activeElement).toBe(options[1]);

      // Arrow up back to first option
      fireEvent.keyDown(listbox, { key: "ArrowUp" });
      expect(document.activeElement).toBe(options[0]);

      // Arrow up should wrap to last option
      fireEvent.keyDown(listbox, { key: "ArrowUp" });
      expect(document.activeElement).toBe(options[1]);
    });

    it("should select option with Enter key", () => {
      const onSelectKey = vi.fn();
      render(
        <DashboardHeader {...multiKeyProps} onSelectKey={onSelectKey} />,
      );

      fireEvent.click(screen.getByTitle("Switch key"));
      const listbox = screen.getByRole("listbox");

      fireEvent.keyDown(listbox, { key: "ArrowDown" }); // Move to second option
      fireEvent.keyDown(listbox, { key: "Enter" });

      expect(onSelectKey).toHaveBeenCalledOnce();
      expect(onSelectKey).toHaveBeenCalledWith("xyz987uvw654rst321");
    });
  });
});
