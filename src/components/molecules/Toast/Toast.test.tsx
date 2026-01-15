import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";
import { render, screen, act } from "@testing-library/react";
import { Toast } from "./Toast";
import { useUIStore } from "@/stores";

describe("Toast", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    // Reset store state
    useUIStore.setState({
      currentScreen: "startup",
      statusMessageMode: "sync",
      pubkyInputValue: "",
      toast: {
        visible: false,
        pubkyText: "",
      },
    });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("should render with hidden state by default", () => {
    render(<Toast />);

    const toast = screen.getByText("Pubky copied to clipboard").parentElement
      ?.parentElement;
    expect(toast).toHaveClass("opacity-0");
    expect(toast).toHaveClass("pointer-events-none");
  });

  describe("visibility animation classes", () => {
    it("should have translate-y-5 when hidden", () => {
      render(<Toast />);

      const toast = screen.getByText("Pubky copied to clipboard").parentElement
        ?.parentElement;
      expect(toast).toHaveClass("translate-y-5");
      expect(toast).not.toHaveClass("translate-y-0");
    });

    it("should have translate-y-0 when visible", () => {
      useUIStore.getState().showToast("test-pubky");

      render(<Toast />);

      const toast = screen.getByText("Pubky copied to clipboard").parentElement
        ?.parentElement;
      expect(toast).toHaveClass("translate-y-0");
      expect(toast).not.toHaveClass("translate-y-5");
    });

    it("should have transition classes for animation", () => {
      render(<Toast />);

      const toast = screen.getByText("Pubky copied to clipboard").parentElement
        ?.parentElement;
      expect(toast).toHaveClass("transition-all");
      expect(toast).toHaveClass("duration-300");
      expect(toast).toHaveClass("ease-out");
    });

    it("should transition from hidden to visible state", () => {
      const { rerender } = render(<Toast />);

      const toast = screen.getByText("Pubky copied to clipboard").parentElement
        ?.parentElement;

      // Initially hidden
      expect(toast).toHaveClass("opacity-0");
      expect(toast).toHaveClass("translate-y-5");

      // Show toast
      act(() => {
        useUIStore.getState().showToast("test-pubky");
      });
      rerender(<Toast />);

      // Now visible
      expect(toast).toHaveClass("opacity-100");
      expect(toast).toHaveClass("translate-y-0");
    });

    it("should transition from visible to hidden state", () => {
      useUIStore.getState().showToast("test-pubky");
      const { rerender } = render(<Toast />);

      const toast = screen.getByText("Pubky copied to clipboard").parentElement
        ?.parentElement;

      // Initially visible
      expect(toast).toHaveClass("opacity-100");
      expect(toast).toHaveClass("translate-y-0");

      // Hide toast via timeout
      act(() => {
        vi.advanceTimersByTime(2000);
      });
      rerender(<Toast />);

      // Now hidden
      expect(toast).toHaveClass("opacity-0");
      expect(toast).toHaveClass("translate-y-5");
    });
  });

  it("should show toast when visible is true", () => {
    useUIStore.getState().showToast("test-pubky-abc123");

    render(<Toast />);

    const toast = screen.getByText("Pubky copied to clipboard").parentElement
      ?.parentElement;
    expect(toast).toHaveClass("opacity-100");
    expect(toast).not.toHaveClass("pointer-events-none");
  });

  it("should display truncated pubky text", () => {
    const longPubky = "abcdefghijklmnopqrstuvwxyz1234567890";
    useUIStore.getState().showToast(longPubky);

    render(<Toast />);

    // displayPubky truncates to "abcde...67890"
    expect(screen.getByText("abcde...67890")).toBeInTheDocument();
  });

  it("should auto-hide after 2 seconds", () => {
    useUIStore.getState().showToast("test-pubky");

    render(<Toast />);

    expect(useUIStore.getState().toast.visible).toBe(true);

    act(() => {
      vi.advanceTimersByTime(2000);
    });

    expect(useUIStore.getState().toast.visible).toBe(false);
  });

  it("should not auto-hide if already hidden", () => {
    const hideToastSpy = vi.spyOn(useUIStore.getState(), "hideToast");

    render(<Toast />);

    vi.advanceTimersByTime(2000);

    // hideToast should not be called since toast was never visible
    expect(hideToastSpy).not.toHaveBeenCalled();
  });

  it("should clear timeout on unmount", () => {
    useUIStore.getState().showToast("test-pubky");

    const { unmount } = render(<Toast />);

    // Unmount before timeout fires
    unmount();

    // Advance past the timeout - should not cause issues
    vi.advanceTimersByTime(3000);

    // Toast should still be visible since hideToast wasn't called
    expect(useUIStore.getState().toast.visible).toBe(true);
  });

  it("should update displayed pubky when changed", () => {
    useUIStore.getState().showToast("first-pubky-abcdefghij");

    const { rerender } = render(<Toast />);

    expect(screen.getByText("first...fghij")).toBeInTheDocument();

    // Show toast with different pubky
    act(() => {
      useUIStore.getState().showToast("second-pubky-1234567890");
    });
    rerender(<Toast />);

    expect(screen.getByText("secon...67890")).toBeInTheDocument();
  });
});
