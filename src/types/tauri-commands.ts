/**
 * Type-safe wrappers for Tauri commands
 * All commands verified from src-tauri/src/lib.rs
 */

import { invoke } from "@tauri-apps/api/core";
import type { AppState } from "./app-state";
import { isAppState } from "./app-state";
import type { BackendError } from "./backend-errors";
import { isBackendError } from "./backend-errors";

/**
 * Initialize the app state with a pubky string
 * @throws {BackendError} If initialization fails
 */
export async function initAppState(pubkyStr: string): Promise<void> {
  try {
    await invoke("init_app_state", { pubkyStr });
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
 * Get list of previously used pubky keys
 * @throws {BackendError} If fetching keys fails
 */
export async function getPreviousPubkyKeys(): Promise<string[]> {
  try {
    return await invoke<string[]>("get_previous_pubky_keys");
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
 * Begin the backup controller
 * @throws {BackendError} If starting backup controller fails
 */
export async function backupControllerBegin(): Promise<void> {
  try {
    await invoke("backup_controller_begin");
  } catch (error: unknown) {
    throw normalizeError(error);
  }
}

/**
 * Close the backup controller
 * @throws {BackendError} If closing backup controller fails
 */
export async function backupControllerClose(): Promise<void> {
  try {
    await invoke("backup_controller_close");
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
