import { describe, it, expect, beforeEach, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useSetViewedPubky } from "./useSetViewedPubky";
import { useUIStore } from "@/stores/uiStore";
import * as services from "@/services";

vi.mock("@/services", () => ({
  setLastPubky: vi.fn(),
}));

describe("useSetViewedPubky", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useUIStore.setState({
      viewedPubky: null,
      keyStates: {},
    });
  });

  it("should return setViewedPubky function and isPending state", () => {
    const { result } = renderHook(() => useSetViewedPubky());

    expect(result.current.setViewedPubky).toBeInstanceOf(Function);
    expect(result.current.isPending).toBe(false);
  });

  it("should set isPending to true while setting pubky", async () => {
    let resolvePromise: () => void;
    const mockPromise = new Promise<void>((resolve) => {
      resolvePromise = resolve;
    });
    vi.mocked(services.setLastPubky).mockReturnValue(mockPromise);

    const { result } = renderHook(() => useSetViewedPubky());

    expect(result.current.isPending).toBe(false);

    let setPromise: Promise<void>;
    act(() => {
      setPromise = result.current.setViewedPubky("new-pubky");
    });

    expect(result.current.isPending).toBe(true);

    await act(async () => {
      resolvePromise!();
      await setPromise;
    });

    expect(result.current.isPending).toBe(false);
  });

  it("should update viewedPubky in store immediately", async () => {
    vi.mocked(services.setLastPubky).mockResolvedValue(undefined);

    const { result } = renderHook(() => useSetViewedPubky());

    await act(async () => {
      await result.current.setViewedPubky("my-pubky");
    });

    const state = useUIStore.getState();
    expect(state.viewedPubky).toBe("my-pubky");
  });

  it("should call setLastPubky service for persistence", async () => {
    vi.mocked(services.setLastPubky).mockResolvedValue(undefined);

    const { result } = renderHook(() => useSetViewedPubky());

    await act(async () => {
      await result.current.setViewedPubky("persist-pubky");
    });

    expect(services.setLastPubky).toHaveBeenCalledWith("persist-pubky");
    expect(services.setLastPubky).toHaveBeenCalledTimes(1);
  });

  it("should set isPending to false on error", async () => {
    vi.mocked(services.setLastPubky).mockRejectedValue(new Error("Failed to persist"));

    const { result } = renderHook(() => useSetViewedPubky());

    await act(async () => {
      try {
        await result.current.setViewedPubky("bad-pubky");
      } catch {
        // Expected to throw
      }
    });

    expect(result.current.isPending).toBe(false);
  });

  it("should still update store even if persistence fails", async () => {
    vi.mocked(services.setLastPubky).mockRejectedValue(new Error("Failed"));

    const { result } = renderHook(() => useSetViewedPubky());

    await act(async () => {
      try {
        await result.current.setViewedPubky("my-pubky");
      } catch {
        // Expected to throw
      }
    });

    // Store was updated before the service call
    const state = useUIStore.getState();
    expect(state.viewedPubky).toBe("my-pubky");
  });

  it("should propagate errors from service", async () => {
    const error = new Error("Persistence failed");
    vi.mocked(services.setLastPubky).mockRejectedValue(error);

    const { result } = renderHook(() => useSetViewedPubky());

    await expect(
      act(async () => {
        await result.current.setViewedPubky("bad-pubky");
      })
    ).rejects.toThrow("Persistence failed");
  });

  it("should update viewedPubky when switching keys", async () => {
    vi.mocked(services.setLastPubky).mockResolvedValue(undefined);
    useUIStore.setState({ viewedPubky: "first-pubky" });

    const { result } = renderHook(() => useSetViewedPubky());

    await act(async () => {
      await result.current.setViewedPubky("second-pubky");
    });

    const state = useUIStore.getState();
    expect(state.viewedPubky).toBe("second-pubky");
  });

  it("should handle multiple rapid calls", async () => {
    vi.mocked(services.setLastPubky).mockResolvedValue(undefined);

    const { result } = renderHook(() => useSetViewedPubky());

    await act(async () => {
      await result.current.setViewedPubky("first");
      await result.current.setViewedPubky("second");
      await result.current.setViewedPubky("third");
    });

    expect(services.setLastPubky).toHaveBeenCalledTimes(3);
    expect(useUIStore.getState().viewedPubky).toBe("third");
  });

  it("should strip 'pubky' prefix before storing and persisting", async () => {
    vi.mocked(services.setLastPubky).mockResolvedValue(undefined);

    const { result } = renderHook(() => useSetViewedPubky());

    await act(async () => {
      await result.current.setViewedPubky("pubkyabc123def456");
    });

    // Should strip the "pubky" prefix
    const state = useUIStore.getState();
    expect(state.viewedPubky).toBe("abc123def456");
    expect(services.setLastPubky).toHaveBeenCalledWith("abc123def456");
  });

  it("should not modify pubky without prefix", async () => {
    vi.mocked(services.setLastPubky).mockResolvedValue(undefined);

    const { result } = renderHook(() => useSetViewedPubky());

    await act(async () => {
      await result.current.setViewedPubky("abc123def456");
    });

    const state = useUIStore.getState();
    expect(state.viewedPubky).toBe("abc123def456");
    expect(services.setLastPubky).toHaveBeenCalledWith("abc123def456");
  });
});
