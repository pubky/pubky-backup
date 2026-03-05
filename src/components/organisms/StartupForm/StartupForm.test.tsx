import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createElement, type ReactNode } from "react";
import { StartupForm } from "./StartupForm";
import { useUIStore } from "@/stores";
import * as services from "@/services";

vi.mock("@/services");

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
      },
    },
  });
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(
      QueryClientProvider,
      { client: queryClient },
      children,
    );
  };
}

describe("StartupForm", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Reset store state
    useUIStore.setState({
      currentScreen: "startup",
      statusMessageMode: "sync",
      pubkyInputValue: "",
      hasAutoLoaded: false,
      toast: { visible: false, pubkyText: "", type: "success", message: "" },
    });
    // Default mocks
    vi.mocked(services.getKeys).mockResolvedValue([]);
    vi.mocked(services.getLastPubky).mockResolvedValue(null);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("should render the startup form", () => {
    render(<StartupForm />, { wrapper: createWrapper() });

    expect(
      screen.getByText("Securely mirror your Pubky data."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /backup/i })).toBeInTheDocument();
  });

  it("should disable backup button when input is empty", () => {
    render(<StartupForm />, { wrapper: createWrapper() });

    const button = screen.getByRole("button", { name: /backup/i });
    expect(button).toBeDisabled();
  });

  it("should enable backup button when input has value", () => {
    useUIStore.setState({ pubkyInputValue: "pk:test123" });
    render(<StartupForm />, { wrapper: createWrapper() });

    const button = screen.getByRole("button", { name: /backup/i });
    expect(button).not.toBeDisabled();
  });

  it("should call addKey on button click", async () => {
    vi.mocked(services.addKey).mockResolvedValue(undefined);

    useUIStore.setState({ pubkyInputValue: "pk:test123" });
    render(<StartupForm />, { wrapper: createWrapper() });

    const button = screen.getByRole("button", { name: /backup/i });
    fireEvent.click(button);

    await waitFor(() => {
      expect(services.addKey).toHaveBeenCalledWith("pk:test123");
    });
  });

  it("should navigate to dashboard on successful add key", async () => {
    vi.mocked(services.addKey).mockResolvedValue(undefined);

    useUIStore.setState({ pubkyInputValue: "pk:test123" });
    render(<StartupForm />, { wrapper: createWrapper() });

    const button = screen.getByRole("button", { name: /backup/i });
    fireEvent.click(button);

    await waitFor(() => {
      expect(useUIStore.getState().currentScreen).toBe("dashboard");
    });
  });

  it("should show error toast on add key failure", async () => {
    const mockError = { type: "InvalidPubkyFormat", message: "Bad format" };
    vi.mocked(services.addKey).mockRejectedValue(mockError);

    useUIStore.setState({ pubkyInputValue: "bad-pubky" });
    render(<StartupForm />, { wrapper: createWrapper() });

    const button = screen.getByRole("button", { name: /backup/i });
    fireEvent.click(button);

    await waitFor(() => {
      const toast = useUIStore.getState().toast;
      expect(toast.visible).toBe(true);
      expect(toast.type).toBe("error");
    });

    // Should stay on startup screen
    expect(useUIStore.getState().currentScreen).toBe("startup");
  });

  it("should show spinner when loading", async () => {
    let resolveAddKey: () => void;
    const addKeyPromise = new Promise<void>((resolve) => {
      resolveAddKey = resolve;
    });
    vi.mocked(services.addKey).mockReturnValue(addKeyPromise);

    useUIStore.setState({ pubkyInputValue: "pk:test123" });
    const { container } = render(<StartupForm />, { wrapper: createWrapper() });

    const button = screen.getByRole("button", { name: /backup/i });
    fireEvent.click(button);

    await waitFor(() => {
      const spinner = container.querySelector(".animate-spin");
      expect(spinner).toBeInTheDocument();
    });

    resolveAddKey!();
  });

  it("should use default placeholder when no keys", async () => {
    vi.mocked(services.getKeys).mockResolvedValue([]);
    render(<StartupForm />, { wrapper: createWrapper() });

    await waitFor(() => {
      expect(
        screen.getByPlaceholderText("g1b6wp8bhhxt..."),
      ).toBeInTheDocument();
    });
  });

  it("should use different placeholder when keys exist", async () => {
    vi.mocked(services.getKeys).mockResolvedValue([
      "pk:prev1",
      "pk:prev2",
    ]);
    render(<StartupForm />, { wrapper: createWrapper() });

    await waitFor(() => {
      expect(
        screen.getByPlaceholderText("Enter your pubky..."),
      ).toBeInTheDocument();
    });
  });

  it("should trim whitespace from pubky input before adding", async () => {
    vi.mocked(services.addKey).mockResolvedValue(undefined);

    useUIStore.setState({ pubkyInputValue: "  pk:test123  " });
    render(<StartupForm />, { wrapper: createWrapper() });

    const button = screen.getByRole("button", { name: /backup/i });
    fireEvent.click(button);

    await waitFor(() => {
      expect(services.addKey).toHaveBeenCalledWith("pk:test123");
    });
  });

  describe("auto-load behavior", () => {
    it("should auto-load and add key when lastPubky exists", async () => {
      vi.mocked(services.getLastPubky).mockResolvedValue("pk:saved-pubky-123");
      vi.mocked(services.addKey).mockResolvedValue(undefined);

      render(<StartupForm />, { wrapper: createWrapper() });

      await waitFor(() => {
        expect(services.addKey).toHaveBeenCalledWith("pk:saved-pubky-123");
      });

      await waitFor(() => {
        expect(useUIStore.getState().currentScreen).toBe("dashboard");
        expect(useUIStore.getState().pubkyInputValue).toBe(
          "pk:saved-pubky-123",
        );
        expect(useUIStore.getState().hasAutoLoaded).toBe(true);
      });
    });

    it("should not auto-load if hasAutoLoaded is already true", async () => {
      vi.mocked(services.getLastPubky).mockResolvedValue("pk:saved-pubky-123");
      vi.mocked(services.addKey).mockResolvedValue(undefined);

      useUIStore.setState({ hasAutoLoaded: true });
      render(<StartupForm />, { wrapper: createWrapper() });

      // Wait a tick to ensure the effect has run
      await waitFor(() => {
        expect(services.getLastPubky).toHaveBeenCalled();
      });

      // Should not have called addKey because hasAutoLoaded is true
      expect(services.addKey).not.toHaveBeenCalled();
      expect(useUIStore.getState().currentScreen).toBe("startup");
    });

    it("should not auto-load when lastPubky is null", async () => {
      vi.mocked(services.getLastPubky).mockResolvedValue(null);

      render(<StartupForm />, { wrapper: createWrapper() });

      await waitFor(() => {
        expect(services.getLastPubky).toHaveBeenCalled();
      });

      expect(services.addKey).not.toHaveBeenCalled();
      expect(useUIStore.getState().hasAutoLoaded).toBe(false);
    });

    it("should handle auto-load add key failure gracefully", async () => {
      const mockError = { type: "HomeserverNotFound", message: "Not found" };
      vi.mocked(services.getLastPubky).mockResolvedValue("pk:invalid-pubky");
      vi.mocked(services.addKey).mockRejectedValue(mockError);

      render(<StartupForm />, { wrapper: createWrapper() });

      await waitFor(() => {
        expect(services.addKey).toHaveBeenCalledWith("pk:invalid-pubky");
      });

      await waitFor(() => {
        const toast = useUIStore.getState().toast;
        expect(toast.visible).toBe(true);
        expect(toast.type).toBe("error");
      });

      // Should stay on startup screen after error
      expect(useUIStore.getState().currentScreen).toBe("startup");
      // But hasAutoLoaded should be true to prevent retry loop
      expect(useUIStore.getState().hasAutoLoaded).toBe(true);
    });
  });
});
