import { useMemo } from "react";
import { useShallow } from "zustand/react/shallow";
import * as Stores from "@/stores";
import * as Hooks from "@/hooks";

/**
 * Status information for the dashboard display
 */
export interface StatusInfo {
  badge: "snapshot" | "syncing" | "synced";
  message: "snapshot-success" | "error" | "syncing" | "synced";
  text: string;
}

/**
 * State returned by useDashboardState hook
 */
export interface DashboardState {
  pubky: string | null;
  isSyncing: boolean;
  nextSyncTime: number | null;
  dataSize: number;
  lastSyncTime: number | null;
  countdownText: string;
  dataDirPath: string | null;
  keys: string[];
  statusInfo: StatusInfo;
}

/**
 * useDashboardState
 *
 * Hook that consolidates all dashboard state into a single interface.
 * Derives state from the UI store, app state, and various utility hooks.
 *
 * @returns Consolidated dashboard state
 *
 * @example
 * ```tsx
 * const { pubky, isSyncing, statusInfo, lastSyncTime } = useDashboardState();
 *
 * return (
 *   <div>
 *     <StatusBadge status={statusInfo.badge} />
 *     <span>Last sync: {formatTimestamp(lastSyncTime)}</span>
 *   </div>
 * );
 * ```
 */
export function useDashboardState(): DashboardState {
  const { statusMessageMode } = Stores.useUIStore(
    useShallow((s) => ({ statusMessageMode: s.statusMessageMode })),
  );

  const appState = Hooks.useAppState();
  const { dataDirPath } = Hooks.useDataDirPath();
  const keys = Hooks.useKeys();

  const pubky = appState.pubky;
  const isSyncing = appState.is_syncing;
  const nextSyncTime = appState.next_sync_time;
  const dataSize = appState.data_dir_size;

  const lastSyncTime = Hooks.useLastSyncTime(nextSyncTime, isSyncing);
  const countdownText = Hooks.useCountdown(nextSyncTime, isSyncing);

  // Consolidated status information derived from state
  const statusInfo = useMemo((): StatusInfo => {
    if (statusMessageMode === "snapshot-success") {
      return {
        badge: "snapshot",
        message: "snapshot-success",
        text: "Snapshot created",
      };
    }
    if (statusMessageMode === "snapshot-error") {
      return {
        badge: "synced",
        message: "error",
        text: "Failed to create snapshot",
      };
    }
    if (isSyncing) {
      return {
        badge: "syncing",
        message: "syncing",
        text: "Syncing data...",
      };
    }
    return {
      badge: "synced",
      message: "synced",
      text: countdownText,
    };
  }, [statusMessageMode, isSyncing, countdownText]);

  return {
    pubky,
    isSyncing,
    nextSyncTime,
    dataSize,
    lastSyncTime,
    countdownText,
    dataDirPath,
    keys,
    statusInfo,
  };
}
