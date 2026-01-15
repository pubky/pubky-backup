import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook } from "@testing-library/react";
import { useLastSyncTime } from "./useLastSyncTime";

describe("useLastSyncTime", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2024-01-15T12:00:00Z"));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("should return null initially", () => {
    const now = Math.floor(Date.now() / 1000);
    const { result } = renderHook(() => useLastSyncTime(now + 60, false));

    expect(result.current).toBeNull();
  });

  it("should persist null when nextSyncTime stays the same", () => {
    const now = Math.floor(Date.now() / 1000);
    const { result, rerender } = renderHook(
      ({ nextSyncTime, isSyncing }) => useLastSyncTime(nextSyncTime, isSyncing),
      { initialProps: { nextSyncTime: now + 60, isSyncing: false } },
    );

    expect(result.current).toBeNull();

    // Multiple rerenders with same props should keep null
    rerender({ nextSyncTime: now + 60, isSyncing: false });
    expect(result.current).toBeNull();

    rerender({ nextSyncTime: now + 60, isSyncing: false });
    expect(result.current).toBeNull();
  });

  it("should persist null when only isSyncing toggles without nextSyncTime change", () => {
    const now = Math.floor(Date.now() / 1000);
    const { result, rerender } = renderHook(
      ({ nextSyncTime, isSyncing }) => useLastSyncTime(nextSyncTime, isSyncing),
      { initialProps: { nextSyncTime: now + 60, isSyncing: false } },
    );

    expect(result.current).toBeNull();

    // Start syncing
    rerender({ nextSyncTime: now + 60, isSyncing: true });
    expect(result.current).toBeNull();

    // Stop syncing but nextSyncTime hasn't changed
    rerender({ nextSyncTime: now + 60, isSyncing: false });
    expect(result.current).toBeNull();
  });

  it("should not update lastSyncTime while syncing", () => {
    const now = Math.floor(Date.now() / 1000);
    const { result, rerender } = renderHook(
      ({ nextSyncTime, isSyncing }) => useLastSyncTime(nextSyncTime, isSyncing),
      { initialProps: { nextSyncTime: now + 60, isSyncing: true } },
    );

    expect(result.current).toBeNull();

    // Still syncing with same nextSyncTime
    rerender({ nextSyncTime: now + 60, isSyncing: true });

    expect(result.current).toBeNull();
  });

  it("should set lastSyncTime when sync completes (nextSyncTime increases)", () => {
    const now = Math.floor(Date.now() / 1000);
    const { result, rerender } = renderHook(
      ({ nextSyncTime, isSyncing }) => useLastSyncTime(nextSyncTime, isSyncing),
      { initialProps: { nextSyncTime: now + 60, isSyncing: true } },
    );

    expect(result.current).toBeNull();

    // Sync completes - nextSyncTime jumps to a new future time
    rerender({ nextSyncTime: now + 120, isSyncing: false });

    expect(result.current).toBe(now);
  });

  it("should not update if nextSyncTime decreases", () => {
    const now = Math.floor(Date.now() / 1000);
    const { result, rerender } = renderHook(
      ({ nextSyncTime, isSyncing }) => useLastSyncTime(nextSyncTime, isSyncing),
      { initialProps: { nextSyncTime: now + 120, isSyncing: false } },
    );

    expect(result.current).toBeNull();

    // nextSyncTime decreases (shouldn't happen normally)
    rerender({ nextSyncTime: now + 60, isSyncing: false });

    expect(result.current).toBeNull();
  });

  it("should not update if still syncing even when nextSyncTime increases", () => {
    const now = Math.floor(Date.now() / 1000);
    const { result, rerender } = renderHook(
      ({ nextSyncTime, isSyncing }) => useLastSyncTime(nextSyncTime, isSyncing),
      { initialProps: { nextSyncTime: now + 60, isSyncing: true } },
    );

    // nextSyncTime increases but still syncing
    rerender({ nextSyncTime: now + 120, isSyncing: true });

    expect(result.current).toBeNull();
  });

  it("should not update if nextSyncTime is in the past", () => {
    const now = Math.floor(Date.now() / 1000);
    const { result, rerender } = renderHook(
      ({ nextSyncTime, isSyncing }) => useLastSyncTime(nextSyncTime, isSyncing),
      { initialProps: { nextSyncTime: now - 60, isSyncing: true } },
    );

    // "Complete" sync but nextSyncTime is still in the past
    rerender({ nextSyncTime: now - 30, isSyncing: false });

    expect(result.current).toBeNull();
  });

  it("should preserve lastSyncTime across subsequent renders", () => {
    const now = Math.floor(Date.now() / 1000);
    const { result, rerender } = renderHook(
      ({ nextSyncTime, isSyncing }) => useLastSyncTime(nextSyncTime, isSyncing),
      { initialProps: { nextSyncTime: now + 60, isSyncing: true } },
    );

    // First sync completes
    rerender({ nextSyncTime: now + 120, isSyncing: false });
    const firstSyncTime = result.current;

    expect(firstSyncTime).toBe(now);

    // Some time passes, same props
    rerender({ nextSyncTime: now + 120, isSyncing: false });

    expect(result.current).toBe(firstSyncTime);
  });

  it("should update lastSyncTime on subsequent sync completions", () => {
    const now = Math.floor(Date.now() / 1000);
    const { result, rerender } = renderHook(
      ({ nextSyncTime, isSyncing }) => useLastSyncTime(nextSyncTime, isSyncing),
      { initialProps: { nextSyncTime: now + 60, isSyncing: true } },
    );

    // First sync completes
    rerender({ nextSyncTime: now + 120, isSyncing: false });
    expect(result.current).toBe(now);

    // Advance time
    vi.advanceTimersByTime(60000);
    const laterNow = Math.floor(Date.now() / 1000);

    // Second sync starts
    rerender({ nextSyncTime: now + 120, isSyncing: true });

    // Second sync completes with new nextSyncTime
    rerender({ nextSyncTime: laterNow + 120, isSyncing: false });

    expect(result.current).toBe(laterNow);
  });
});
