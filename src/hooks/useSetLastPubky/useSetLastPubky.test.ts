import { describe, it, expect, beforeEach, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useSetLastPubky } from "./useSetLastPubky";
import { useUIStore } from "@/stores/uiStore";
import * as services from "@/services";

vi.mock("@/services", () => ({
  setLastPubky: vi.fn(),
}));

describe("useSetLastPubky", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useUIStore.setState({
      lastPubky: null,
      keyStates: {},
    });
  });

  it("should return setLastPubky function and isPending state", () => {
    const { result } = renderHook(() => useSetLastPubky());

    expect(result.current.setLastPubky).toBeInstanceOf(Function);
    expect(result.current.isPending).toBe(false);
  });

  it("should set isPending to true while setting pubky", async () => {
    let resolvePromise: () => void;
    const mockPromise = new Promise<void>((resolve) => {
      resolvePromise = resolve;
    });
    vi.mocked(services.setLastPubky).mockReturnValue(mockPromise);

    const { result } = renderHook(() => useSetLastPubky());

    expect(result.current.isPending).toBe(false);

    let setPromise: Promise<void>;
    act(() => {
      setPromise = result.current.setLastPubky("new-pubky");
    });

    expect(result.current.isPending).toBe(true);

    await act(async () => {
      resolvePromise!();
      await setPromise;
    });

    expect(result.current.isPending).toBe(false);
  });

  it("should update lastPubky in store immediately", async () => {
    vi.mocked(services.setLastPubky).mockResolvedValue(undefined);

    const { result } = renderHook(() => useSetLastPubky());

    await act(async () => {
      await result.current.setLastPubky("my-pubky");
    });

    const state = useUIStore.getState();
    expect(state.lastPubky).toBe("my-pubky");
  });

  it("should call setLastPubky service for persistence", async () => {
    vi.mocked(services.setLastPubky).mockResolvedValue(undefined);

    const { result } = renderHook(() => useSetLastPubky());

    await act(async () => {
      await result.current.setLastPubky("persist-pubky");
    });

    expect(services.setLastPubky).toHaveBeenCalledWith("persist-pubky");
    expect(services.setLastPubky).toHaveBeenCalledTimes(1);
  });

  it("should set isPending to false on error", async () => {
    vi.mocked(services.setLastPubky).mockRejectedValue(
      new Error("Failed to persist"),
    );

    const { result } = renderHook(() => useSetLastPubky());

    await act(async () => {
      try {
        await result.current.setLastPubky("bad-pubky");
      } catch {
        // Expected to throw
      }
    });

    expect(result.current.isPending).toBe(false);
  });

  it("should still update store even if persistence fails", async () => {
    vi.mocked(services.setLastPubky).mockRejectedValue(new Error("Failed"));

    const { result } = renderHook(() => useSetLastPubky());

    await act(async () => {
      try {
        await result.current.setLastPubky("my-pubky");
      } catch {
        // Expected to throw
      }
    });

    // Store was updated before the service call
    const state = useUIStore.getState();
    expect(state.lastPubky).toBe("my-pubky");
  });

  it("should propagate errors from service", async () => {
    const error = new Error("Persistence failed");
    vi.mocked(services.setLastPubky).mockRejectedValue(error);

    const { result } = renderHook(() => useSetLastPubky());

    await expect(
      act(async () => {
        await result.current.setLastPubky("bad-pubky");
      }),
    ).rejects.toThrow("Persistence failed");
  });

  it("should update lastPubky when switching keys", async () => {
    vi.mocked(services.setLastPubky).mockResolvedValue(undefined);
    useUIStore.setState({ lastPubky: "first-pubky" });

    const { result } = renderHook(() => useSetLastPubky());

    await act(async () => {
      await result.current.setLastPubky("second-pubky");
    });

    const state = useUIStore.getState();
    expect(state.lastPubky).toBe("second-pubky");
  });

  it("should handle multiple rapid calls", async () => {
    vi.mocked(services.setLastPubky).mockResolvedValue(undefined);

    const { result } = renderHook(() => useSetLastPubky());

    await act(async () => {
      await result.current.setLastPubky("first");
      await result.current.setLastPubky("second");
      await result.current.setLastPubky("third");
    });

    expect(services.setLastPubky).toHaveBeenCalledTimes(3);
    expect(useUIStore.getState().lastPubky).toBe("third");
  });

  it("should strip 'pubky' prefix before storing and persisting", async () => {
    vi.mocked(services.setLastPubky).mockResolvedValue(undefined);

    const { result } = renderHook(() => useSetLastPubky());

    await act(async () => {
      await result.current.setLastPubky("pubkyabc123def456");
    });

    // Should strip the "pubky" prefix
    const state = useUIStore.getState();
    expect(state.lastPubky).toBe("abc123def456");
    expect(services.setLastPubky).toHaveBeenCalledWith("abc123def456");
  });

  it("should not modify pubky without prefix", async () => {
    vi.mocked(services.setLastPubky).mockResolvedValue(undefined);

    const { result } = renderHook(() => useSetLastPubky());

    await act(async () => {
      await result.current.setLastPubky("abc123def456");
    });

    const state = useUIStore.getState();
    expect(state.lastPubky).toBe("abc123def456");
    expect(services.setLastPubky).toHaveBeenCalledWith("abc123def456");
  });
});
