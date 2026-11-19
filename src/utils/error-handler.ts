/**
 * Handles discriminated union errors from Rust backend
 */

import { isBackendError } from "@/types/backend-errors";
import { useUIStore } from "@/stores/uiStore/uiStore.store";

/**
 * Shows an error message using the toast system
 */
function showError(message: string): void {
  useUIStore.getState().showErrorToast(message);
}

/**
 * Handle backend errors with type-safe discriminated union
 * Shows an error toast with the error message
 * @param error - The error from the backend (unknown type for safety)
 */
export function handleBackendError(error: unknown): void {
  if (!isBackendError(error)) {
    showError(`Error: ${String(error)}`);
    return;
  }

  // Handle discriminated union errors from Rust
  switch (error.type) {
    case "InvalidPubkyFormat":
      // The backend's message comes from the key decoder and means nothing to users
      showError("That isn't a valid pubky.");
      break;

    case "HomeserverNotFound":
      showError(error.message || "Please check your pubky.");
      break;

    case "Internal":
      showError(`Error: ${error.message || "An error occurred"}`);
      break;

    default: {
      // Exhaustive check - this should never be reached
      const exhaustiveCheck: never = error;
      showError(`Error: ${JSON.stringify(exhaustiveCheck)}`);
      console.error("Unexpected error type:", exhaustiveCheck);
    }
  }
}
