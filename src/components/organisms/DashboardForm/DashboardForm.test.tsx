import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { DashboardForm } from "./DashboardForm";
import { useUIStore } from "@/stores/uiStore";
import type { KeyState } from "@/stores/uiStore";
import * as services from "@/services";

// Mock services
vi.mock("@/services", () => ({
  forceSyncNow: vi.fn(),
  createSnapshot: vi.fn(),
  getConfig: vi.fn(),
  openDataDir: vi.fn(),
}));

// Mock clipboard API
const mockWriteText = vi.fn().mockResolvedValue(undefined);
Object.defineProperty(navigator, "clipboard", {
  value: { writeText: mockWriteText },
  writable: true,
  configurable: true,
});

describe("DashboardForm", () => {
  const mockKeyState: KeyState = {
    status: { type: "Idle" },
    data_size: 1024,
    last_sync: Math.floor(Date.now() / 1000) - 60, // 1 minute ago
    next_sync: Math.floor(Date.now() / 1000) + 30, // 30 seconds from now
    error: null,
    total_files: null,
    files_synced: null,
    bytes_downloaded: null,
  };

  const syncingKeyState: KeyState = {
    status: { type: "Syncing", events_processed: 5 },
    data_size: 2048,
    last_sync: null,
    next_sync: null,
    error: null,
    total_files: 100,
    files_synced: 50,
    bytes_downloaded: 1024,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(services.getConfig).mockResolvedValue({
      developer_mode: false,
      sync_interval_secs: 300,
      keys_dir: "/home/user/.pubky-backup/keys",
    });

    useUIStore.setState({
      keyStates: { "pk:test-pubky": mockKeyState },
      lastPubky: "pk:test-pubky",
      statusMessageMode: "sync",
      developerMode: false,
    });
  });

  describe("rendering", () => {
    it("should render dashboard header with pubky", async () => {
      render(<DashboardForm />);
      await waitFor(() => {
        expect(screen.getByText(/test-pubky/i)).toBeInTheDocument();
      });
    });

    it("should render sync message area", async () => {
      render(<DashboardForm />);
      await waitFor(() => {
        expect(
          screen.getByRole("button", { name: /force sync/i }),
        ).toBeInTheDocument();
      });
    });

    it("should render info cards", async () => {
      render(<DashboardForm />);
      await waitFor(() => {
        expect(screen.getByText("Backup Size")).toBeInTheDocument();
      });
      expect(screen.getByText("Last Sync")).toBeInTheDocument();
      expect(screen.getByText("Backup Location")).toBeInTheDocument();
    });

    it("should render action buttons", async () => {
      render(<DashboardForm />);
      await waitFor(() => {
        expect(
          screen.getByRole("button", { name: /create snapshot/i }),
        ).toBeInTheDocument();
      });
      expect(
        screen.getByRole("button", { name: /force sync/i }),
      ).toBeInTheDocument();
    });

    it("should display formatted backup size", async () => {
      render(<DashboardForm />);
      await waitFor(() => {
        // 1024 bytes should display as "1 KB" or similar
        expect(screen.getByText(/1.*KB/i)).toBeInTheDocument();
      });
    });

    it("should display data directory path", async () => {
      render(<DashboardForm />);

      await waitFor(() => {
        expect(screen.getByText(/\.pubky-backup/i)).toBeInTheDocument();
      });
    });
  });

  describe("syncing state", () => {
    it("should show syncing badge when syncing", async () => {
      useUIStore.setState({
        keyStates: { "pk:test-pubky": syncingKeyState },
        lastPubky: "pk:test-pubky",
      });

      render(<DashboardForm />);

      await waitFor(() => {
        // StatusBadge displays "SYNCING" in uppercase
        expect(screen.getByText("SYNCING")).toBeInTheDocument();
      });
    });

    it("should disable action buttons when syncing", async () => {
      useUIStore.setState({
        keyStates: { "pk:test-pubky": syncingKeyState },
        lastPubky: "pk:test-pubky",
      });

      render(<DashboardForm />);

      await waitFor(() => {
        expect(
          screen.getByRole("button", { name: /create snapshot/i }),
        ).toBeDisabled();
      });
      expect(
        screen.getByRole("button", { name: /force sync/i }),
      ).toBeDisabled();
    });
  });

  describe("copy functionality", () => {
    it("should copy pubky to clipboard when copy button is clicked", async () => {
      render(<DashboardForm />);

      // DashboardHeader uses "Copy full pubky" as the title
      const copyButton = screen.getByTitle("Copy full pubky");
      fireEvent.click(copyButton);

      await waitFor(() => {
        expect(mockWriteText).toHaveBeenCalledWith("pk:test-pubky");
      });
    });

    it("should show toast after copying", async () => {
      useUIStore.setState({
        toast: { visible: false, pubkyText: "", type: "success", message: "" },
      });

      render(<DashboardForm />);

      // DashboardHeader uses "Copy full pubky" as the title
      const copyButton = screen.getByTitle("Copy full pubky");
      fireEvent.click(copyButton);

      await waitFor(() => {
        expect(useUIStore.getState().toast.visible).toBe(true);
      });
    });
  });

  describe("force sync", () => {
    it("should call forceSyncNow when force sync button is clicked", async () => {
      vi.mocked(services.forceSyncNow).mockResolvedValue(undefined);

      render(<DashboardForm />);

      fireEvent.click(screen.getByRole("button", { name: /force sync/i }));

      await waitFor(() => {
        expect(services.forceSyncNow).toHaveBeenCalledWith("pk:test-pubky");
      });
    });
  });

  describe("snapshot creation", () => {
    it("should call createSnapshot when snapshot button is clicked", async () => {
      vi.mocked(services.createSnapshot).mockResolvedValue(
        "/path/to/snapshot.zip",
      );

      render(<DashboardForm />);

      fireEvent.click(screen.getByRole("button", { name: /create snapshot/i }));

      await waitFor(() => {
        expect(services.createSnapshot).toHaveBeenCalledWith("pk:test-pubky");
      });
    });

    it("should show success message after snapshot creation", async () => {
      vi.mocked(services.createSnapshot).mockResolvedValue(
        "/path/to/snapshot.zip",
      );

      render(<DashboardForm />);

      fireEvent.click(screen.getByRole("button", { name: /create snapshot/i }));

      await waitFor(() => {
        expect(screen.getByText(/snapshot created/i)).toBeInTheDocument();
      });
    });

    it("should show error message when snapshot fails", async () => {
      vi.mocked(services.createSnapshot).mockRejectedValue(
        new Error("Disk full"),
      );

      render(<DashboardForm />);

      fireEvent.click(screen.getByRole("button", { name: /create snapshot/i }));

      await waitFor(() => {
        expect(
          screen.getByText(/failed to create snapshot/i),
        ).toBeInTheDocument();
      });
    });

    it("should set statusMessageMode to snapshot-success on successful snapshot", async () => {
      vi.mocked(services.createSnapshot).mockResolvedValue(
        "/path/to/snapshot.zip",
      );
      useUIStore.setState({ statusMessageMode: "sync" });

      render(<DashboardForm />);

      fireEvent.click(screen.getByRole("button", { name: /create snapshot/i }));

      await waitFor(() => {
        expect(useUIStore.getState().statusMessageMode).toBe(
          "snapshot-success",
        );
      });
    });
  });

  describe("open data directory", () => {
    it("should call openDataDir when folder icon is clicked", async () => {
      vi.mocked(services.openDataDir).mockResolvedValue(undefined);

      render(<DashboardForm />);

      // Wait for data dir path to load
      await waitFor(
        () => {
          expect(screen.getByText(/\.pubky-backup/i)).toBeInTheDocument();
        },
        { timeout: 2000 },
      );

      const openDirButton = screen.getByTitle("Open data directory");
      fireEvent.click(openDirButton);

      await waitFor(
        () => {
          expect(services.openDataDir).toHaveBeenCalled();
        },
        { timeout: 1000 },
      );
    });
  });

  describe("null pubky handling", () => {
    it("should handle null lastPubky gracefully", async () => {
      useUIStore.setState({
        keyStates: {},
        lastPubky: null,
      });

      render(<DashboardForm />);

      await waitFor(() => {
        // Should render without crashing
        expect(screen.getByText("Backup Size")).toBeInTheDocument();
      });
    });
  });
});
