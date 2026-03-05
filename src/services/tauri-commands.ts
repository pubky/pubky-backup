/**
 * Type-safe wrappers for Tauri commands
 * All commands verified from src-tauri/src/lib.rs
 */

import { invoke } from "@tauri-apps/api/core";
import type { AppState } from "@/types/app-state";
import { isAppState } from "@/types/app-state";
import type { BackendError } from "@/types/backend-errors";
import { isBackendError } from "@/types/backend-errors";

/**
 * Add a key to start backing up
 * @throws {BackendError} If adding key fails
 */
export async function addKey(pubkyStr: string): Promise<void> {
  try {
    await invoke<void>("add_key", { pubkyStr });
  } catch (error: unknown) {
    throw normalizeError(error);
  }
}

/**
 * Fetch the current app state
 * @throws {BackendError} If fetching state fails
 */
export async function fetchState(): Promise<AppState> {
  try {
    const result = await invoke<AppState>("fetch_state");
    if (!isAppState(result)) {
      throw new Error("Invalid AppState received from backend");
    }
    return result;
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
 * Set which pubky is currently being viewed in the UI
 * @throws {BackendError} If setting viewed pubky fails
 */
export async function setViewedPubky(pubkyStr: string): Promise<void> {
  try {
    await invoke("set_viewed_pubky", { pubkyStr });
  } catch (error: unknown) {
    throw normalizeError(error);
  }
}


/**
 * Remove a key from the backup manager, stopping its backup controller
 * @param pubky - The pubky key to remove
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
 * Force a sync to happen now
 * @throws {BackendError} If forcing sync fails
 */
export async function forceSyncNow(): Promise<void> {
  try {
    await invoke("force_sync_now");
  } catch (error: unknown) {
    throw normalizeError(error);
  }
}

/**
 * Get the data directory path
 * @throws {BackendError} If getting path fails
 */
export async function getDataDirPath(): Promise<string> {
  try {
    return await invoke<string>("get_data_dir_path");
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
 * Create a snapshot (zip archive) of the current pubky's backed-up data
 * @returns Path to the created snapshot file
 * @throws {BackendError} If creating snapshot fails
 */
export async function createSnapshot(): Promise<string> {
  try {
    return await invoke<string>("create_snapshot");
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
