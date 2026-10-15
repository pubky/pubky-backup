/**
 * TypeScript representation of Rust BackupAppError enum
 * Must match src-tauri/src/error.rs
 */

export type BackendError =
  | { type: "Internal"; message: string }
  | { type: "HomeserverNotFound"; message: string }
  | { type: "InvalidPubkyFormat"; message: string };

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

  const validTypes = ["Internal", "HomeserverNotFound", "InvalidPubkyFormat"];

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
