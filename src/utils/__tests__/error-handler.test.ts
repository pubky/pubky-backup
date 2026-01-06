import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { handleBackendError } from "../error-handler";
import type { BackendError } from "@/types/backend-errors";

describe("handleBackendError", () => {
  let alertSpy: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    // Mock window.alert since it doesn't exist in happy-dom
    window.alert = vi.fn();
    alertSpy = window.alert as ReturnType<typeof vi.fn>;
  });

  afterEach(() => {
    vi.restoreAllMocks();
    document.body.innerHTML = "";
  });

  describe("InvalidPubkyFormat errors", () => {
    it("should handle InvalidPubkyFormat errors with custom message", () => {
      const error: BackendError = {
        type: "InvalidPubkyFormat",
        message: "Invalid pubky key format",
      };

      handleBackendError(error);

      expect(alertSpy).toHaveBeenCalledOnce();
      expect(alertSpy).toHaveBeenCalledWith(
        "Invalid Format: Invalid pubky key format",
      );
    });

    it("should use fallback message when message is empty", () => {
      const error: BackendError = {
        type: "InvalidPubkyFormat",
        message: "",
      };

      handleBackendError(error);

      expect(alertSpy).toHaveBeenCalledOnce();
      expect(alertSpy).toHaveBeenCalledWith(
        "Invalid Format: Please check your pubky format",
      );
    });

    it("should focus input element when it exists", () => {
      document.body.innerHTML = '<input id="pubky-input" type="text" />';
      const input = document.getElementById("pubky-input") as HTMLInputElement;
      const focusSpy = vi.spyOn(input, "focus");

      const error: BackendError = {
        type: "InvalidPubkyFormat",
        message: "Invalid format",
      };

      handleBackendError(error);

      expect(focusSpy).toHaveBeenCalledOnce();
      focusSpy.mockRestore();
    });

    it("should not throw when input element does not exist", () => {
      const error: BackendError = {
        type: "InvalidPubkyFormat",
        message: "Invalid format",
      };

      expect(() => handleBackendError(error)).not.toThrow();
      expect(alertSpy).toHaveBeenCalledOnce();
    });
  });

  describe("HomeserverNotFound errors", () => {
    it("should handle HomeserverNotFound errors with custom message", () => {
      const error: BackendError = {
        type: "HomeserverNotFound",
        message: "Could not reach homeserver at example.com",
      };

      handleBackendError(error);

      expect(alertSpy).toHaveBeenCalledOnce();
      expect(alertSpy).toHaveBeenCalledWith(
        "Homeserver Not Found: Could not reach homeserver at example.com",
      );
    });

    it("should use fallback message when message is empty", () => {
      const error: BackendError = {
        type: "HomeserverNotFound",
        message: "",
      };

      handleBackendError(error);

      expect(alertSpy).toHaveBeenCalledOnce();
      expect(alertSpy).toHaveBeenCalledWith(
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

      expect(alertSpy).toHaveBeenCalledOnce();
      expect(alertSpy).toHaveBeenCalledWith(
        "No Data Found: No backup found for this key",
      );
    });

    it("should use fallback message when message is empty", () => {
      const error: BackendError = {
        type: "DataNotFound",
        message: "",
      };

      handleBackendError(error);

      expect(alertSpy).toHaveBeenCalledOnce();
      expect(alertSpy).toHaveBeenCalledWith(
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

      expect(alertSpy).toHaveBeenCalledOnce();
      expect(alertSpy).toHaveBeenCalledWith(
        "Error: Database connection failed",
      );
    });

    it("should use fallback message when message is empty", () => {
      const error: BackendError = {
        type: "Internal",
        message: "",
      };

      handleBackendError(error);

      expect(alertSpy).toHaveBeenCalledOnce();
      expect(alertSpy).toHaveBeenCalledWith("Error: An error occurred");
    });
  });

  describe("Storage errors", () => {
    it("should handle Storage errors with custom message", () => {
      const error: BackendError = {
        type: "Storage",
        message: "Failed to write to disk",
      };

      handleBackendError(error);

      expect(alertSpy).toHaveBeenCalledOnce();
      expect(alertSpy).toHaveBeenCalledWith("Error: Failed to write to disk");
    });

    it("should use fallback message when message is empty", () => {
      const error: BackendError = {
        type: "Storage",
        message: "",
      };

      handleBackendError(error);

      expect(alertSpy).toHaveBeenCalledOnce();
      expect(alertSpy).toHaveBeenCalledWith("Error: An error occurred");
    });
  });

  describe("Events errors", () => {
    it("should handle Events errors with custom message", () => {
      const error: BackendError = {
        type: "Events",
        message: "Event processing failed",
      };

      handleBackendError(error);

      expect(alertSpy).toHaveBeenCalledOnce();
      expect(alertSpy).toHaveBeenCalledWith("Error: Event processing failed");
    });

    it("should use fallback message when message is empty", () => {
      const error: BackendError = {
        type: "Events",
        message: "",
      };

      handleBackendError(error);

      expect(alertSpy).toHaveBeenCalledOnce();
      expect(alertSpy).toHaveBeenCalledWith("Error: An error occurred");
    });
  });

  describe("Backup errors", () => {
    it("should handle Backup errors with custom message", () => {
      const error: BackendError = {
        type: "Backup",
        message: "Backup sync interrupted",
      };

      handleBackendError(error);

      expect(alertSpy).toHaveBeenCalledOnce();
      expect(alertSpy).toHaveBeenCalledWith("Error: Backup sync interrupted");
    });

    it("should use fallback message when message is empty", () => {
      const error: BackendError = {
        type: "Backup",
        message: "",
      };

      handleBackendError(error);

      expect(alertSpy).toHaveBeenCalledOnce();
      expect(alertSpy).toHaveBeenCalledWith("Error: An error occurred");
    });
  });

  describe("non-BackendError handling", () => {
    it("should handle string errors", () => {
      handleBackendError("Something went wrong");

      expect(alertSpy).toHaveBeenCalledOnce();
      expect(alertSpy).toHaveBeenCalledWith("Error: Something went wrong");
    });

    it("should handle Error objects", () => {
      const error = new Error("System error");
      handleBackendError(error);

      expect(alertSpy).toHaveBeenCalledOnce();
      expect(alertSpy).toHaveBeenCalledWith("Error: Error: System error");
    });

    it("should handle null", () => {
      handleBackendError(null);

      expect(alertSpy).toHaveBeenCalledOnce();
      expect(alertSpy).toHaveBeenCalledWith("Error: null");
    });

    it("should handle undefined", () => {
      handleBackendError(undefined);

      expect(alertSpy).toHaveBeenCalledOnce();
      expect(alertSpy).toHaveBeenCalledWith("Error: undefined");
    });

    it("should handle numbers", () => {
      handleBackendError(404);

      expect(alertSpy).toHaveBeenCalledOnce();
      expect(alertSpy).toHaveBeenCalledWith("Error: 404");
    });

    it("should handle objects without type field", () => {
      handleBackendError({ message: "Some error" });

      expect(alertSpy).toHaveBeenCalledOnce();
      // String() on an object returns "[object Object]"
      expect(alertSpy).toHaveBeenCalledWith("Error: [object Object]");
    });
  });
});
