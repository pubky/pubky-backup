import { describe, it, expect, beforeEach, vi, type Mock } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useKeyUpdates } from "./useKeyUpdates";
import { useUIStore } from "@/stores/uiStore";
import type { KeyState, KeyUpdate } from "@/stores/uiStore";

// Mock the Tauri event API
const mockUnlisten = vi.fn();
let eventCallback: ((event: { payload: KeyUpdate }) => void) | null = null;

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((eventName: string, callback: (event: { payload: KeyUpdate }) => void) => {
    eventCallback = callback;
    return Promise.resolve(mockUnlisten);
  }),
}));

import { listen } from "@tauri-apps/api/event";

describe("useKeyUpdates", () => {
  const mockKeyState: KeyState = {
    status: { type: "Idle" },
    data_size: 1024,
    last_sync: 1672531200,
    next_sync: 1672531230,
    error: null,
    total_files: null,
    files_synced: null,
    bytes_downloaded: null,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    eventCallback = null;
    useUIStore.setState({
      keyStates: {},
      viewedPubky: null,
    });
  });

  it("should set up event listener on mount", async () => {
    renderHook(() => useKeyUpdates());

    // Wait for the listen promise to resolve
    await vi.waitFor(() => {
      expect(listen).toHaveBeenCalledTimes(1);
    });

    expect(listen).toHaveBeenCalledWith("key-update", expect.any(Function));
  });

  it("should update store when key-update event is received", async () => {
    renderHook(() => useKeyUpdates());

    // Wait for listener to be set up
    await vi.waitFor(() => {
      expect(eventCallback).not.toBeNull();
    });

    // Simulate receiving an event
    act(() => {
      eventCallback!({
        payload: {
          pubky: "pk:test-key",
          state: mockKeyState,
        },
      });
    });

    const state = useUIStore.getState();
    expect(state.keyStates["pk:test-key"]).toEqual(mockKeyState);
  });

  it("should handle multiple key updates", async () => {
    renderHook(() => useKeyUpdates());

    await vi.waitFor(() => {
      expect(eventCallback).not.toBeNull();
    });

    const syncingState: KeyState = {
      ...mockKeyState,
      status: { type: "Syncing", events_processed: 5 },
    };

    // Simulate multiple events
    act(() => {
      eventCallback!({
        payload: { pubky: "pk:key1", state: mockKeyState },
      });
      eventCallback!({
        payload: { pubky: "pk:key2", state: syncingState },
      });
    });

    const state = useUIStore.getState();
    expect(state.keyStates["pk:key1"]).toEqual(mockKeyState);
    expect(state.keyStates["pk:key2"]).toEqual(syncingState);
  });

  it("should update existing key state", async () => {
    // Pre-populate a key state
    useUIStore.setState({
      keyStates: { "pk:existing": mockKeyState },
    });

    renderHook(() => useKeyUpdates());

    await vi.waitFor(() => {
      expect(eventCallback).not.toBeNull();
    });

    const updatedState: KeyState = {
      ...mockKeyState,
      data_size: 2048,
      status: { type: "Syncing", events_processed: 10 },
    };

    act(() => {
      eventCallback!({
        payload: { pubky: "pk:existing", state: updatedState },
      });
    });

    const state = useUIStore.getState();
    expect(state.keyStates["pk:existing"]).toEqual(updatedState);
  });

  it("should call unlisten on unmount", async () => {
    const { unmount } = renderHook(() => useKeyUpdates());

    // Wait for listener to be set up
    await vi.waitFor(() => {
      expect(listen).toHaveBeenCalled();
    });

    unmount();

    expect(mockUnlisten).toHaveBeenCalled();
  });

  it("should handle error states in updates", async () => {
    renderHook(() => useKeyUpdates());

    await vi.waitFor(() => {
      expect(eventCallback).not.toBeNull();
    });

    const errorState: KeyState = {
      status: { type: "Error" },
      data_size: 0,
      last_sync: null,
      next_sync: null,
      error: {
        code: "NetworkError",
        message: "Connection failed",
        recoverable: true,
      },
      total_files: null,
      files_synced: null,
      bytes_downloaded: null,
    };

    act(() => {
      eventCallback!({
        payload: { pubky: "pk:error-key", state: errorState },
      });
    });

    const state = useUIStore.getState();
    expect(state.keyStates["pk:error-key"].status.type).toBe("Error");
    expect(state.keyStates["pk:error-key"].error?.message).toBe("Connection failed");
  });

  it("should not create duplicate listeners on rerender", async () => {
    const { rerender } = renderHook(() => useKeyUpdates());

    await vi.waitFor(() => {
      expect(listen).toHaveBeenCalledTimes(1);
    });

    rerender();
    rerender();
    rerender();

    // Should still only have one listener
    expect(listen).toHaveBeenCalledTimes(1);
  });
});
