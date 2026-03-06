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

  it("should return createSnapshotFn function and isPending state", () => {
    const { result } = renderHook(() => useCreateSnapshot());

    expect(result.current.createSnapshotFn).toBeInstanceOf(Function);
    expect(result.current.isPending).toBe(false);
  });

  it("should set isPending to true while creating snapshot", async () => {
    let resolvePromise: (value: string) => void;
    const mockPromise = new Promise<string>((resolve) => {
      resolvePromise = resolve;
    });
    vi.mocked(services.createSnapshot).mockReturnValue(mockPromise);

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
  });

  it("should call createSnapshot service with pubky", async () => {
    vi.mocked(services.createSnapshot).mockResolvedValue("/snapshots/test.zip");

    const { result } = renderHook(() => useCreateSnapshot());

    await act(async () => {
      await result.current.createSnapshotFn("test-pubky-123");
    });

    expect(services.createSnapshot).toHaveBeenCalledWith("test-pubky-123");
    expect(services.createSnapshot).toHaveBeenCalledTimes(1);
  });

  it("should return snapshot path from service", async () => {
    vi.mocked(services.createSnapshot).mockResolvedValue(
      "/path/to/my-snapshot.zip",
    );

    const { result } = renderHook(() => useCreateSnapshot());

    let snapshotPath: string;
    await act(async () => {
      snapshotPath = await result.current.createSnapshotFn("pubky");
    });

    expect(snapshotPath!).toBe("/path/to/my-snapshot.zip");
  });

  it("should set isPending to false on error", async () => {
    vi.mocked(services.createSnapshot).mockRejectedValue(
      new Error("Snapshot failed"),
    );

    const { result } = renderHook(() => useCreateSnapshot());

    await act(async () => {
      try {
        await result.current.createSnapshotFn("bad-pubky");
      } catch {
        // Expected to throw
      }
    });

    expect(result.current.isPending).toBe(false);
  });

  it("should propagate errors from service", async () => {
    const error = new Error("Disk full");
    vi.mocked(services.createSnapshot).mockRejectedValue(error);

    const { result } = renderHook(() => useCreateSnapshot());

    await expect(
      act(async () => {
        await result.current.createSnapshotFn("pubky");
      }),
    ).rejects.toThrow("Disk full");
  });

  it("should handle multiple sequential calls", async () => {
    vi.mocked(services.createSnapshot)
      .mockResolvedValueOnce("/snapshot-1.zip")
      .mockResolvedValueOnce("/snapshot-2.zip");

    const { result } = renderHook(() => useCreateSnapshot());

    let path1: string;
    let path2: string;

    await act(async () => {
      path1 = await result.current.createSnapshotFn("pubky-1");
    });
    expect(result.current.isPending).toBe(false);

    await act(async () => {
      path2 = await result.current.createSnapshotFn("pubky-2");
    });
    expect(result.current.isPending).toBe(false);

    expect(path1!).toBe("/snapshot-1.zip");
    expect(path2!).toBe("/snapshot-2.zip");
    expect(services.createSnapshot).toHaveBeenCalledTimes(2);
  });

  it("should be stable across re-renders", () => {
    const { result, rerender } = renderHook(() => useCreateSnapshot());

    const firstFn = result.current.createSnapshotFn;
    rerender();
    const secondFn = result.current.createSnapshotFn;

    expect(firstFn).toBe(secondFn);
  });
});
