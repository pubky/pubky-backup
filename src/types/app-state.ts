/**
 * TypeScript representation of Rust AppState struct
 * Must match src-tauri/src/lib.rs:61-77
 */
export interface AppState {
  /** This session's pubky */
  pubky: string | null;
  /** This session's pubky's homeserver */
  homeserver: string | null;
  /** Developer mode for working on the front-end */
  developer_mode: boolean;
  /** Current sync status */
  is_syncing: boolean;
  /** Next sync time in seconds since epoch (for countdown display) */
  next_sync_time: number;
  /** Size of data stored for current pubky in bytes */
  data_dir_size: number;
  /** Error message if backup controller failed, null if running normally */
  backup_controller_error: string | null;
}

/**
 * Type guard to check if an unknown value is an AppState
 */
export function isAppState(value: unknown): value is AppState {
  if (typeof value !== "object" || value === null) {
    return false;
  }

  const state = value as Record<string, unknown>;

  return (
    (typeof state.pubky === "string" || state.pubky === null) &&
    (typeof state.homeserver === "string" || state.homeserver === null) &&
    typeof state.developer_mode === "boolean" &&
    typeof state.is_syncing === "boolean" &&
    typeof state.next_sync_time === "number" &&
    typeof state.data_dir_size === "number" &&
    (typeof state.backup_controller_error === "string" ||
      state.backup_controller_error === null)
  );
}
