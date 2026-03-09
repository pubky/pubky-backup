import { describe, it, expect, beforeEach, vi } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { useBackupLocation } from "./useBackupLocation";
import * as services from "@/services";

vi.mock("@/services", () => ({
  getConfig: vi.fn(),
}));

describe("useBackupLocation", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should fetch path on mount and update state", async () => {
    vi.mocked(services.getConfig).mockResolvedValue({
      developer_mode: false,
      sync_interval_secs: 300,
      keys_dir: "/home/user/.pubky-backup/keys",
    });

    const { result } = renderHook(() => useBackupLocation());

    expect(result.current.isLoading).toBe(true);
    expect(result.current.backupLocation).toBeNull();

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.backupLocation).toBe("/home/user/.pubky-backup/keys");
    expect(services.getConfig).toHaveBeenCalledTimes(1);
  });

  it("should handle errors gracefully", async () => {
    vi.mocked(services.getConfig).mockRejectedValue(
      new Error("Failed to get config"),
    );

    const { result } = renderHook(() => useBackupLocation());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.backupLocation).toBeNull();
  });
});
