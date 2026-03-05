import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  render,
  screen,
  fireEvent,
  waitFor,
  cleanup,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createElement, type ReactNode } from "react";
import { DashboardForm } from "./DashboardForm";
import { useUIStore } from "@/stores";
import * as services from "@/services";
import * as hooks from "@/hooks";
import type { AppState } from "@/types/app-state";

vi.mock("@/services");
// Mock all hooks to prevent memory issues from polling/intervals (useAppState, useCountdown)
vi.mock("@/hooks", () => ({
  useAppState: vi.fn(),
  useCountdown: vi.fn(),
  useDataDirPath: vi.fn(),
  useForceSync: vi.fn(),
  useCreateSnapshot: vi.fn(),
  useLastSyncTime: vi.fn(),
  useOpenDataDir: vi.fn(),
}));

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

const mockAppState: AppState = {
  pubky: "pk:testpubky12345678901234567890",
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
      hasAutoLoaded: false,
      toast: { visible: false, pubkyText: "", type: "success", message: "" },
    });
    // Mock useAppState to avoid 200ms polling that causes memory issues
    vi.mocked(hooks.useAppState).mockReturnValue({
      data: mockAppState,
      isLoading: false,
      isSuccess: true,
      isError: false,
      error: null,
    } as ReturnType<typeof hooks.useAppState>);
    // Mock useCountdown to avoid 1-second interval
    vi.mocked(hooks.useCountdown).mockReturnValue("Next backup in 1 minute...");
    // Mock useLastSyncTime
    vi.mocked(hooks.useLastSyncTime).mockReturnValue(null);
    // Mock useDataDirPath
    vi.mocked(hooks.useDataDirPath).mockReturnValue({
      data: "/home/user/.local/share/pubky-backup",
      isLoading: false,
      isSuccess: true,
      isError: false,
      error: null,
    } as ReturnType<typeof hooks.useDataDirPath>);
    // Mock mutation hooks
    vi.mocked(hooks.useForceSync).mockReturnValue({
      mutateAsync: vi.fn().mockResolvedValue(undefined),
      isPending: false,
    } as unknown as ReturnType<typeof hooks.useForceSync>);
    vi.mocked(hooks.useCreateSnapshot).mockReturnValue({
      mutateAsync: vi.fn().mockResolvedValue("/path/to/snapshot.zip"),
      isPending: false,
    } as unknown as ReturnType<typeof hooks.useCreateSnapshot>);
    vi.mocked(hooks.useOpenDataDir).mockReturnValue({
      mutateAsync: vi.fn().mockResolvedValue(undefined),
      isPending: false,
    } as unknown as ReturnType<typeof hooks.useOpenDataDir>);
    // Default service mocks
    vi.mocked(services.openDataDir).mockResolvedValue(undefined);
  });

  afterEach(() => {
    cleanup();
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
    vi.mocked(hooks.useAppState).mockReturnValue({
      data: { ...mockAppState, is_syncing: true },
      isLoading: false,
      isSuccess: true,
      isError: false,
      error: null,
    } as ReturnType<typeof hooks.useAppState>);

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

  it("should call force sync when force sync button is clicked", async () => {
    const mockMutateAsync = vi.fn().mockResolvedValue(undefined);
    vi.mocked(hooks.useForceSync).mockReturnValue({
      mutateAsync: mockMutateAsync,
      isPending: false,
    } as unknown as ReturnType<typeof hooks.useForceSync>);

    render(<DashboardForm />, { wrapper: createWrapper() });

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /force sync/i }),
      ).toBeInTheDocument();
    });

    const forceSyncButton = screen.getByRole("button", { name: /force sync/i });
    fireEvent.click(forceSyncButton);

    await waitFor(() => {
      expect(mockMutateAsync).toHaveBeenCalled();
    });
  });

  it("should create snapshot when snapshot button is clicked", async () => {
    const mockMutateAsync = vi.fn().mockResolvedValue("/path/to/snapshot.zip");
    vi.mocked(hooks.useCreateSnapshot).mockReturnValue({
      mutateAsync: mockMutateAsync,
      isPending: false,
    } as unknown as ReturnType<typeof hooks.useCreateSnapshot>);

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
      expect(mockMutateAsync).toHaveBeenCalled();
    });
  });

  it("should show snapshot success message after creation", async () => {
    vi.mocked(hooks.useCreateSnapshot).mockReturnValue({
      mutateAsync: vi.fn().mockResolvedValue("/path/to/snapshot.zip"),
      isPending: false,
    } as unknown as ReturnType<typeof hooks.useCreateSnapshot>);

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
    vi.mocked(hooks.useCreateSnapshot).mockReturnValue({
      mutateAsync: vi.fn().mockRejectedValue(new Error("Disk full")),
      isPending: false,
    } as unknown as ReturnType<typeof hooks.useCreateSnapshot>);

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
    vi.mocked(hooks.useAppState).mockReturnValue({
      data: { ...mockAppState, is_syncing: true },
      isLoading: false,
      isSuccess: true,
      isError: false,
      error: null,
    } as ReturnType<typeof hooks.useAppState>);

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

});
