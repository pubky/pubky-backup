import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useCountdown } from "./useCountdown";

describe("useCountdown", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("should return 'Syncing data...' when isSyncing is true", () => {
    const now = Math.floor(Date.now() / 1000);
    const { result } = renderHook(() => useCountdown(now + 60, true));

    expect(result.current).toBe("Syncing data...");
  });

  it("should return countdown text when not syncing", () => {
    const now = Math.floor(Date.now() / 1000);
    const { result } = renderHook(() => useCountdown(now + 120, false));

    expect(result.current).toBe("Next backup in 2 minutes...");
  });

  it("should return seconds countdown when less than a minute", () => {
    const now = Math.floor(Date.now() / 1000);
    const { result } = renderHook(() => useCountdown(now + 45, false));

    expect(result.current).toBe("Next backup in 45 seconds...");
  });

  it("should return 'Syncing soon...' when nextSyncTime is in the past", () => {
    const now = Math.floor(Date.now() / 1000);
    const { result } = renderHook(() => useCountdown(now - 10, false));

    expect(result.current).toBe("Syncing soon...");
  });

  it("should update countdown every second", () => {
    const now = Math.floor(Date.now() / 1000);
    const { result } = renderHook(() => useCountdown(now + 65, false));

    expect(result.current).toBe("Next backup in 1 minute...");

    // Advance time by 6 seconds
    act(() => {
      vi.advanceTimersByTime(6000);
    });

    expect(result.current).toBe("Next backup in 59 seconds...");
  });

  it("should switch from syncing message to countdown when sync completes", () => {
    const now = Math.floor(Date.now() / 1000);
    const { result, rerender } = renderHook(
      ({ nextSyncTime, isSyncing }) => useCountdown(nextSyncTime, isSyncing),
      { initialProps: { nextSyncTime: now + 60, isSyncing: true } },
    );

    expect(result.current).toBe("Syncing data...");

    rerender({ nextSyncTime: now + 60, isSyncing: false });

    expect(result.current).toBe("Next backup in 1 minute...");
  });

  it("should update when nextSyncTime changes", () => {
    const now = Math.floor(Date.now() / 1000);
    const { result, rerender } = renderHook(
      ({ nextSyncTime, isSyncing }) => useCountdown(nextSyncTime, isSyncing),
      { initialProps: { nextSyncTime: now + 60, isSyncing: false } },
    );

    expect(result.current).toBe("Next backup in 1 minute...");

    rerender({ nextSyncTime: now + 300, isSyncing: false });

    expect(result.current).toBe("Next backup in 5 minutes...");
  });

  it("should clean up interval on unmount", () => {
    const clearIntervalSpy = vi.spyOn(global, "clearInterval");
    const now = Math.floor(Date.now() / 1000);

    const { unmount } = renderHook(() => useCountdown(now + 60, false));

    unmount();

    expect(clearIntervalSpy).toHaveBeenCalled();
    clearIntervalSpy.mockRestore();
  });

  it("should handle singular 'second' correctly", () => {
    const now = Math.floor(Date.now() / 1000);
    const { result } = renderHook(() => useCountdown(now + 1, false));

    expect(result.current).toBe("Next backup in 1 second...");
  });

  it("should handle singular 'minute' correctly", () => {
    const now = Math.floor(Date.now() / 1000);
    const { result } = renderHook(() => useCountdown(now + 60, false));

    expect(result.current).toBe("Next backup in 1 minute...");
  });
});
