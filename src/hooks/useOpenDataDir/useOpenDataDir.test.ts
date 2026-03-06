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

  it("should call openDataDir and toggle isPending", async () => {
    let resolvePromise: () => void;
    vi.mocked(services.openDataDir).mockReturnValue(
      new Promise<void>((resolve) => {
        resolvePromise = resolve;
      }),
    );

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
    expect(services.openDataDir).toHaveBeenCalledTimes(1);
  });

  it("should reset isPending and propagate errors", async () => {
    vi.mocked(services.openDataDir).mockRejectedValue(
      new Error("Failed to open"),
    );

    const { result } = renderHook(() => useOpenDataDir());

    await expect(
      act(async () => {
        await result.current.openDir();
      }),
    ).rejects.toThrow("Failed to open");

    expect(result.current.isPending).toBe(false);
  });
});
