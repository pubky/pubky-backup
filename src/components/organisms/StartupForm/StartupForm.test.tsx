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
    vi.mocked(services.getPreviousPubkyKeys).mockResolvedValue([]);
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

  it("should call initialize on button click", async () => {
    vi.mocked(services.initAppState).mockResolvedValue(undefined);
    vi.mocked(services.backupControllerBegin).mockResolvedValue(undefined);

    useUIStore.setState({ pubkyInputValue: "pk:test123" });
    render(<StartupForm />, { wrapper: createWrapper() });

    const button = screen.getByRole("button", { name: /backup/i });
    fireEvent.click(button);

    await waitFor(() => {
      expect(services.initAppState).toHaveBeenCalledWith("pk:test123");
    });
  });

  it("should navigate to dashboard on successful initialization", async () => {
    vi.mocked(services.initAppState).mockResolvedValue(undefined);
    vi.mocked(services.backupControllerBegin).mockResolvedValue(undefined);

    useUIStore.setState({ pubkyInputValue: "pk:test123" });
    render(<StartupForm />, { wrapper: createWrapper() });

    const button = screen.getByRole("button", { name: /backup/i });
    fireEvent.click(button);

    await waitFor(() => {
      expect(useUIStore.getState().currentScreen).toBe("dashboard");
    });
  });

  it("should show error toast on initialization failure", async () => {
    const mockError = { type: "InvalidPubkyFormat", message: "Bad format" };
    vi.mocked(services.initAppState).mockRejectedValue(mockError);

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
    let resolveInit: () => void;
    const initPromise = new Promise<void>((resolve) => {
      resolveInit = resolve;
    });
    vi.mocked(services.initAppState).mockReturnValue(initPromise);
    vi.mocked(services.backupControllerBegin).mockResolvedValue(undefined);

    useUIStore.setState({ pubkyInputValue: "pk:test123" });
    const { container } = render(<StartupForm />, { wrapper: createWrapper() });

    const button = screen.getByRole("button", { name: /backup/i });
    fireEvent.click(button);

    await waitFor(() => {
      const spinner = container.querySelector(".animate-spin");
      expect(spinner).toBeInTheDocument();
    });

    resolveInit!();
  });

  it("should use default placeholder when no previous keys", async () => {
    vi.mocked(services.getPreviousPubkyKeys).mockResolvedValue([]);
    render(<StartupForm />, { wrapper: createWrapper() });

    await waitFor(() => {
      expect(
        screen.getByPlaceholderText("g1b6wp8bhhxt..."),
      ).toBeInTheDocument();
    });
  });

  it("should use different placeholder when previous keys exist", async () => {
    vi.mocked(services.getPreviousPubkyKeys).mockResolvedValue([
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

  it("should trim whitespace from pubky input before initializing", async () => {
    vi.mocked(services.initAppState).mockResolvedValue(undefined);
    vi.mocked(services.backupControllerBegin).mockResolvedValue(undefined);

    useUIStore.setState({ pubkyInputValue: "  pk:test123  " });
    render(<StartupForm />, { wrapper: createWrapper() });

    const button = screen.getByRole("button", { name: /backup/i });
    fireEvent.click(button);

    await waitFor(() => {
      expect(services.initAppState).toHaveBeenCalledWith("pk:test123");
    });
  });

  describe("auto-load behavior", () => {
    it("should auto-load and initialize when lastPubky exists", async () => {
      vi.mocked(services.getLastPubky).mockResolvedValue("pk:saved-pubky-123");
      vi.mocked(services.initAppState).mockResolvedValue(undefined);
      vi.mocked(services.backupControllerBegin).mockResolvedValue(undefined);

      render(<StartupForm />, { wrapper: createWrapper() });

      await waitFor(() => {
        expect(services.initAppState).toHaveBeenCalledWith(
          "pk:saved-pubky-123",
        );
        expect(services.backupControllerBegin).toHaveBeenCalled();
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
      vi.mocked(services.initAppState).mockResolvedValue(undefined);
      vi.mocked(services.backupControllerBegin).mockResolvedValue(undefined);

      useUIStore.setState({ hasAutoLoaded: true });
      render(<StartupForm />, { wrapper: createWrapper() });

      // Wait a tick to ensure the effect has run
      await waitFor(() => {
        expect(services.getLastPubky).toHaveBeenCalled();
      });

      // Should not have called initAppState because hasAutoLoaded is true
      expect(services.initAppState).not.toHaveBeenCalled();
      expect(useUIStore.getState().currentScreen).toBe("startup");
    });

    it("should not auto-load when lastPubky is null", async () => {
      vi.mocked(services.getLastPubky).mockResolvedValue(null);

      render(<StartupForm />, { wrapper: createWrapper() });

      await waitFor(() => {
        expect(services.getLastPubky).toHaveBeenCalled();
      });

      expect(services.initAppState).not.toHaveBeenCalled();
      expect(useUIStore.getState().hasAutoLoaded).toBe(false);
    });

    it("should handle auto-load initialization failure gracefully", async () => {
      const mockError = { type: "HomeserverNotFound", message: "Not found" };
      vi.mocked(services.getLastPubky).mockResolvedValue("pk:invalid-pubky");
      vi.mocked(services.initAppState).mockRejectedValue(mockError);

      render(<StartupForm />, { wrapper: createWrapper() });

      await waitFor(() => {
        expect(services.initAppState).toHaveBeenCalledWith("pk:invalid-pubky");
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
