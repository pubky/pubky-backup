import { describe, it, expect, beforeEach, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useCreateSnapshot } from "./useCreateSnapshot";
import * as services from "@/services";

vi.mock("@/services", () => ({
  createSnapshot: vi.fn(),
}));

describe("useCreateSnapshot", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should call createSnapshot, toggle isPending, and return path", async () => {
    let resolvePromise: (value: string) => void;
    vi.mocked(services.createSnapshot).mockReturnValue(
      new Promise<string>((resolve) => {
        resolvePromise = resolve;
      }),
    );

    const { result } = renderHook(() => useCreateSnapshot());
    expect(result.current.isPending).toBe(false);

    let snapshotPromise: Promise<string>;
    act(() => {
      snapshotPromise = result.current.createSnapshotFn("my-pubky");
    });

    expect(result.current.isPending).toBe(true);

    await act(async () => {
      resolvePromise!("/path/to/snapshot.zip");
      await snapshotPromise;
    });

    expect(result.current.isPending).toBe(false);
    expect(services.createSnapshot).toHaveBeenCalledWith("my-pubky");
  });

  it("should reset isPending and propagate errors", async () => {
    vi.mocked(services.createSnapshot).mockRejectedValue(
      new Error("Disk full"),
    );

    const { result } = renderHook(() => useCreateSnapshot());

    await expect(
      act(async () => {
        await result.current.createSnapshotFn("pubky");
      }),
    ).rejects.toThrow("Disk full");

    expect(result.current.isPending).toBe(false);
  });
});
