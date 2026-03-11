import { describe, it, expect, beforeEach, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useAddKey } from "./useAddKey";
import { useUIStore } from "@/stores/uiStore";
import * as services from "@/services";

vi.mock("@/services", () => ({
  addKey: vi.fn(),
}));

describe("useAddKey", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useUIStore.setState({
      lastPubky: null,
      keyStates: {},
    });
  });

  it("should return addKey function and isPending state", () => {
    const { result } = renderHook(() => useAddKey());

    expect(result.current.addKey).toBeInstanceOf(Function);
    expect(result.current.isPending).toBe(false);
  });

  it("should set isPending to true while adding key", async () => {
    let resolvePromise: (value: string) => void;
    const mockPromise = new Promise<string>((resolve) => {
      resolvePromise = resolve;
    });
    vi.mocked(services.addKey).mockReturnValue(mockPromise);

    const { result } = renderHook(() => useAddKey());

    expect(result.current.isPending).toBe(false);

    let addKeyPromise: Promise<string>;
    act(() => {
      addKeyPromise = result.current.addKey({ pubkyValue: "test-pubky" });
    });

    expect(result.current.isPending).toBe(true);

    await act(async () => {
      resolvePromise!("normalized-pubky");
      await addKeyPromise;
    });

    expect(result.current.isPending).toBe(false);
  });

  it("should call addKey service with pubky value", async () => {
    vi.mocked(services.addKey).mockResolvedValue("normalized-pubky");

    const { result } = renderHook(() => useAddKey());

    await act(async () => {
      await result.current.addKey({ pubkyValue: "my-pubky-input" });
    });

    expect(services.addKey).toHaveBeenCalledWith("my-pubky-input");
    expect(services.addKey).toHaveBeenCalledTimes(1);
  });

  it("should return normalized pubky from service", async () => {
    vi.mocked(services.addKey).mockResolvedValue("normalized-pubky-123");

    const { result } = renderHook(() => useAddKey());

    let returnedPubky: string;
    await act(async () => {
      returnedPubky = await result.current.addKey({
        pubkyValue: "input-pubky",
      });
    });

    expect(returnedPubky!).toBe("normalized-pubky-123");
  });

  it("should update lastPubky in store after successful add", async () => {
    vi.mocked(services.addKey).mockResolvedValue("normalized-pubky");

    const { result } = renderHook(() => useAddKey());

    await act(async () => {
      await result.current.addKey({ pubkyValue: "my-pubky" });
    });

    const state = useUIStore.getState();
    expect(state.lastPubky).toBe("normalized-pubky");
  });

  it("should set isPending to false on error", async () => {
    vi.mocked(services.addKey).mockRejectedValue(
      new Error("Failed to add key"),
    );

    const { result } = renderHook(() => useAddKey());

    await act(async () => {
      try {
        await result.current.addKey({ pubkyValue: "bad-pubky" });
      } catch {
        // Expected to throw
      }
    });

    expect(result.current.isPending).toBe(false);
  });

  it("should propagate errors from service", async () => {
    const error = new Error("Invalid pubky format");
    vi.mocked(services.addKey).mockRejectedValue(error);

    const { result } = renderHook(() => useAddKey());

    await expect(
      act(async () => {
        await result.current.addKey({ pubkyValue: "invalid" });
      }),
    ).rejects.toThrow("Invalid pubky format");
  });

  it("should not update lastPubky on error", async () => {
    useUIStore.setState({ lastPubky: "existing-pubky" });
    vi.mocked(services.addKey).mockRejectedValue(new Error("Failed"));

    const { result } = renderHook(() => useAddKey());

    await act(async () => {
      try {
        await result.current.addKey({ pubkyValue: "bad-pubky" });
      } catch {
        // Expected to throw
      }
    });

    const state = useUIStore.getState();
    expect(state.lastPubky).toBe("existing-pubky");
  });

  it("should handle sequential calls correctly", async () => {
    vi.mocked(services.addKey)
      .mockResolvedValueOnce("first-normalized")
      .mockResolvedValueOnce("second-normalized");

    const { result } = renderHook(() => useAddKey());

    await act(async () => {
      await result.current.addKey({ pubkyValue: "first" });
    });

    expect(result.current.isPending).toBe(false);
    expect(useUIStore.getState().lastPubky).toBe("first-normalized");

    await act(async () => {
      await result.current.addKey({ pubkyValue: "second" });
    });

    expect(result.current.isPending).toBe(false);
    expect(useUIStore.getState().lastPubky).toBe("second-normalized");
    expect(services.addKey).toHaveBeenCalledTimes(2);
  });
});
