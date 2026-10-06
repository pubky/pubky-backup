import { describe, it, expect, beforeEach, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useMirror } from "./useMirror";
import * as services from "@/services";

vi.mock("@/services", () => ({
  startMirror: vi.fn(),
  cancelMirror: vi.fn(),
  stopMirror: vi.fn(),
}));

describe("useMirror", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should call startMirror and toggle isPending", async () => {
    let resolvePromise: (url: string) => void;
    vi.mocked(services.startMirror).mockReturnValue(
      new Promise<string>((resolve) => {
        resolvePromise = resolve;
      }),
    );

    const { result } = renderHook(() => useMirror());
    expect(result.current.isPending).toBe(false);

    let mirrorPromise: Promise<void>;
    act(() => {
      mirrorPromise = result.current.startMirror("test-pubky", "homeserver");
    });

    expect(result.current.isPending).toBe(true);

    await act(async () => {
      resolvePromise!("pubkyauth://example");
      await mirrorPromise;
    });

    expect(result.current.isPending).toBe(false);
    expect(services.startMirror).toHaveBeenCalledWith(
      "test-pubky",
      "homeserver",
    );
  });

  it("should call cancelMirror and stopMirror for the given pubky", async () => {
    vi.mocked(services.cancelMirror).mockResolvedValue(undefined);
    vi.mocked(services.stopMirror).mockResolvedValue(undefined);
    const { result } = renderHook(() => useMirror());

    await act(async () => {
      await result.current.cancelMirror("test-pubky");
      await result.current.stopMirror("test-pubky");
    });

    expect(services.cancelMirror).toHaveBeenCalledWith("test-pubky");
    expect(services.stopMirror).toHaveBeenCalledWith("test-pubky");
  });

  it("should reset isPending and propagate errors", async () => {
    vi.mocked(services.stopMirror).mockRejectedValue(new Error("Failed"));
    const { result } = renderHook(() => useMirror());

    await expect(
      act(async () => {
        await result.current.stopMirror("test-pubky");
      }),
    ).rejects.toThrow("Failed");

    expect(result.current.isPending).toBe(false);
  });
});
