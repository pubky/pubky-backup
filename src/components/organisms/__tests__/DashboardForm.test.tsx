import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createElement, type ReactNode } from "react";
import { DashboardForm } from "../DashboardForm";
import { useUIStore } from "@/stores/useUIStore";
import * as tauriCommands from "@/services/tauri-commands";
import type { AppState } from "@/types/app-state";

vi.mock("@/services/tauri-commands");

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
        refetchInterval: false,
      },
    },
  });
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

const mockAppState: AppState = {
  pubky: "pk:testpubky12345678901234567890",
  homeserver: "https://example.com",
  developer_mode: false,
  is_syncing: false,
  next_sync_time: Math.floor(Date.now() / 1000) + 60,
  data_dir_size: 1024000,
  backup_controller_error: null,
};

describe("DashboardForm", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Reset store state
    useUIStore.setState({
      currentScreen: "dashboard",
      statusMessageMode: "sync",
      pubkyInputValue: "",
      toast: { visible: false, pubkyText: "" },
    });
    // Mock window.alert
    window.alert = vi.fn();
    // Default mocks
    vi.mocked(tauriCommands.fetchState).mockResolvedValue(mockAppState);
    vi.mocked(tauriCommands.getDataDirPath).mockResolvedValue(
      "/home/user/.local/share/pubky-backup",
    );
    vi.mocked(tauriCommands.backupControllerClose).mockResolvedValue(undefined);
    vi.mocked(tauriCommands.forceSyncNow).mockResolvedValue(undefined);
    vi.mocked(tauriCommands.createSnapshot).mockResolvedValue(
      "/path/to/snapshot.zip",
    );
    vi.mocked(tauriCommands.openDataDir).mockResolvedValue(undefined);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("should render the dashboard with pubky info", async () => {
    render(<DashboardForm />, { wrapper: createWrapper() });

    await waitFor(() => {
      // Truncated pubky display
      expect(screen.getByText("pk:te...67890")).toBeInTheDocument();
    });
  });

  it("should display backup size", async () => {
    render(<DashboardForm />, { wrapper: createWrapper() });

    await waitFor(() => {
      expect(screen.getByText("1000.0 KB")).toBeInTheDocument();
    });
  });

  it("should display backup location", async () => {
    render(<DashboardForm />, { wrapper: createWrapper() });

    await waitFor(() => {
      expect(
        screen.getByText("/home/user/.local/share/pubky-backup"),
      ).toBeInTheDocument();
    });
  });

  it("should show SYNCED badge when not syncing", async () => {
    render(<DashboardForm />, { wrapper: createWrapper() });

    await waitFor(() => {
      expect(screen.getByText("SYNCED")).toBeInTheDocument();
    });
  });

  it("should show SYNCING badge when syncing", async () => {
    vi.mocked(tauriCommands.fetchState).mockResolvedValue({
      ...mockAppState,
      is_syncing: true,
    });

    render(<DashboardForm />, { wrapper: createWrapper() });

    await waitFor(() => {
      expect(screen.getByText("SYNCING")).toBeInTheDocument();
    });
  });

  it("should render copy button", async () => {
    render(<DashboardForm />, { wrapper: createWrapper() });

    await waitFor(() => {
      expect(screen.getByTitle("Copy full pubky")).toBeInTheDocument();
    });
  });

  it("should navigate back when back button is clicked", async () => {
    render(<DashboardForm />, { wrapper: createWrapper() });

    await waitFor(() => {
      expect(screen.getByTitle("Back")).toBeInTheDocument();
    });

    const backButton = screen.getByTitle("Back");
    fireEvent.click(backButton);

    await waitFor(() => {
      expect(tauriCommands.backupControllerClose).toHaveBeenCalled();
      expect(useUIStore.getState().currentScreen).toBe("startup");
    });
  });

  it("should call force sync when force sync button is clicked", async () => {
    render(<DashboardForm />, { wrapper: createWrapper() });

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /force sync/i }),
      ).toBeInTheDocument();
    });

    const forceSyncButton = screen.getByRole("button", { name: /force sync/i });
    fireEvent.click(forceSyncButton);

    await waitFor(() => {
      expect(tauriCommands.forceSyncNow).toHaveBeenCalled();
    });
  });

  it("should create snapshot when snapshot button is clicked", async () => {
    render(<DashboardForm />, { wrapper: createWrapper() });

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /create snapshot/i }),
      ).toBeInTheDocument();
    });

    const snapshotButton = screen.getByRole("button", {
      name: /create snapshot/i,
    });
    fireEvent.click(snapshotButton);

    await waitFor(() => {
      expect(tauriCommands.createSnapshot).toHaveBeenCalled();
    });
  });

  it("should show snapshot success message after creation", async () => {
    render(<DashboardForm />, { wrapper: createWrapper() });

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /create snapshot/i }),
      ).toBeInTheDocument();
    });

    const snapshotButton = screen.getByRole("button", {
      name: /create snapshot/i,
    });
    fireEvent.click(snapshotButton);

    await waitFor(() => {
      expect(screen.getByText("Snapshot created")).toBeInTheDocument();
    });
  });

  it("should show snapshot error message on failure", async () => {
    vi.mocked(tauriCommands.createSnapshot).mockRejectedValue(
      new Error("Disk full"),
    );

    render(<DashboardForm />, { wrapper: createWrapper() });

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /create snapshot/i }),
      ).toBeInTheDocument();
    });

    const snapshotButton = screen.getByRole("button", {
      name: /create snapshot/i,
    });
    fireEvent.click(snapshotButton);

    await waitFor(() => {
      expect(screen.getByText("Failed to create snapshot")).toBeInTheDocument();
    });
  });

  it("should disable action buttons when syncing", async () => {
    vi.mocked(tauriCommands.fetchState).mockResolvedValue({
      ...mockAppState,
      is_syncing: true,
    });

    render(<DashboardForm />, { wrapper: createWrapper() });

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /create snapshot/i }),
      ).toBeDisabled();
      expect(
        screen.getByRole("button", { name: /force sync/i }),
      ).toBeDisabled();
    });
  });

  it("should handle backup controller error by showing alert and navigating back", async () => {
    vi.mocked(tauriCommands.fetchState).mockResolvedValue({
      ...mockAppState,
      backup_controller_error: "Connection lost",
    });

    render(<DashboardForm />, { wrapper: createWrapper() });

    await waitFor(() => {
      expect(window.alert).toHaveBeenCalled();
      expect(useUIStore.getState().currentScreen).toBe("startup");
    });
  });
});
