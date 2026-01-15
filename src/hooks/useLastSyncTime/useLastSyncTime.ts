import { useState, useEffect, useRef } from "react";

/**
 * useLastSyncTime
 *
 * Hook to derive lastSyncTime from nextSyncTime changes.
 * Detects when a sync completes by watching nextSyncTime get updated to a future timestamp.
 *
 * @param nextSyncTime - Unix timestamp of the next scheduled sync
 * @param isSyncing - Whether a sync is currently in progress
 * @returns Unix timestamp of the last completed sync, or null if no sync has completed
 *
 * @example
 * ```tsx
 * const { data: appState } = useAppState();
 * const lastSyncTime = useLastSyncTime(
 *   appState?.next_sync_time ?? 0,
 *   appState?.is_syncing ?? false
 * );
 *
 * return (
 *   <InfoCard
 *     icon={<ClockIcon />}
 *     label="Last Sync"
 *     value={lastSyncTime !== null ? formatTimestamp(lastSyncTime) : '--'}
 *   />
 * );
 * ```
 */
export function useLastSyncTime(nextSyncTime: number, isSyncing: boolean) {
  const [lastSyncTime, setLastSyncTime] = useState<number | null>(null);
  const prevNextSyncTime = useRef(nextSyncTime);

  useEffect(() => {
    const now = Math.floor(Date.now() / 1000);
    // Sync just completed when:
    // - Not currently syncing
    // - nextSyncTime increased (new future timestamp)
    // - nextSyncTime is in the future
    if (
      !isSyncing &&
      nextSyncTime > prevNextSyncTime.current &&
      nextSyncTime > now
    ) {
      setLastSyncTime(now);
    }
    prevNextSyncTime.current = nextSyncTime;
  }, [nextSyncTime, isSyncing]);

  return lastSyncTime;
}
