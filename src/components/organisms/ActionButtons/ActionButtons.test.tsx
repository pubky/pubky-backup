import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ActionButtons } from "./ActionButtons";

describe("ActionButtons", () => {
  const defaultProps = {
    isSyncing: false,
    isCreatingSnapshot: false,
    isForceSyncing: false,
    onSnapshot: vi.fn(),
    onForceSync: vi.fn(),
  };

  it("should render both buttons", () => {
    render(<ActionButtons {...defaultProps} />);

    expect(
      screen.getByRole("button", { name: /create snapshot/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /force sync/i }),
    ).toBeInTheDocument();
  });

  it("should call onSnapshot when snapshot button is clicked", () => {
    const onSnapshot = vi.fn();
    render(<ActionButtons {...defaultProps} onSnapshot={onSnapshot} />);

    fireEvent.click(screen.getByRole("button", { name: /create snapshot/i }));

    expect(onSnapshot).toHaveBeenCalledOnce();
  });

  it("should call onForceSync when force sync button is clicked", () => {
    const onForceSync = vi.fn();
    render(<ActionButtons {...defaultProps} onForceSync={onForceSync} />);

    fireEvent.click(screen.getByRole("button", { name: /force sync/i }));

    expect(onForceSync).toHaveBeenCalledOnce();
  });

  describe("disabled states", () => {
    it("should disable snapshot button when isSyncing is true", () => {
      render(<ActionButtons {...defaultProps} isSyncing={true} />);

      expect(
        screen.getByRole("button", { name: /create snapshot/i }),
      ).toBeDisabled();
    });

    it("should disable snapshot button when isCreatingSnapshot is true", () => {
      render(<ActionButtons {...defaultProps} isCreatingSnapshot={true} />);

      expect(
        screen.getByRole("button", { name: /create snapshot/i }),
      ).toBeDisabled();
    });

    it("should disable force sync button when isSyncing is true", () => {
      render(<ActionButtons {...defaultProps} isSyncing={true} />);

      expect(
        screen.getByRole("button", { name: /force sync/i }),
      ).toBeDisabled();
    });

    it("should disable force sync button when isForceSyncing is true", () => {
      render(<ActionButtons {...defaultProps} isForceSyncing={true} />);

      expect(
        screen.getByRole("button", { name: /force sync/i }),
      ).toBeDisabled();
    });

    it("should enable both buttons when all flags are false", () => {
      render(<ActionButtons {...defaultProps} />);

      expect(
        screen.getByRole("button", { name: /create snapshot/i }),
      ).not.toBeDisabled();
      expect(
        screen.getByRole("button", { name: /force sync/i }),
      ).not.toBeDisabled();
    });
  });

  describe("visual states", () => {
    it("should show spinning icon on force sync button when isForceSyncing is true", () => {
      const { container } = render(
        <ActionButtons {...defaultProps} isForceSyncing={true} />,
      );

      const spinners = container.querySelectorAll(".animate-spin");
      expect(spinners.length).toBe(1);
    });

    it("should show spinning icon on snapshot button when isCreatingSnapshot is true", () => {
      const { container } = render(
        <ActionButtons {...defaultProps} isCreatingSnapshot={true} />,
      );

      const spinners = container.querySelectorAll(".animate-spin");
      expect(spinners.length).toBe(1);
    });

    it("should show two spinning icons when both isForceSyncing and isCreatingSnapshot are true", () => {
      const { container } = render(
        <ActionButtons
          {...defaultProps}
          isForceSyncing={true}
          isCreatingSnapshot={true}
        />,
      );

      const spinners = container.querySelectorAll(".animate-spin");
      expect(spinners.length).toBe(2);
    });

    it("should not show spinning icon when neither isForceSyncing nor isCreatingSnapshot", () => {
      const { container } = render(
        <ActionButtons {...defaultProps} isForceSyncing={false} />,
      );

      const spinner = container.querySelector(".animate-spin");
      expect(spinner).not.toBeInTheDocument();
    });

    it("should apply reduced opacity when isCreatingSnapshot is true", () => {
      render(<ActionButtons {...defaultProps} isCreatingSnapshot={true} />);

      const snapshotButton = screen.getByRole("button", {
        name: /create snapshot/i,
      });
      expect(snapshotButton).toHaveClass("opacity-70");
    });
  });

  it("should not call handlers when buttons are disabled", () => {
    const onSnapshot = vi.fn();
    const onForceSync = vi.fn();

    render(
      <ActionButtons
        {...defaultProps}
        isSyncing={true}
        onSnapshot={onSnapshot}
        onForceSync={onForceSync}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /create snapshot/i }));
    fireEvent.click(screen.getByRole("button", { name: /force sync/i }));

    expect(onSnapshot).not.toHaveBeenCalled();
    expect(onForceSync).not.toHaveBeenCalled();
  });
});
