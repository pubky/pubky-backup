/**
 * Type-safe wrappers for Tauri commands
 * All commands verified from src-tauri/src/lib.rs
 */

import { invoke } from "@tauri-apps/api/core";
import type { ActivityEntry, KeyState } from "@/stores/uiStore";
import type { BackendError } from "@/types/backend-errors";
import { isBackendError } from "@/types/backend-errors";

/**
 * App config returned from backend
 */
export interface AppConfig {
  developer_mode: boolean;
  sync_interval_secs: number;
  keys_dir: string;
}

/**
 * Add a key to start backing up
 * @returns The pubky string (normalized) that was added
 * @throws {BackendError} If adding key fails
 */
export async function addKey(pubkyStr: string): Promise<string> {
  try {
    return await invoke<string>("add_key", { pubkyStr });
  } catch (error: unknown) {
    throw normalizeError(error);
  }
}

/**
 * Get all key states for all tracked keys
 * @throws {BackendError} If fetching state fails
 */
export async function getAllKeyStates(): Promise<Record<string, KeyState>> {
  try {
    return await invoke<Record<string, KeyState>>("get_all_key_states");
  } catch (error: unknown) {
    throw normalizeError(error);
  }
}

/**
 * Get application config (one-time fetch)
 */
export async function getConfig(): Promise<AppConfig> {
  try {
    return await invoke<AppConfig>("get_config");
  } catch (error: unknown) {
    throw normalizeError(error);
  }
}

/**
 * Get list of pubky keys that have data stored
 * @throws {BackendError} If fetching keys fails
 */
export async function getKeys(): Promise<string[]> {
  try {
    return await invoke<string[]>("get_keys");
  } catch (error: unknown) {
    throw normalizeError(error);
  }
}

/**
 * Get the last used pubky key
 * @throws {BackendError} If fetching last pubky fails
 */
export async function getLastPubky(): Promise<string | null> {
  try {
    return await invoke<string | null>("get_last_pubky");
  } catch (error: unknown) {
    throw normalizeError(error);
  }
}

/**
 * Set the last used pubky (for restoring on next app launch)
 * @throws {BackendError} If setting last pubky fails
 */
export async function setLastPubky(pubkyStr: string): Promise<void> {
  try {
    await invoke("set_last_pubky", { pubkyStr });
  } catch (error: unknown) {
    throw normalizeError(error);
  }
}

/**
 * Remove a key from the backup manager, stopping its backup controller
 * @param pubkyStr - The pubky key to remove
 * @throws {BackendError} If removing the key fails
 */
export async function removeKey(pubkyStr: string): Promise<void> {
  try {
    await invoke("remove_key", { pubkyStr });
  } catch (error: unknown) {
    throw normalizeError(error);
  }
}

/**
 * Delete a key from the backup manager and remove all backed-up data
 * @param pubkyStr - The pubky key to delete
 * @throws {BackendError} If deleting the key fails
 */
export async function deleteKey(pubkyStr: string): Promise<void> {
  try {
    await invoke("delete_key", { pubkyStr });
  } catch (error: unknown) {
    throw normalizeError(error);
  }
}

/**
 * Force a sync to happen now for a specific pubky
 * @param pubkyStr - The pubky key to force sync
 * @throws {BackendError} If forcing sync fails
 */
export async function forceSyncNow(pubkyStr: string): Promise<void> {
  try {
    await invoke("force_sync_now", { pubkyStr });
  } catch (error: unknown) {
    throw normalizeError(error);
  }
}

/**
 * Open the data directory in the system file browser
 * @throws {BackendError} If opening directory fails
 */
export async function openDataDir(): Promise<void> {
  try {
    await invoke("open_data_dir");
  } catch (error: unknown) {
    throw normalizeError(error);
  }
}

/**
 * Create a snapshot (zip archive) of a pubky's backed-up data
 * @param pubkyStr - The pubky key to create snapshot for
 * @returns Path to the created snapshot file
 * @throws {BackendError} If creating snapshot fails
 */
export async function createSnapshot(pubkyStr: string): Promise<string> {
  try {
    return await invoke<string>("create_snapshot", { pubkyStr });
  } catch (error: unknown) {
    throw normalizeError(error);
  }
}

/**
 * Set the sync interval in seconds. Controllers pick up the new value dynamically.
 * @param intervalSecs - The new sync interval in seconds
 * @throws {BackendError} If setting interval fails
 */
export async function setSyncInterval(intervalSecs: number): Promise<void> {
  try {
    await invoke("set_sync_interval", { intervalSecs });
  } catch (error: unknown) {
    throw normalizeError(error);
  }
}

/**
 * Move backup data to a new location.
 * Shuts down controllers, moves keys directory, then recreates the manager.
 * @param newParent - The new parent directory for the keys folder
 * @returns The new keys directory path
 * @throws {BackendError} If moving backup location fails
 */
export async function setBackupLocation(newParent: string): Promise<string> {
  try {
    return await invoke<string>("set_backup_location", { newParent });
  } catch (error: unknown) {
    throw normalizeError(error);
  }
}

/**
 * Get recent activity entries for a key
 * @param pubkyStr - The pubky key to get activity for
 * @returns Array of activity entries, newest first
 * @throws {BackendError} If fetching activity fails
 */
export async function getActivity(pubkyStr: string): Promise<ActivityEntry[]> {
  try {
    return await invoke<ActivityEntry[]>("get_activity", { pubkyStr });
  } catch (error: unknown) {
    throw normalizeError(error);
  }
}

/**
 * Normalize unknown errors to BackendError
 * If the error is already a BackendError, return it as-is
 * Otherwise, wrap it in an Internal error
 */
function normalizeError(error: unknown): BackendError {
  if (isBackendError(error)) {
    return error;
  }

  return {
    type: "Internal",
    message: error instanceof Error ? error.message : String(error),
  };
}
