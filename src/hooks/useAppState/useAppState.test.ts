import { describe, it, expect, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useAppState } from "./useAppState";
import { useUIStore } from "@/stores/uiStore";
import type { KeyState } from "@/stores/uiStore";

describe("useAppState", () => {
  beforeEach(() => {
    // Reset the Zustand store before each test
    useUIStore.setState({
      keyStates: {},
      viewedPubky: null,
      developerMode: false,
    });
  });

  it("should return empty state when no viewed pubky", () => {
    const { result } = renderHook(() => useAppState());

    expect(result.current.pubky).toBeNull();
    expect(result.current.is_syncing).toBe(false);
    expect(result.current.data_dir_size).toBe(0);
    expect(result.current.keyState).toBeNull();
  });

  it("should return state for viewed pubky", () => {
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

    act(() => {
      useUIStore.setState({
        viewedPubky: "pk:test123",
        keyStates: { "pk:test123": mockKeyState },
        developerMode: false,
      });
    });

    const { result } = renderHook(() => useAppState());

    expect(result.current.pubky).toBe("pk:test123");
    expect(result.current.is_syncing).toBe(false);
    expect(result.current.data_dir_size).toBe(1024);
    expect(result.current.next_sync_time).toBe(1672531230);
    expect(result.current.keyState).toEqual(mockKeyState);
  });

  it("should return is_syncing true when syncing", () => {
    const mockKeyState: KeyState = {
      status: { type: "Syncing", events_processed: 5 },
      data_size: 1024,
      last_sync: null,
      next_sync: null,
      error: null,
      total_files: null,
      files_synced: null,
      bytes_downloaded: null,
    };

    act(() => {
      useUIStore.setState({
        viewedPubky: "pk:test123",
        keyStates: { "pk:test123": mockKeyState },
        developerMode: false,
      });
    });

    const { result } = renderHook(() => useAppState());

    expect(result.current.is_syncing).toBe(true);
    expect(result.current.backup_running).toBe(true);
  });

  it("should return error message when in error state", () => {
    const mockKeyState: KeyState = {
      status: { type: "Error" },
      data_size: 0,
      last_sync: null,
      next_sync: null,
      error: {
        code: "Internal",
        message: "Connection failed",
        recoverable: false,
      },
      total_files: null,
      files_synced: null,
      bytes_downloaded: null,
    };

    act(() => {
      useUIStore.setState({
        viewedPubky: "pk:test123",
        keyStates: { "pk:test123": mockKeyState },
        developerMode: false,
      });
    });

    const { result } = renderHook(() => useAppState());

    expect(result.current.backup_controller_error).toBe("Connection failed");
    expect(result.current.backup_running).toBe(false);
  });

  it("should reflect developer_mode from store", () => {
    act(() => {
      useUIStore.setState({
        viewedPubky: null,
        keyStates: {},
        developerMode: true,
      });
    });

    const { result } = renderHook(() => useAppState());

    expect(result.current.developer_mode).toBe(true);
  });
});
