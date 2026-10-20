import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  render,
  screen,
  fireEvent,
  waitFor,
  act,
} from "@testing-library/react";
import { DashboardForm } from "./DashboardForm";
import { useUIStore } from "@/stores/uiStore";
import type { KeyState } from "@/stores/uiStore";
import * as services from "@/services";
import * as Utils from "@/utils";

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
    auth: { type: "SignedOut" },
    mirror: { type: "NotMirrored" },
  };

  const syncingKeyState: KeyState = {
    status: { type: "Syncing", events_processed: 5 },
    data_size: 2048,
    last_sync: null,
    next_sync: null,
    error: null,
    auth: { type: "SignedOut" },
    mirror: { type: "NotMirrored" },
  };

  const erroredKeyState: KeyState = {
    ...mockKeyState,
    status: { type: "Error" },
    error: { message: "Sync error: Storage error: disk full" },
  };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(services.getConfig).mockResolvedValue({
      sync_interval_secs: 300,
      keys_dir: "/home/user/.pubky-backup/keys",
    });

    useUIStore.setState({
      keyStates: { "pk:test-pubky": mockKeyState },
      lastPubky: "pk:test-pubky",
      statusMessageMode: "sync",
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

  describe("error state", () => {
    beforeEach(() => {
      useUIStore.setState({
        keyStates: { "pk:test-pubky": erroredKeyState },
        lastPubky: "pk:test-pubky",
      });
    });

    it("should show the error badge instead of synced", () => {
      render(<DashboardForm />);

      expect(screen.getByText("ERROR")).toBeInTheDocument();
      expect(screen.queryByText("SYNCED")).not.toBeInTheDocument();
    });

    it("should show the error message instead of the countdown", () => {
      render(<DashboardForm />);

      expect(
        screen.getByText("Sync error: Storage error: disk full"),
      ).toHaveClass("text-pubky-red");
      expect(screen.queryByText(/next backup/i)).not.toBeInTheDocument();
    });

    it("should show a fallback message when the error has none", () => {
      useUIStore.setState({
        keyStates: { "pk:test-pubky": { ...erroredKeyState, error: null } },
      });

      render(<DashboardForm />);

      expect(screen.getByText("Backup failed")).toBeInTheDocument();
    });

    it("should retry the backup by forcing a sync", async () => {
      vi.mocked(services.forceSyncNow).mockResolvedValue(undefined);

      render(<DashboardForm />);

      fireEvent.click(screen.getByRole("button", { name: "Retry" }));

      await waitFor(() => {
        expect(services.forceSyncNow).toHaveBeenCalledWith("pk:test-pubky");
      });
    });

    it("should go back to synced once the retried backup succeeds", async () => {
      render(<DashboardForm />);
      expect(screen.getByText("ERROR")).toBeInTheDocument();

      act(() => {
        useUIStore.getState().setKeyState("pk:test-pubky", mockKeyState);
      });

      await waitFor(() => {
        expect(screen.getByText("SYNCED")).toBeInTheDocument();
      });
      expect(
        screen.queryByText("Sync error: Storage error: disk full"),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByRole("button", { name: "Retry" }),
      ).not.toBeInTheDocument();
    });

    it("should show an error toast when the retry fails", async () => {
      vi.mocked(services.forceSyncNow).mockRejectedValue({
        type: "Internal",
        message: "Failed to send force sync message",
      });
      useUIStore.setState({
        toast: { visible: false, pubkyText: "", type: "success", message: "" },
      });

      render(<DashboardForm />);

      fireEvent.click(screen.getByRole("button", { name: "Retry" }));

      await waitFor(() => {
        expect(useUIStore.getState().toast).toMatchObject({
          visible: true,
          type: "error",
        });
      });
    });

    it("should show syncing and hide retry once the retried backup syncs", async () => {
      render(<DashboardForm />);
      expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();

      act(() => {
        useUIStore.getState().setKeyState("pk:test-pubky", syncingKeyState);
      });

      await waitFor(() => {
        expect(screen.getByText("SYNCING")).toBeInTheDocument();
      });
      expect(
        screen.queryByRole("button", { name: "Retry" }),
      ).not.toBeInTheDocument();
    });

    it("should keep the error badge when a snapshot fails", async () => {
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
      expect(screen.getByText("ERROR")).toBeInTheDocument();
      expect(screen.queryByText("SYNCED")).not.toBeInTheDocument();
      expect(
        screen.queryByRole("button", { name: "Retry" }),
      ).not.toBeInTheDocument();
    });

    it("should not offer a retry while the backup is healthy", () => {
      useUIStore.setState({
        keyStates: { "pk:test-pubky": mockKeyState },
      });

      render(<DashboardForm />);

      expect(
        screen.queryByRole("button", { name: "Retry" }),
      ).not.toBeInTheDocument();
    });

    it("should mark the errored key in the key switcher", () => {
      useUIStore.setState({
        keyStates: {
          "pk:test-pubky": erroredKeyState,
          "pk:other-pubky": mockKeyState,
        },
      });

      render(<DashboardForm />);

      fireEvent.click(screen.getByTitle("Switch key"));

      const marker = screen.getByTitle("Backup failed");
      const [errored, healthy] = ["pk:test-pubky", "pk:other-pubky"].map((k) =>
        screen
          .getAllByRole("option")
          .find((o) => o.textContent?.includes(Utils.displayPubky(k))),
      );
      expect(errored).toContainElement(marker);
      expect(healthy).not.toContainElement(marker);
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

    it("should show an error toast when force sync fails", async () => {
      vi.mocked(services.forceSyncNow).mockRejectedValue({
        type: "Internal",
        message: "Failed to send force sync message",
      });
      useUIStore.setState({
        toast: { visible: false, pubkyText: "", type: "success", message: "" },
      });

      render(<DashboardForm />);

      fireEvent.click(screen.getByRole("button", { name: /force sync/i }));

      await waitFor(() => {
        expect(useUIStore.getState().toast).toMatchObject({
          visible: true,
          type: "error",
        });
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
