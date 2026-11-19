import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { StartupForm } from "./StartupForm";
import { useUIStore } from "@/stores/uiStore";
import * as services from "@/services";

// Mock services
vi.mock("@/services", () => ({
  addKey: vi.fn(),
  getLastPubky: vi.fn(),
  signInNewKey: vi.fn(),
  cancelSignInNewKey: vi.fn(),
}));

describe("StartupForm", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Mock getLastPubky to return null by default (no auto-load)
    vi.mocked(services.getLastPubky).mockResolvedValue(null);
    vi.mocked(services.addKey).mockResolvedValue("normalized-pubky");

    useUIStore.setState({
      currentScreen: "startup",
      hasAutoLoaded: false,
      keyStates: {},
      lastPubky: null,
    });
  });

  describe("rendering", () => {
    it("should render logo and tagline", async () => {
      render(<StartupForm />);

      // Wait for async hooks to settle
      await waitFor(() => {
        expect(
          screen.getByText(/securely mirror your pubky data/i),
        ).toBeInTheDocument();
      });
    });

    it("should render pubky input field", async () => {
      render(<StartupForm />);

      await waitFor(() => {
        expect(
          screen.getByPlaceholderText(/enter pubky to add/i),
        ).toBeInTheDocument();
      });
    });

    it("should render the public-only backup button", async () => {
      render(<StartupForm />);

      await waitFor(() => {
        expect(
          screen.getByRole("button", { name: /back up public data/i }),
        ).toBeInTheDocument();
      });
    });

    it("should show placeholder when no existing keys", async () => {
      render(<StartupForm />);

      await waitFor(() => {
        expect(
          screen.getByPlaceholderText(/enter pubky to add/i),
        ).toBeInTheDocument();
      });
    });
  });

  describe("input handling", () => {
    it("should disable button when input is empty", async () => {
      render(<StartupForm />);

      await waitFor(() => {
        expect(
          screen.getByRole("button", { name: /back up public data/i }),
        ).toBeDisabled();
      });
    });

    it("should enable button when input has value", async () => {
      render(<StartupForm />);
      fireEvent.change(
        await screen.findByPlaceholderText(/enter pubky to add/i),
        {
          target: { value: "some-pubky" },
        },
      );

      await waitFor(() => {
        expect(
          screen.getByRole("button", { name: /back up public data/i }),
        ).not.toBeDisabled();
      });
    });
  });

  describe("form submission", () => {
    it("should call addKey when the public-only backup button is clicked", async () => {
      render(<StartupForm />);
      fireEvent.change(
        await screen.findByPlaceholderText(/enter pubky to add/i),
        {
          target: { value: "my-pubky" },
        },
      );

      await waitFor(() => {
        expect(
          screen.getByRole("button", { name: /back up public data/i }),
        ).not.toBeDisabled();
      });

      fireEvent.click(
        screen.getByRole("button", { name: /back up public data/i }),
      );

      await waitFor(() => {
        expect(services.addKey).toHaveBeenCalledWith("my-pubky");
      });
    });

    it("should navigate to the dashboard's sync page on successful add", async () => {
      // The dashboard may have been left on another page by a removed key
      useUIStore.setState({ currentPage: "keys" });
      render(<StartupForm />);
      fireEvent.change(
        await screen.findByPlaceholderText(/enter pubky to add/i),
        {
          target: { value: "my-pubky" },
        },
      );

      await waitFor(() => {
        expect(
          screen.getByRole("button", { name: /back up public data/i }),
        ).not.toBeDisabled();
      });

      fireEvent.click(
        screen.getByRole("button", { name: /back up public data/i }),
      );

      await waitFor(() => {
        expect(useUIStore.getState().currentScreen).toBe("dashboard");
      });
      expect(useUIStore.getState().currentPage).toBe("sync");
    });

    it("should trim whitespace from input before submitting", async () => {
      render(<StartupForm />);
      fireEvent.change(
        await screen.findByPlaceholderText(/enter pubky to add/i),
        {
          target: { value: "  pubky-with-spaces  " },
        },
      );

      await waitFor(() => {
        expect(
          screen.getByRole("button", { name: /back up public data/i }),
        ).not.toBeDisabled();
      });

      fireEvent.click(
        screen.getByRole("button", { name: /back up public data/i }),
      );

      await waitFor(() => {
        expect(services.addKey).toHaveBeenCalledWith("pubky-with-spaces");
      });
    });

    it("should not submit when input is empty", async () => {
      render(<StartupForm />);

      await waitFor(() => {
        expect(
          screen.getByRole("button", { name: /back up public data/i }),
        ).toBeDisabled();
      });

      // Button is disabled, so clicking shouldn't do anything
      fireEvent.click(
        screen.getByRole("button", { name: /back up public data/i }),
      );

      expect(services.addKey).not.toHaveBeenCalled();
    });

    it("should not submit when only whitespace", async () => {
      render(<StartupForm />);
      fireEvent.change(
        await screen.findByPlaceholderText(/enter pubky to add/i),
        {
          target: { value: "   " },
        },
      );

      await waitFor(() => {
        expect(
          screen.getByRole("button", { name: /back up public data/i }),
        ).toBeDisabled();
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
        expect(
          screen.getByPlaceholderText(/enter pubky to add/i),
        ).toBeInTheDocument();
      });

      // Should not have called addKey
      expect(services.addKey).not.toHaveBeenCalled();
    });

    it("should show that the last pubky is being opened instead of the form", async () => {
      vi.mocked(services.getLastPubky).mockResolvedValue("last-used-pubky");
      vi.mocked(services.addKey).mockReturnValue(new Promise(() => {}));

      render(<StartupForm />);

      await waitFor(() => {
        expect(screen.getByText(/opening your backup/i)).toBeInTheDocument();
      });
      expect(
        screen.queryByRole("button", { name: /sign in with pubky ring/i }),
      ).not.toBeInTheDocument();
    });

    it("should open the dashboard once the last pubky is resumed", async () => {
      vi.mocked(services.getLastPubky).mockResolvedValue("last-used-pubky");

      render(<StartupForm />);

      await waitFor(() => {
        expect(useUIStore.getState().currentScreen).toBe("dashboard");
      });
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
      render(<StartupForm />);
      fireEvent.change(
        await screen.findByPlaceholderText(/enter pubky to add/i),
        {
          target: { value: "bad-pubky" },
        },
      );

      await waitFor(() => {
        expect(
          screen.getByRole("button", { name: /back up public data/i }),
        ).not.toBeDisabled();
      });

      fireEvent.click(
        screen.getByRole("button", { name: /back up public data/i }),
      );

      await waitFor(() => {
        expect(services.addKey).toHaveBeenCalled();
      });

      // Should remain on startup screen
      expect(useUIStore.getState().currentScreen).toBe("startup");
    });

    it("should not navigate on error", async () => {
      vi.mocked(services.addKey).mockRejectedValue(new Error("Failed"));
      render(<StartupForm />);
      fireEvent.change(
        await screen.findByPlaceholderText(/enter pubky to add/i),
        {
          target: { value: "bad-pubky" },
        },
      );

      await waitFor(() => {
        expect(
          screen.getByRole("button", { name: /back up public data/i }),
        ).not.toBeDisabled();
      });

      fireEvent.click(
        screen.getByRole("button", { name: /back up public data/i }),
      );

      await waitFor(() => {
        expect(services.addKey).toHaveBeenCalled();
      });

      // Should still be on startup screen
      expect(useUIStore.getState().currentScreen).toBe("startup");
    });
  });

  describe("signing in", () => {
    it("should open the dashboard once a sign-in adds a pubky", async () => {
      vi.mocked(services.signInNewKey).mockResolvedValue("signed-in-pubky");

      render(<StartupForm />);
      fireEvent.click(
        await screen.findByRole("button", { name: /sign in with pubky ring/i }),
      );

      await waitFor(() => {
        expect(useUIStore.getState().currentScreen).toBe("dashboard");
      });
      expect(useUIStore.getState().lastPubky).toBe("signed-in-pubky");
    });
  });
});
