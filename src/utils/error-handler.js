/**
 * Handles discriminated union errors from Rust backend
 * @param {Object|string} error - The error object or string from the backend
 */
export function handleBackendError(error) {
  // Handle discriminated union errors from Rust
  if (error && typeof error === 'object' && error.type) {
    switch (error.type) {
      case 'InvalidPubkyFormat':
        alert(`Invalid Format: ${error.message || 'Please check your pubky format'}`);
        // Highlight the input field if it exists
        const pubkyInput = document.getElementById("pubky-input");
        if (pubkyInput) {
          pubkyInput.focus();
        }
        break;

      case 'HomeserverNotFound':
        alert(`Homeserver Not Found: ${error.message || 'Could not connect to your homeserver. Please check your pubky.'}`);
        break;

      case 'DataNotFound':
        alert(`No Data Found: ${error.message || 'No backup data exists for this pubky yet.'}`);
        break;

      case 'Internal':
      case 'Storage':
      case 'Events':
      case 'Backup':
        alert(`Error: ${error.message || 'An error occurred'}`);
        break;

      default:
        alert(`Error: ${JSON.stringify(error)}`);
        console.error("Unexpected error type:", error);
    }
  } else {
    // Fallback for non-structured errors (strings)
    alert(`Error: ${error}`);
  }
}
