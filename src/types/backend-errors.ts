/**
 * TypeScript representation of Rust BackupAppError enum
 * Must match src-tauri/src/error.rs:6-21
 */

export type BackendError =
  | { type: "Internal"; message: string }
  | { type: "HomeserverNotFound"; message: string }
  | { type: "InvalidPubkyFormat"; message: string }
  | { type: "Storage"; message: string }
  | { type: "Events"; message: string }
  | { type: "Backup"; message: string };

/**
 * Type guard to check if an unknown error is a BackendError
 */
export function isBackendError(error: unknown): error is BackendError {
  if (typeof error !== "object" || error === null) {
    return false;
  }

  const err = error as Record<string, unknown>;

  if (typeof err.type !== "string" || typeof err.message !== "string") {
    return false;
  }

  const validTypes = [
    "Internal",
    "HomeserverNotFound",
    "InvalidPubkyFormat",
    "Storage",
    "Events",
    "Backup",
  ];

  return validTypes.includes(err.type);
}

/**
 * Specific type guards for each error variant
 */
export function isInternalError(
  error: BackendError,
): error is { type: "Internal"; message: string } {
  return error.type === "Internal";
}

export function isHomeserverNotFoundError(
  error: BackendError,
): error is { type: "HomeserverNotFound"; message: string } {
  return error.type === "HomeserverNotFound";
}

export function isInvalidPubkyFormatError(
  error: BackendError,
): error is { type: "InvalidPubkyFormat"; message: string } {
  return error.type === "InvalidPubkyFormat";
}

export function isStorageError(
  error: BackendError,
): error is { type: "Storage"; message: string } {
  return error.type === "Storage";
}

export function isEventsError(
  error: BackendError,
): error is { type: "Events"; message: string } {
  return error.type === "Events";
}

export function isBackupError(
  error: BackendError,
): error is { type: "Backup"; message: string } {
  return error.type === "Backup";
}
