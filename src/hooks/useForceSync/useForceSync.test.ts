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

  it("should call forceSyncNow and toggle isPending", async () => {
    let resolvePromise: () => void;
    vi.mocked(services.forceSyncNow).mockReturnValue(
      new Promise<void>((resolve) => {
        resolvePromise = resolve;
      }),
    );

    const { result } = renderHook(() => useForceSync());
    expect(result.current.isPending).toBe(false);

    let syncPromise: Promise<void>;
    act(() => {
      syncPromise = result.current.forceSync("test-pubky");
    });

    expect(result.current.isPending).toBe(true);

    await act(async () => {
      resolvePromise!();
      await syncPromise;
    });

    expect(result.current.isPending).toBe(false);
    expect(services.forceSyncNow).toHaveBeenCalledWith("test-pubky");
  });

  it("should reset isPending and propagate errors", async () => {
    vi.mocked(services.forceSyncNow).mockRejectedValue(
      new Error("Sync failed"),
    );

    const { result } = renderHook(() => useForceSync());

    await expect(
      act(async () => {
        await result.current.forceSync("pubky");
      }),
    ).rejects.toThrow("Sync failed");

    expect(result.current.isPending).toBe(false);
  });
});
