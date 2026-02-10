import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { handleBackendError } from "../error-handler";
import { useUIStore } from "@/stores";
import type { BackendError } from "@/types/backend-errors";

describe("handleBackendError", () => {
  const getLastErrorToast = () => useUIStore.getState().toast;

  beforeEach(() => {
    // Reset store to initial state
    useUIStore.setState({
      currentScreen: "startup",
      statusMessageMode: "sync",
      pubkyInputValue: "",
      hasAutoLoaded: false,
      toast: {
        visible: false,
        pubkyText: "",
        type: "success",
        message: "",
      },
    });
  });

  afterEach(() => {
    document.body.innerHTML = "";
  });

  describe("InvalidPubkyFormat errors", () => {
    it("should handle InvalidPubkyFormat errors with custom message", () => {
      const error: BackendError = {
        type: "InvalidPubkyFormat",
        message: "Invalid pubky key format",
      };

      handleBackendError(error);

      const toast = getLastErrorToast();
      expect(toast.visible).toBe(true);
      expect(toast.type).toBe("error");
      expect(toast.message).toBe("Invalid Format: Invalid pubky key format");
    });

    it("should use fallback message when message is empty", () => {
      const error: BackendError = {
        type: "InvalidPubkyFormat",
        message: "",
      };

      handleBackendError(error);

      const toast = getLastErrorToast();
      expect(toast.visible).toBe(true);
      expect(toast.message).toBe(
        "Invalid Format: Please check your pubky format",
      );
    });

    it("should not throw when handling InvalidPubkyFormat", () => {
      const error: BackendError = {
        type: "InvalidPubkyFormat",
        message: "Invalid format",
      };

      expect(() => handleBackendError(error)).not.toThrow();
      expect(getLastErrorToast().visible).toBe(true);
    });
  });

  describe("HomeserverNotFound errors", () => {
    it("should handle HomeserverNotFound errors with custom message", () => {
      const error: BackendError = {
        type: "HomeserverNotFound",
        message: "Could not reach homeserver at example.com",
      };

      handleBackendError(error);

      const toast = getLastErrorToast();
      expect(toast.visible).toBe(true);
      expect(toast.message).toBe(
        "Homeserver Not Found: Could not reach homeserver at example.com",
      );
    });

    it("should use fallback message when message is empty", () => {
      const error: BackendError = {
        type: "HomeserverNotFound",
        message: "",
      };

      handleBackendError(error);

      const toast = getLastErrorToast();
      expect(toast.visible).toBe(true);
      expect(toast.message).toBe(
        "Homeserver Not Found: Could not connect to your homeserver. Please check your pubky.",
      );
    });
  });

  describe("DataNotFound errors", () => {
    it("should handle DataNotFound errors with custom message", () => {
      const error: BackendError = {
        type: "DataNotFound",
        message: "No backup found for this key",
      };

      handleBackendError(error);

      const toast = getLastErrorToast();
      expect(toast.visible).toBe(true);
      expect(toast.message).toBe("No Data Found: No backup found for this key");
    });

    it("should use fallback message when message is empty", () => {
      const error: BackendError = {
        type: "DataNotFound",
        message: "",
      };

      handleBackendError(error);

      const toast = getLastErrorToast();
      expect(toast.visible).toBe(true);
      expect(toast.message).toBe(
        "No Data Found: No backup data exists for this pubky yet.",
      );
    });
  });

  describe("Internal errors", () => {
    it("should handle Internal errors with custom message", () => {
      const error: BackendError = {
        type: "Internal",
        message: "Database connection failed",
      };

      handleBackendError(error);

      const toast = getLastErrorToast();
      expect(toast.visible).toBe(true);
      expect(toast.message).toBe("Error: Database connection failed");
    });

    it("should use fallback message when message is empty", () => {
      const error: BackendError = {
        type: "Internal",
        message: "",
      };

      handleBackendError(error);

      const toast = getLastErrorToast();
      expect(toast.visible).toBe(true);
      expect(toast.message).toBe("Error: An error occurred");
    });
  });

  describe("Storage errors", () => {
    it("should handle Storage errors with custom message", () => {
      const error: BackendError = {
        type: "Storage",
        message: "Failed to write to disk",
      };

      handleBackendError(error);

      const toast = getLastErrorToast();
      expect(toast.visible).toBe(true);
      expect(toast.message).toBe("Error: Failed to write to disk");
    });

    it("should use fallback message when message is empty", () => {
      const error: BackendError = {
        type: "Storage",
        message: "",
      };

      handleBackendError(error);

      const toast = getLastErrorToast();
      expect(toast.visible).toBe(true);
      expect(toast.message).toBe("Error: An error occurred");
    });
  });

  describe("Events errors", () => {
    it("should handle Events errors with custom message", () => {
      const error: BackendError = {
        type: "Events",
        message: "Event processing failed",
      };

      handleBackendError(error);

      const toast = getLastErrorToast();
      expect(toast.visible).toBe(true);
      expect(toast.message).toBe("Error: Event processing failed");
    });

    it("should use fallback message when message is empty", () => {
      const error: BackendError = {
        type: "Events",
        message: "",
      };

      handleBackendError(error);

      const toast = getLastErrorToast();
      expect(toast.visible).toBe(true);
      expect(toast.message).toBe("Error: An error occurred");
    });
  });

  describe("Backup errors", () => {
    it("should handle Backup errors with custom message", () => {
      const error: BackendError = {
        type: "Backup",
        message: "Backup sync interrupted",
      };

      handleBackendError(error);

      const toast = getLastErrorToast();
      expect(toast.visible).toBe(true);
      expect(toast.message).toBe("Error: Backup sync interrupted");
    });

    it("should use fallback message when message is empty", () => {
      const error: BackendError = {
        type: "Backup",
        message: "",
      };

      handleBackendError(error);

      const toast = getLastErrorToast();
      expect(toast.visible).toBe(true);
      expect(toast.message).toBe("Error: An error occurred");
    });
  });

  describe("non-BackendError handling", () => {
    it("should handle string errors", () => {
      handleBackendError("Something went wrong");

      const toast = getLastErrorToast();
      expect(toast.visible).toBe(true);
      expect(toast.message).toBe("Error: Something went wrong");
    });

    it("should handle Error objects", () => {
      const error = new Error("System error");
      handleBackendError(error);

      const toast = getLastErrorToast();
      expect(toast.visible).toBe(true);
      expect(toast.message).toBe("Error: Error: System error");
    });

    it("should handle null", () => {
      handleBackendError(null);

      const toast = getLastErrorToast();
      expect(toast.visible).toBe(true);
      expect(toast.message).toBe("Error: null");
    });

    it("should handle undefined", () => {
      handleBackendError(undefined);

      const toast = getLastErrorToast();
      expect(toast.visible).toBe(true);
      expect(toast.message).toBe("Error: undefined");
    });

    it("should handle numbers", () => {
      handleBackendError(404);

      const toast = getLastErrorToast();
      expect(toast.visible).toBe(true);
      expect(toast.message).toBe("Error: 404");
    });

    it("should handle objects without type field", () => {
      handleBackendError({ message: "Some error" });

      const toast = getLastErrorToast();
      expect(toast.visible).toBe(true);
      // String() on an object returns "[object Object]"
      expect(toast.message).toBe("Error: [object Object]");
    });
  });
});
