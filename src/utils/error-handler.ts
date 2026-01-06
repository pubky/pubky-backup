/**
 * Handles discriminated union errors from Rust backend
 */

import { isBackendError } from "@/types/backend-errors";
import { getElementById } from "@/types/dom-helpers";

/**
 * Handle backend errors with type-safe discriminated union
 * @param error - The error from the backend (unknown type for safety)
 */
export function handleBackendError(error: unknown): void {
  if (!isBackendError(error)) {
    alert(`Error: ${String(error)}`);
    return;
  }

  // Handle discriminated union errors from Rust
  switch (error.type) {
    case "InvalidPubkyFormat":
      alert(
        `Invalid Format: ${error.message || "Please check your pubky format"}`,
      );
      // Highlight the input field if it exists
      const pubkyInput = getElementById<HTMLInputElement>("pubky-input");
      if (pubkyInput !== null) {
        pubkyInput.focus();
      }
      break;

    case "HomeserverNotFound":
      alert(
        `Homeserver Not Found: ${error.message || "Could not connect to your homeserver. Please check your pubky."}`,
      );
      break;

    case "DataNotFound":
      alert(
        `No Data Found: ${error.message || "No backup data exists for this pubky yet."}`,
      );
      break;

    case "Internal":
    case "Storage":
    case "Events":
    case "Backup":
      alert(`Error: ${error.message || "An error occurred"}`);
      break;

    default: {
      // Exhaustive check - this should never be reached
      const exhaustiveCheck: never = error;
      alert(`Error: ${JSON.stringify(exhaustiveCheck)}`);
      console.error("Unexpected error type:", exhaustiveCheck);
    }
  }
}
