import { useState, useEffect } from "react";
import { formatCountdown } from "@/utils/format";

const COUNTDOWN_UPDATE_INTERVAL_MS = 1000;

/**
 * useCountdown
 *
 * Hook for displaying countdown to next sync.
 * Updates every second while not syncing to show time remaining.
 *
 * @param nextSyncTime - Unix timestamp of the next scheduled sync
 * @param isSyncing - Whether a sync is currently in progress
 * @returns Formatted countdown text string
 *
 * @example
 * ```tsx
 * const { data: appState } = useAppState();
 * const countdownText = useCountdown(
 *   appState?.next_sync_time ?? 0,
 *   appState?.is_syncing ?? false
 * );
 *
 * return <p>{countdownText}</p>;
 * // Output: "Next backup in 2 minutes..." or "Syncing data..."
 * ```
 */
export function useCountdown(nextSyncTime: number, isSyncing: boolean) {
  const [countdownText, setCountdownText] = useState(() =>
    formatCountdown(nextSyncTime),
  );

  useEffect(() => {
    if (isSyncing) {
      setCountdownText("Syncing data...");
      return;
    }

    // Update immediately
    setCountdownText(formatCountdown(nextSyncTime));

    // Then update every second
    const interval = setInterval(() => {
      setCountdownText(formatCountdown(nextSyncTime));
    }, COUNTDOWN_UPDATE_INTERVAL_MS);

    return () => clearInterval(interval);
  }, [nextSyncTime, isSyncing]);

  return countdownText;
}
