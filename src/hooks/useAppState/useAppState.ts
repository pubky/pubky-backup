import { useShallow } from "zustand/react/shallow";
import { useUIStore, type KeyState } from "@/stores/uiStore";

/**
 * Derived state that matches the old AppState interface for easier migration.
 * Components can use this during transition, or access keyStates directly.
 */
export interface DerivedAppState {
  pubky: string | null;
  developer_mode: boolean;
  is_syncing: boolean;
  next_sync_time: number;
  last_sync_time: number | null;
  data_dir_size: number;
  backup_controller_error: string | null;
  backup_running: boolean;
  keyState: KeyState | null;
}

/**
 * Default next sync time (30 seconds from now)
 */
function defaultNextSyncTime(): number {
  return Math.floor(Date.now() / 1000) + 30;
}

/**
 * Derive AppState-like object from KeyState for the viewed pubky
 */
function deriveAppState(
  viewedPubky: string | null,
  keyStates: Record<string, KeyState>,
  developerMode: boolean,
): DerivedAppState {
  if (viewedPubky === null) {
    return {
      pubky: null,
      developer_mode: developerMode,
      is_syncing: false,
      next_sync_time: 0,
      last_sync_time: null,
      data_dir_size: 0,
      backup_controller_error: null,
      backup_running: false,
      keyState: null,
    };
  }

  const keyState = keyStates[viewedPubky];
  if (keyState === undefined) {
    // Key was just added but state hasn't arrived yet - show as syncing
    return {
      pubky: viewedPubky,
      developer_mode: developerMode,
      is_syncing: true,
      next_sync_time: 0,
      last_sync_time: null,
      data_dir_size: 0,
      backup_controller_error: null,
      backup_running: true,
      keyState: null,
    };
  }

  const isSyncing = keyState.status.type === "Syncing" || keyState.status.type === "Starting";
  const isError = keyState.status.type === "Error";
  const isStopped = keyState.status.type === "Stopped";

  return {
    pubky: viewedPubky,
    developer_mode: developerMode,
    is_syncing: isSyncing,
    next_sync_time: keyState.next_sync ?? defaultNextSyncTime(),
    last_sync_time: keyState.last_sync,
    data_dir_size: keyState.data_size,
    backup_controller_error: keyState.error?.message ?? null,
    backup_running: !isStopped && !isError,
    keyState,
  };
}

/**
 * useAppState
 *
 * Hook for accessing the current app state from the Zustand store.
 * Returns derived state for the currently viewed pubky.
 *
 * State is updated in real-time via Tauri events (no polling).
 *
 * @returns Derived app state for the viewed pubky
 *
 * @example
 * ```tsx
 * const appState = useAppState();
 *
 * return (
 *   <div>
 *     <p>Pubky: {appState.pubky}</p>
 *     <p>Syncing: {appState.is_syncing ? 'Yes' : 'No'}</p>
 *   </div>
 * );
 * ```
 */
export function useAppState(): DerivedAppState {
  // Use useShallow to ensure re-render when any property changes
  const { viewedPubky, keyState, developerMode } = useUIStore(
    useShallow((s) => ({
      viewedPubky: s.viewedPubky,
      keyState: s.viewedPubky !== null ? s.keyStates[s.viewedPubky] : undefined,
      developerMode: s.developerMode,
    })),
  );

  // Build keyStates with just the viewed key for deriveAppState
  const keyStates: Record<string, KeyState> =
    viewedPubky !== null && keyState !== undefined
      ? { [viewedPubky]: keyState }
      : {};

  return deriveAppState(viewedPubky, keyStates, developerMode);
}
