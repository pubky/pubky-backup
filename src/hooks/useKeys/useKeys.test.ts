import { describe, it, expect, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useKeys } from "./useKeys";
import { useUIStore } from "@/stores/uiStore";
import type { KeyState } from "@/stores/uiStore";

describe("useKeys", () => {
  const mockKeyState: KeyState = {
    status: { type: "Idle" },
    data_size: 1024,
    last_sync: null,
    next_sync: null,
    error: null,
    total_files: null,
    files_synced: null,
    bytes_downloaded: null,
  };

  beforeEach(() => {
    // Reset the Zustand store before each test
    useUIStore.setState({
      keyStates: {},
      viewedPubky: null,
      developerMode: false,
    });
  });

  it("should return keys from keyStates", () => {
    act(() => {
      useUIStore.setState({
        keyStates: {
          "pk:key1": mockKeyState,
          "pk:key2": mockKeyState,
          "pk:key3": mockKeyState,
        },
      });
    });

    const { result } = renderHook(() => useKeys());

    expect(result.current).toHaveLength(3);
    expect(result.current).toContain("pk:key1");
    expect(result.current).toContain("pk:key2");
    expect(result.current).toContain("pk:key3");
  });

  it("should return empty array when no keys", () => {
    const { result } = renderHook(() => useKeys());

    expect(result.current).toEqual([]);
    expect(result.current).toHaveLength(0);
  });

  it("should update when keyStates changes", () => {
    const { result, rerender } = renderHook(() => useKeys());

    expect(result.current).toHaveLength(0);

    act(() => {
      useUIStore.setState({
        keyStates: {
          "pk:newKey": mockKeyState,
        },
      });
    });

    rerender();

    expect(result.current).toHaveLength(1);
    expect(result.current).toContain("pk:newKey");
  });
});
