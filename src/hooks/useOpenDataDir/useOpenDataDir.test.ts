import { describe, it, expect, beforeEach, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useOpenDataDir } from "./useOpenDataDir";
import * as services from "@/services";

vi.mock("@/services", () => ({
  openDataDir: vi.fn(),
}));

describe("useOpenDataDir", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should return openDir function and isPending state", () => {
    const { result } = renderHook(() => useOpenDataDir());

    expect(result.current.openDir).toBeInstanceOf(Function);
    expect(result.current.isPending).toBe(false);
  });

  it("should set isPending to true while opening directory", async () => {
    let resolvePromise: () => void;
    const mockPromise = new Promise<void>((resolve) => {
      resolvePromise = resolve;
    });
    vi.mocked(services.openDataDir).mockReturnValue(mockPromise);

    const { result } = renderHook(() => useOpenDataDir());

    expect(result.current.isPending).toBe(false);

    let openPromise: Promise<void>;
    act(() => {
      openPromise = result.current.openDir();
    });

    expect(result.current.isPending).toBe(true);

    await act(async () => {
      resolvePromise!();
      await openPromise;
    });

    expect(result.current.isPending).toBe(false);
  });

  it("should call openDataDir service", async () => {
    vi.mocked(services.openDataDir).mockResolvedValue(undefined);

    const { result } = renderHook(() => useOpenDataDir());

    await act(async () => {
      await result.current.openDir();
    });

    expect(services.openDataDir).toHaveBeenCalledTimes(1);
  });

  it("should set isPending to false on error", async () => {
    vi.mocked(services.openDataDir).mockRejectedValue(
      new Error("Failed to open"),
    );

    const { result } = renderHook(() => useOpenDataDir());

    await act(async () => {
      try {
        await result.current.openDir();
      } catch {
        // Expected to throw
      }
    });

    expect(result.current.isPending).toBe(false);
  });

  it("should propagate errors from service", async () => {
    const error = new Error("Directory not found");
    vi.mocked(services.openDataDir).mockRejectedValue(error);

    const { result } = renderHook(() => useOpenDataDir());

    await expect(
      act(async () => {
        await result.current.openDir();
      }),
    ).rejects.toThrow("Directory not found");
  });

  it("should handle multiple sequential calls", async () => {
    vi.mocked(services.openDataDir).mockResolvedValue(undefined);

    const { result } = renderHook(() => useOpenDataDir());

    await act(async () => {
      await result.current.openDir();
    });
    expect(result.current.isPending).toBe(false);

    await act(async () => {
      await result.current.openDir();
    });
    expect(result.current.isPending).toBe(false);

    expect(services.openDataDir).toHaveBeenCalledTimes(2);
  });

  it("should be stable across re-renders", () => {
    const { result, rerender } = renderHook(() => useOpenDataDir());

    const firstOpenDir = result.current.openDir;
    rerender();
    const secondOpenDir = result.current.openDir;

    expect(firstOpenDir).toBe(secondOpenDir);
  });
});
