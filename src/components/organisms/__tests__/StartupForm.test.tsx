import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createElement, type ReactNode } from "react";
import { StartupForm } from "../StartupForm";
import { useUIStore } from "@/stores/useUIStore";
import * as tauriCommands from "@/services/tauri-commands";

vi.mock("@/services/tauri-commands");

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
      },
    },
  });
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
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
      toast: { visible: false, pubkyText: "" },
    });
    // Mock window.alert
    window.alert = vi.fn();
    // Default mocks
    vi.mocked(tauriCommands.getPreviousPubkyKeys).mockResolvedValue([]);
    vi.mocked(tauriCommands.getLastPubky).mockResolvedValue(null);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("should render the startup form", () => {
    render(<StartupForm />, { wrapper: createWrapper() });

    expect(
      screen.getByText("Securely mirror your Pubky data."),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /backup/i }),
    ).toBeInTheDocument();
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
    vi.mocked(tauriCommands.initAppState).mockResolvedValue(undefined);
    vi.mocked(tauriCommands.backupControllerBegin).mockResolvedValue(undefined);

    useUIStore.setState({ pubkyInputValue: "pk:test123" });
    render(<StartupForm />, { wrapper: createWrapper() });

    const button = screen.getByRole("button", { name: /backup/i });
    fireEvent.click(button);

    await waitFor(() => {
      expect(tauriCommands.initAppState).toHaveBeenCalledWith("pk:test123");
    });
  });

  it("should navigate to dashboard on successful initialization", async () => {
    vi.mocked(tauriCommands.initAppState).mockResolvedValue(undefined);
    vi.mocked(tauriCommands.backupControllerBegin).mockResolvedValue(undefined);

    useUIStore.setState({ pubkyInputValue: "pk:test123" });
    render(<StartupForm />, { wrapper: createWrapper() });

    const button = screen.getByRole("button", { name: /backup/i });
    fireEvent.click(button);

    await waitFor(() => {
      expect(useUIStore.getState().currentScreen).toBe("dashboard");
    });
  });

  it("should show error alert on initialization failure", async () => {
    const mockError = { type: "InvalidPubkyFormat", message: "Bad format" };
    vi.mocked(tauriCommands.initAppState).mockRejectedValue(mockError);

    useUIStore.setState({ pubkyInputValue: "bad-pubky" });
    render(<StartupForm />, { wrapper: createWrapper() });

    const button = screen.getByRole("button", { name: /backup/i });
    fireEvent.click(button);

    await waitFor(() => {
      expect(window.alert).toHaveBeenCalled();
    });

    // Should stay on startup screen
    expect(useUIStore.getState().currentScreen).toBe("startup");
  });

  it("should show spinner when loading", async () => {
    let resolveInit: () => void;
    const initPromise = new Promise<void>((resolve) => {
      resolveInit = resolve;
    });
    vi.mocked(tauriCommands.initAppState).mockReturnValue(initPromise);
    vi.mocked(tauriCommands.backupControllerBegin).mockResolvedValue(undefined);

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
    vi.mocked(tauriCommands.getPreviousPubkyKeys).mockResolvedValue([]);
    render(<StartupForm />, { wrapper: createWrapper() });

    await waitFor(() => {
      expect(screen.getByPlaceholderText("g1b6wp8bhhxt...")).toBeInTheDocument();
    });
  });

  it("should use different placeholder when previous keys exist", async () => {
    vi.mocked(tauriCommands.getPreviousPubkyKeys).mockResolvedValue([
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
    vi.mocked(tauriCommands.initAppState).mockResolvedValue(undefined);
    vi.mocked(tauriCommands.backupControllerBegin).mockResolvedValue(undefined);

    useUIStore.setState({ pubkyInputValue: "  pk:test123  " });
    render(<StartupForm />, { wrapper: createWrapper() });

    const button = screen.getByRole("button", { name: /backup/i });
    fireEvent.click(button);

    await waitFor(() => {
      expect(tauriCommands.initAppState).toHaveBeenCalledWith("pk:test123");
    });
  });
});
