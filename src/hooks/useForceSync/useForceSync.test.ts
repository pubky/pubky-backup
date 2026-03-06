import { describe, it, expect, beforeEach, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useForceSync } from "./useForceSync";
import * as services from "@/services";

vi.mock("@/services", () => ({
  forceSyncNow: vi.fn(),
}));

describe("useForceSync", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should return forceSync function and isPending state", () => {
    const { result } = renderHook(() => useForceSync());

    expect(result.current.forceSync).toBeInstanceOf(Function);
    expect(result.current.isPending).toBe(false);
  });

  it("should set isPending to true while syncing", async () => {
    let resolvePromise: () => void;
    const mockPromise = new Promise<void>((resolve) => {
      resolvePromise = resolve;
    });
    vi.mocked(services.forceSyncNow).mockReturnValue(mockPromise);

    const { result } = renderHook(() => useForceSync());

    expect(result.current.isPending).toBe(false);

    let syncPromise: Promise<void>;
    act(() => {
      syncPromise = result.current.forceSync("my-pubky");
    });

    expect(result.current.isPending).toBe(true);

    await act(async () => {
      resolvePromise!();
      await syncPromise;
    });

    expect(result.current.isPending).toBe(false);
  });

  it("should call forceSyncNow service with pubky", async () => {
    vi.mocked(services.forceSyncNow).mockResolvedValue(undefined);

    const { result } = renderHook(() => useForceSync());

    await act(async () => {
      await result.current.forceSync("test-pubky-123");
    });

    expect(services.forceSyncNow).toHaveBeenCalledWith("test-pubky-123");
    expect(services.forceSyncNow).toHaveBeenCalledTimes(1);
  });

  it("should set isPending to false on error", async () => {
    vi.mocked(services.forceSyncNow).mockRejectedValue(
      new Error("Sync failed"),
    );

    const { result } = renderHook(() => useForceSync());

    await act(async () => {
      try {
        await result.current.forceSync("bad-pubky");
      } catch {
        // Expected to throw
      }
    });

    expect(result.current.isPending).toBe(false);
  });

  it("should propagate errors from service", async () => {
    const error = new Error("Network error");
    vi.mocked(services.forceSyncNow).mockRejectedValue(error);

    const { result } = renderHook(() => useForceSync());

    await expect(
      act(async () => {
        await result.current.forceSync("pubky");
      }),
    ).rejects.toThrow("Network error");
  });

  it("should handle multiple sequential calls", async () => {
    vi.mocked(services.forceSyncNow).mockResolvedValue(undefined);

    const { result } = renderHook(() => useForceSync());

    await act(async () => {
      await result.current.forceSync("pubky-1");
    });
    expect(result.current.isPending).toBe(false);

    await act(async () => {
      await result.current.forceSync("pubky-2");
    });
    expect(result.current.isPending).toBe(false);

    expect(services.forceSyncNow).toHaveBeenCalledTimes(2);
    expect(services.forceSyncNow).toHaveBeenNthCalledWith(1, "pubky-1");
    expect(services.forceSyncNow).toHaveBeenNthCalledWith(2, "pubky-2");
  });

  it("should be stable across re-renders", () => {
    const { result, rerender } = renderHook(() => useForceSync());

    const firstForceSync = result.current.forceSync;
    rerender();
    const secondForceSync = result.current.forceSync;

    expect(firstForceSync).toBe(secondForceSync);
  });
});
