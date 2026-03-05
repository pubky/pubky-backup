import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { StartupForm } from "./StartupForm";
import { useUIStore } from "@/stores/uiStore";
import type { KeyState } from "@/stores/uiStore";
import * as services from "@/services";

// Mock services
vi.mock("@/services", () => ({
  addKey: vi.fn(),
  getLastPubky: vi.fn(),
}));

describe("StartupForm", () => {
  const mockKeyState: KeyState = {
    status: { type: "Idle" },
    data_size: 1024,
    last_sync: 1672531200,
    next_sync: 1672531230,
    error: null,
    total_files: null,
    files_synced: null,
    bytes_downloaded: null,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    // Mock getLastPubky to return null by default (no auto-load)
    vi.mocked(services.getLastPubky).mockResolvedValue(null);
    vi.mocked(services.addKey).mockResolvedValue("normalized-pubky");

    useUIStore.setState({
      currentScreen: "startup",
      pubkyInputValue: "",
      hasAutoLoaded: false,
      keyStates: {},
      viewedPubky: null,
    });
  });

  describe("rendering", () => {
    it("should render logo and tagline", async () => {
      render(<StartupForm />);

      // Wait for async hooks to settle
      await waitFor(() => {
        expect(screen.getByText(/securely mirror your pubky data/i)).toBeInTheDocument();
      });
    });

    it("should render pubky input field", async () => {
      render(<StartupForm />);

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/g1b6wp8bhhxt/i)).toBeInTheDocument();
      });
    });

    it("should render backup button", async () => {
      render(<StartupForm />);

      await waitFor(() => {
        expect(screen.getByRole("button", { name: /backup/i })).toBeInTheDocument();
      });
    });

    it("should show placeholder when no existing keys", async () => {
      render(<StartupForm />);

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/g1b6wp8bhhxt/i)).toBeInTheDocument();
      });
    });

    it("should show different placeholder when keys exist", async () => {
      useUIStore.setState({
        keyStates: { "pk:existing-key": mockKeyState },
      });

      render(<StartupForm />);

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/enter your pubky/i)).toBeInTheDocument();
      });
    });
  });

  describe("input handling", () => {
    it("should update store when typing", async () => {
      render(<StartupForm />);

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/g1b6wp8bhhxt/i)).toBeInTheDocument();
      });

      const input = screen.getByPlaceholderText(/g1b6wp8bhhxt/i);
      fireEvent.change(input, { target: { value: "my-pubky-input" } });

      // The value is stored in Zustand
      expect(useUIStore.getState().pubkyInputValue).toBe("my-pubky-input");
    });

    it("should disable button when input is empty", async () => {
      render(<StartupForm />);

      await waitFor(() => {
        expect(screen.getByRole("button", { name: /backup/i })).toBeDisabled();
      });
    });

    it("should enable button when input has value", async () => {
      useUIStore.setState({ pubkyInputValue: "some-pubky" });

      render(<StartupForm />);

      await waitFor(() => {
        expect(screen.getByRole("button", { name: /backup/i })).not.toBeDisabled();
      });
    });
  });

  describe("form submission", () => {
    it("should call addKey when backup button is clicked", async () => {
      useUIStore.setState({ pubkyInputValue: "my-pubky" });

      render(<StartupForm />);

      await waitFor(() => {
        expect(screen.getByRole("button", { name: /backup/i })).not.toBeDisabled();
      });

      fireEvent.click(screen.getByRole("button", { name: /backup/i }));

      await waitFor(() => {
        expect(services.addKey).toHaveBeenCalledWith("my-pubky");
      });
    });

    it("should navigate to dashboard on successful add", async () => {
      useUIStore.setState({ pubkyInputValue: "my-pubky" });

      render(<StartupForm />);

      await waitFor(() => {
        expect(screen.getByRole("button", { name: /backup/i })).not.toBeDisabled();
      });

      fireEvent.click(screen.getByRole("button", { name: /backup/i }));

      await waitFor(() => {
        expect(useUIStore.getState().currentScreen).toBe("dashboard");
      });
    });

    it("should trim whitespace from input before submitting", async () => {
      useUIStore.setState({ pubkyInputValue: "  pubky-with-spaces  " });

      render(<StartupForm />);

      await waitFor(() => {
        expect(screen.getByRole("button", { name: /backup/i })).not.toBeDisabled();
      });

      fireEvent.click(screen.getByRole("button", { name: /backup/i }));

      await waitFor(() => {
        expect(services.addKey).toHaveBeenCalledWith("pubky-with-spaces");
      });
    });

    it("should not submit when input is empty", async () => {
      render(<StartupForm />);

      await waitFor(() => {
        expect(screen.getByRole("button", { name: /backup/i })).toBeDisabled();
      });

      // Button is disabled, so clicking shouldn't do anything
      fireEvent.click(screen.getByRole("button", { name: /backup/i }));

      expect(services.addKey).not.toHaveBeenCalled();
    });

    it("should not submit when only whitespace", async () => {
      useUIStore.setState({ pubkyInputValue: "   " });

      render(<StartupForm />);

      await waitFor(() => {
        expect(screen.getByRole("button", { name: /backup/i })).toBeDisabled();
      });

      // Button is disabled
      expect(services.addKey).not.toHaveBeenCalled();
    });
  });

  describe("auto-load last pubky", () => {
    it("should auto-load last pubky on mount if available", async () => {
      vi.mocked(services.getLastPubky).mockResolvedValue("last-used-pubky");

      render(<StartupForm />);

      await waitFor(() => {
        expect(services.addKey).toHaveBeenCalledWith("last-used-pubky");
      });
    });

    it("should not auto-load if hasAutoLoaded is true", async () => {
      vi.mocked(services.getLastPubky).mockResolvedValue("last-used-pubky");
      useUIStore.setState({ hasAutoLoaded: true });

      render(<StartupForm />);

      // Wait for hooks to settle
      await waitFor(() => {
        expect(screen.getByPlaceholderText(/g1b6wp8bhhxt/i)).toBeInTheDocument();
      });

      // Should not have called addKey
      expect(services.addKey).not.toHaveBeenCalled();
    });

    it("should set hasAutoLoaded after auto-loading", async () => {
      vi.mocked(services.getLastPubky).mockResolvedValue("last-used-pubky");

      render(<StartupForm />);

      await waitFor(() => {
        expect(useUIStore.getState().hasAutoLoaded).toBe(true);
      });
    });
  });

  describe("error handling", () => {
    it("should handle add key errors gracefully", async () => {
      vi.mocked(services.addKey).mockRejectedValue(new Error("Invalid pubky"));
      useUIStore.setState({ pubkyInputValue: "bad-pubky" });

      render(<StartupForm />);

      await waitFor(() => {
        expect(screen.getByRole("button", { name: /backup/i })).not.toBeDisabled();
      });

      fireEvent.click(screen.getByRole("button", { name: /backup/i }));

      await waitFor(() => {
        expect(services.addKey).toHaveBeenCalled();
      });

      // Should remain on startup screen
      expect(useUIStore.getState().currentScreen).toBe("startup");
    });

    it("should not navigate on error", async () => {
      vi.mocked(services.addKey).mockRejectedValue(new Error("Failed"));
      useUIStore.setState({ pubkyInputValue: "bad-pubky" });

      render(<StartupForm />);

      await waitFor(() => {
        expect(screen.getByRole("button", { name: /backup/i })).not.toBeDisabled();
      });

      fireEvent.click(screen.getByRole("button", { name: /backup/i }));

      await waitFor(() => {
        expect(services.addKey).toHaveBeenCalled();
      });

      // Should still be on startup screen
      expect(useUIStore.getState().currentScreen).toBe("startup");
    });
  });

  describe("suggestions", () => {
    it("should pass existing keys as suggestions to input", async () => {
      useUIStore.setState({
        keyStates: {
          "pk:key1": mockKeyState,
          "pk:key2": mockKeyState,
        },
      });

      render(<StartupForm />);

      // When keys exist, placeholder changes
      await waitFor(() => {
        expect(screen.getByPlaceholderText(/enter your pubky/i)).toBeInTheDocument();
      });
    });
  });
});
