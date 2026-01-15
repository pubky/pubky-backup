import { useState, useEffect } from "react";
import { formatCountdown } from "@/utils/format";

const COUNTDOWN_UPDATE_INTERVAL_MS = 1000;

/**
 * Hook for displaying countdown to next sync
 * Updates every second while not syncing
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
