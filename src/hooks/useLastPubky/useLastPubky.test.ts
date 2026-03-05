import { describe, it, expect, beforeEach, vi } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { useLastPubky } from "./useLastPubky";
import * as services from "@/services";

vi.mock("@/services", () => ({
  getLastPubky: vi.fn(),
}));

describe("useLastPubky", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should return lastPubky and isLoading state", async () => {
    vi.mocked(services.getLastPubky).mockResolvedValue("pk:test");

    const { result } = renderHook(() => useLastPubky());

    expect(result.current).toHaveProperty("lastPubky");
    expect(result.current).toHaveProperty("isLoading");

    // Wait for async effect to settle
    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });
  });

  it("should start with isLoading true and null lastPubky", async () => {
    vi.mocked(services.getLastPubky).mockResolvedValue("pk:test");

    const { result } = renderHook(() => useLastPubky());

    expect(result.current.isLoading).toBe(true);
    expect(result.current.lastPubky).toBeNull();

    // Wait for async effect to settle
    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });
  });

  it("should fetch last pubky on mount", async () => {
    vi.mocked(services.getLastPubky).mockResolvedValue("pk:my-last-pubky");

    const { result } = renderHook(() => useLastPubky());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.lastPubky).toBe("pk:my-last-pubky");
    expect(services.getLastPubky).toHaveBeenCalledTimes(1);
  });

  it("should handle null response (no last pubky)", async () => {
    vi.mocked(services.getLastPubky).mockResolvedValue(null);

    const { result } = renderHook(() => useLastPubky());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.lastPubky).toBeNull();
  });

  it("should set isLoading to false after successful fetch", async () => {
    vi.mocked(services.getLastPubky).mockResolvedValue("pk:test");

    const { result } = renderHook(() => useLastPubky());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });
  });

  it("should handle errors gracefully", async () => {
    vi.mocked(services.getLastPubky).mockRejectedValue(new Error("Failed to get last pubky"));

    const { result } = renderHook(() => useLastPubky());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    // Should not throw, just leave lastPubky as null
    expect(result.current.lastPubky).toBeNull();
  });

  it("should not refetch on rerender", async () => {
    vi.mocked(services.getLastPubky).mockResolvedValue("pk:test");

    const { result, rerender } = renderHook(() => useLastPubky());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    rerender();
    rerender();
    rerender();

    // Should only have been called once on mount
    expect(services.getLastPubky).toHaveBeenCalledTimes(1);
  });

  it("should handle long pubky strings", async () => {
    const longPubky = "pk:" + "a".repeat(100);
    vi.mocked(services.getLastPubky).mockResolvedValue(longPubky);

    const { result } = renderHook(() => useLastPubky());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.lastPubky).toBe(longPubky);
  });
});
