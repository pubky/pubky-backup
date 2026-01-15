import { useState, useEffect, useRef } from "react";

/**
 * Hook to derive lastSyncTime from nextSyncTime changes
 * Detects when a sync completes by watching nextSyncTime get updated to a future timestamp
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
