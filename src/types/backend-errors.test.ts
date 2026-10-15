import { describe, it, expect } from "vitest";
import {
  isBackendError,
  isInternalError,
  isHomeserverNotFoundError,
  isInvalidPubkyFormatError,
  type BackendError,
} from "./backend-errors";

describe("isBackendError", () => {
  it("should accept valid backend errors", () => {
    const validErrors: BackendError[] = [
      { type: "Internal", message: "Internal error occurred" },
      { type: "HomeserverNotFound", message: "Homeserver not found" },
      { type: "InvalidPubkyFormat", message: "Invalid pubky format" },
    ];

    validErrors.forEach((error) => {
      expect(isBackendError(error)).toBe(true);
    });
  });

  it("should reject invalid error types", () => {
    const invalidErrors = [
      { type: "UnknownError", message: "Some message" },
      { type: "Random", message: "Random error" },
      { type: "", message: "Empty type" },
    ];

    invalidErrors.forEach((error) => {
      expect(isBackendError(error)).toBe(false);
    });
  });

  it("should reject non-object values", () => {
    expect(isBackendError(null)).toBe(false);
    expect(isBackendError(undefined)).toBe(false);
    expect(isBackendError("string error")).toBe(false);
    expect(isBackendError(42)).toBe(false);
    expect(isBackendError(true)).toBe(false);
    expect(isBackendError([])).toBe(false);
  });

  it("should reject objects missing required fields", () => {
    expect(isBackendError({ type: "Internal" })).toBe(false);
    expect(isBackendError({ message: "Some message" })).toBe(false);
    expect(isBackendError({})).toBe(false);
  });

  it("should reject objects with wrong field types", () => {
    expect(isBackendError({ type: 123, message: "message" })).toBe(false);
    expect(isBackendError({ type: "Internal", message: 123 })).toBe(false);
    expect(isBackendError({ type: null, message: "message" })).toBe(false);
    expect(isBackendError({ type: "Internal", message: null })).toBe(false);
  });

  it("should accept errors with additional fields", () => {
    const errorWithExtra = {
      type: "Internal",
      message: "Error message",
      extraField: "extra",
    };
    expect(isBackendError(errorWithExtra)).toBe(true);
  });
});

describe("specific type guards", () => {
  const internalError: BackendError = {
    type: "Internal",
    message: "Internal error",
  };
  const homeserverError: BackendError = {
    type: "HomeserverNotFound",
    message: "Homeserver not found",
  };
  const invalidPubkyError: BackendError = {
    type: "InvalidPubkyFormat",
    message: "Invalid pubky",
  };

  describe("isInternalError", () => {
    it("should correctly identify Internal errors", () => {
      expect(isInternalError(internalError)).toBe(true);
    });

    it("should reject non-Internal errors", () => {
      expect(isInternalError(homeserverError)).toBe(false);
      expect(isInternalError(invalidPubkyError)).toBe(false);
    });
  });

  describe("isHomeserverNotFoundError", () => {
    it("should correctly identify HomeserverNotFound errors", () => {
      expect(isHomeserverNotFoundError(homeserverError)).toBe(true);
    });

    it("should reject non-HomeserverNotFound errors", () => {
      expect(isHomeserverNotFoundError(internalError)).toBe(false);
      expect(isHomeserverNotFoundError(invalidPubkyError)).toBe(false);
    });
  });

  describe("isInvalidPubkyFormatError", () => {
    it("should correctly identify InvalidPubkyFormat errors", () => {
      expect(isInvalidPubkyFormatError(invalidPubkyError)).toBe(true);
    });

    it("should reject non-InvalidPubkyFormat errors", () => {
      expect(isInvalidPubkyFormatError(internalError)).toBe(false);
      expect(isInvalidPubkyFormatError(homeserverError)).toBe(false);
    });
  });
});
