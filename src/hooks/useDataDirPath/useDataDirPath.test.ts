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

  it("should return dataDirPath and isLoading state", async () => {
    vi.mocked(services.getDataDirPath).mockResolvedValue("/home/user/.pubky-backup");

    const { result } = renderHook(() => useDataDirPath());

    expect(result.current).toHaveProperty("dataDirPath");
    expect(result.current).toHaveProperty("isLoading");

    // Wait for async effect to settle
    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });
  });

  it("should start with isLoading true and null dataDirPath", async () => {
    vi.mocked(services.getDataDirPath).mockResolvedValue("/path");

    const { result } = renderHook(() => useDataDirPath());

    expect(result.current.isLoading).toBe(true);
    expect(result.current.dataDirPath).toBeNull();

    // Wait for async effect to settle
    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });
  });

  it("should fetch data dir path on mount", async () => {
    vi.mocked(services.getDataDirPath).mockResolvedValue("/home/user/.pubky-backup");

    const { result } = renderHook(() => useDataDirPath());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.dataDirPath).toBe("/home/user/.pubky-backup");
    expect(services.getDataDirPath).toHaveBeenCalledTimes(1);
  });

  it("should set isLoading to false after successful fetch", async () => {
    vi.mocked(services.getDataDirPath).mockResolvedValue("/path/to/data");

    const { result } = renderHook(() => useDataDirPath());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.dataDirPath).toBe("/path/to/data");
  });

  it("should handle errors gracefully", async () => {
    vi.mocked(services.getDataDirPath).mockRejectedValue(new Error("Failed to get path"));

    const { result } = renderHook(() => useDataDirPath());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    // Should not throw, just leave dataDirPath as null
    expect(result.current.dataDirPath).toBeNull();
  });

  it("should not refetch on rerender", async () => {
    vi.mocked(services.getDataDirPath).mockResolvedValue("/path");

    const { result, rerender } = renderHook(() => useDataDirPath());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    rerender();
    rerender();
    rerender();

    // Should only have been called once on mount
    expect(services.getDataDirPath).toHaveBeenCalledTimes(1);
  });

  it("should handle different path formats", async () => {
    vi.mocked(services.getDataDirPath).mockResolvedValue("C:\\Users\\test\\.pubky-backup");

    const { result } = renderHook(() => useDataDirPath());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.dataDirPath).toBe("C:\\Users\\test\\.pubky-backup");
  });
});
