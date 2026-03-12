import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { SettingsPage } from "./SettingsPage";
import { useUIStore } from "@/stores/uiStore";
import * as services from "@/services";

// Mock services
vi.mock("@/services", () => ({
  getConfig: vi.fn(),
  setSyncInterval: vi.fn(),
  setBackupLocation: vi.fn(),
}));

// Mock @tauri-apps/plugin-dialog
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

// Mock error handler
vi.mock("@/utils/error-handler", () => ({
  handleBackendError: vi.fn(),
}));

describe("SettingsPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(services.getConfig).mockResolvedValue({
      developer_mode: false,
      sync_interval_secs: 300,
      keys_dir: "/home/user/.pubky-backup/keys",
    });
    vi.mocked(services.setSyncInterval).mockResolvedValue(undefined);

    useUIStore.setState({ navDisabled: false });
  });

  describe("rendering", () => {
    it("should render the settings header", async () => {
      render(<SettingsPage />);
      expect(screen.getByText("Settings")).toBeInTheDocument();
      expect(
        screen.getByText("Configure your backup preferences."),
      ).toBeInTheDocument();
    });

    it("should render sync interval buttons", async () => {
      render(<SettingsPage />);
      await waitFor(() => {
        expect(screen.getByText("30 sec")).toBeInTheDocument();
      });
      expect(screen.getByText("5 min")).toBeInTheDocument();
      expect(screen.getByText("10 min")).toBeInTheDocument();
      expect(screen.getByText("15 min")).toBeInTheDocument();
      expect(screen.getByText("30 min")).toBeInTheDocument();
      expect(screen.getByText("60 min")).toBeInTheDocument();
    });

    it("should show backup location after config loads", async () => {
      render(<SettingsPage />);
      await waitFor(() => {
        expect(
          screen.getByText("/home/user/.pubky-backup/keys"),
        ).toBeInTheDocument();
      });
    });

    it("should show Loading... before config loads", () => {
      vi.mocked(services.getConfig).mockReturnValue(new Promise(() => {}));
      render(<SettingsPage />);
      expect(screen.getByText("Loading...")).toBeInTheDocument();
    });
  });

  describe("sync interval selection", () => {
    it("should highlight the current interval from config", async () => {
      render(<SettingsPage />);
      await waitFor(() => {
        // 300s = "5 min" should be selected
        const button = screen.getByText("5 min").closest("button")!;
        expect(button.className).toContain("border-pubky-purple");
      });
    });

    it("should call setSyncInterval on button click", async () => {
      render(<SettingsPage />);
      await waitFor(() => {
        expect(screen.getByText("5 min")).toBeInTheDocument();
      });

      fireEvent.click(screen.getByText("10 min"));

      await waitFor(() => {
        expect(services.setSyncInterval).toHaveBeenCalledWith(600);
      });
    });

    it("should optimistically update selection", async () => {
      // Make setSyncInterval hang so we can inspect the intermediate state
      vi.mocked(services.setSyncInterval).mockReturnValue(
        new Promise(() => {}),
      );

      render(<SettingsPage />);
      await waitFor(() => {
        expect(screen.getByText("5 min")).toBeInTheDocument();
      });

      fireEvent.click(screen.getByText("10 min"));

      // Should immediately show 10 min as selected
      await waitFor(() => {
        const button = screen.getByText("10 min").closest("button")!;
        expect(button.className).toContain("border-pubky-purple");
      });
    });

    it("should rollback on error", async () => {
      vi.mocked(services.setSyncInterval).mockRejectedValue(
        new Error("Failed"),
      );

      render(<SettingsPage />);
      await waitFor(() => {
        expect(screen.getByText("5 min")).toBeInTheDocument();
      });

      fireEvent.click(screen.getByText("10 min"));

      // Should rollback to 5 min (300s)
      await waitFor(() => {
        const button = screen.getByText("5 min").closest("button")!;
        expect(button.className).toContain("border-pubky-purple");
      });
    });

    it("should disable nav during interval save", async () => {
      let resolveSave: () => void;
      vi.mocked(services.setSyncInterval).mockReturnValue(
        new Promise((resolve) => {
          resolveSave = resolve;
        }),
      );

      render(<SettingsPage />);
      await waitFor(() => {
        expect(screen.getByText("5 min")).toBeInTheDocument();
      });

      fireEvent.click(screen.getByText("10 min"));

      // Nav should be disabled while saving
      expect(useUIStore.getState().navDisabled).toBe(true);

      // Resolve the save
      resolveSave!();
      await waitFor(() => {
        expect(useUIStore.getState().navDisabled).toBe(false);
      });
    });

    it("should disable interval buttons while saving", async () => {
      vi.mocked(services.setSyncInterval).mockReturnValue(
        new Promise(() => {}),
      );

      render(<SettingsPage />);
      await waitFor(() => {
        expect(screen.getByText("5 min")).toBeInTheDocument();
      });

      fireEvent.click(screen.getByText("10 min"));

      // All interval buttons should be disabled
      await waitFor(() => {
        const buttons = screen
          .getAllByRole("button")
          .filter((b) => b.closest("[class*='grid']"));
        for (const btn of buttons) {
          expect(btn).toBeDisabled();
        }
      });
    });
  });

  describe("backup location", () => {
    it("should show moving state with progress bar", async () => {
      const { open } = await import("@tauri-apps/plugin-dialog");
      vi.mocked(open).mockResolvedValue("/new/path");
      vi.mocked(services.setBackupLocation).mockReturnValue(
        new Promise(() => {}),
      );

      render(<SettingsPage />);
      await waitFor(() => {
        expect(
          screen.getByText("/home/user/.pubky-backup/keys"),
        ).toBeInTheDocument();
      });

      // Click the browse button (folder icon button)
      const browseButtons = screen.getAllByRole("button");
      const folderButton = browseButtons.find((b) => b.querySelector("svg"));
      fireEvent.click(folderButton!);

      await waitFor(() => {
        expect(
          screen.getByText("Copying data to new location..."),
        ).toBeInTheDocument();
      });
    });

    it("should update location after successful move", async () => {
      const { open } = await import("@tauri-apps/plugin-dialog");
      vi.mocked(open).mockResolvedValue("/new/path");
      vi.mocked(services.setBackupLocation).mockResolvedValue("/new/path/keys");

      render(<SettingsPage />);
      await waitFor(() => {
        expect(
          screen.getByText("/home/user/.pubky-backup/keys"),
        ).toBeInTheDocument();
      });

      const browseButtons = screen.getAllByRole("button");
      const folderButton = browseButtons.find((b) => b.querySelector("svg"));
      fireEvent.click(folderButton!);

      await waitFor(() => {
        expect(screen.getByText("/new/path/keys")).toBeInTheDocument();
      });
    });

    it("should rollback location on error", async () => {
      const { open } = await import("@tauri-apps/plugin-dialog");
      vi.mocked(open).mockResolvedValue("/new/path");
      vi.mocked(services.setBackupLocation).mockRejectedValue(
        new Error("Move failed"),
      );

      render(<SettingsPage />);
      await waitFor(() => {
        expect(
          screen.getByText("/home/user/.pubky-backup/keys"),
        ).toBeInTheDocument();
      });

      const browseButtons = screen.getAllByRole("button");
      const folderButton = browseButtons.find((b) => b.querySelector("svg"));
      fireEvent.click(folderButton!);

      // Should restore original location
      await waitFor(() => {
        expect(
          screen.getByText("/home/user/.pubky-backup/keys"),
        ).toBeInTheDocument();
      });
    });

    it("should not move if user cancels dialog", async () => {
      const { open } = await import("@tauri-apps/plugin-dialog");
      vi.mocked(open).mockResolvedValue(null);

      render(<SettingsPage />);
      await waitFor(() => {
        expect(
          screen.getByText("/home/user/.pubky-backup/keys"),
        ).toBeInTheDocument();
      });

      const browseButtons = screen.getAllByRole("button");
      const folderButton = browseButtons.find((b) => b.querySelector("svg"));
      fireEvent.click(folderButton!);

      // Should not call setBackupLocation
      expect(services.setBackupLocation).not.toHaveBeenCalled();
    });
  });

  describe("config loading failure", () => {
    it("should handle config fetch error gracefully", async () => {
      vi.mocked(services.getConfig).mockRejectedValue(
        new Error("Failed to fetch"),
      );

      render(<SettingsPage />);

      // Should still render without crashing
      expect(screen.getByText("Settings")).toBeInTheDocument();
      // Should show loading state since config never loaded
      expect(screen.getByText("Loading...")).toBeInTheDocument();
    });
  });
});
