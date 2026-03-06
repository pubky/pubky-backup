import { describe, it, expect, beforeEach, vi } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { useDataDirPath } from "./useDataDirPath";
import * as services from "@/services";

vi.mock("@/services", () => ({
  getDataDirPath: vi.fn(),
}));

describe("useDataDirPath", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should fetch path on mount and update state", async () => {
    vi.mocked(services.getDataDirPath).mockResolvedValue(
      "/home/user/.pubky-backup",
    );

    const { result } = renderHook(() => useDataDirPath());

    expect(result.current.isLoading).toBe(true);
    expect(result.current.dataDirPath).toBeNull();

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.dataDirPath).toBe("/home/user/.pubky-backup");
    expect(services.getDataDirPath).toHaveBeenCalledTimes(1);
  });

  it("should handle errors gracefully", async () => {
    vi.mocked(services.getDataDirPath).mockRejectedValue(
      new Error("Failed to get path"),
    );

    const { result } = renderHook(() => useDataDirPath());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.dataDirPath).toBeNull();
  });
});
