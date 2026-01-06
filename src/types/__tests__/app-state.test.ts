import { describe, it, expect } from "vitest";
import { isAppState, type AppState } from "../app-state";

describe("isAppState", () => {
  it("should accept valid AppState objects", () => {
    const validState: AppState = {
      pubky: "pk:abc123def456",
      homeserver: "https://example.com",
      developer_mode: false,
      is_syncing: true,
      next_sync_time: 1672531200,
      data_dir_size: 1024000,
      backup_controller_error: null,
    };

    expect(isAppState(validState)).toBe(true);
  });

  it("should accept nullable pubky field", () => {
    const stateWithNullPubky = {
      pubky: null,
      homeserver: "https://example.com",
      developer_mode: false,
      is_syncing: false,
      next_sync_time: 1672531200,
      data_dir_size: 0,
      backup_controller_error: null,
    };

    expect(isAppState(stateWithNullPubky)).toBe(true);
  });

  it("should accept nullable homeserver field", () => {
    const stateWithNullHomeserver = {
      pubky: "pk:abc123def456",
      homeserver: null,
      developer_mode: true,
      is_syncing: false,
      next_sync_time: 1672531200,
      data_dir_size: 500,
      backup_controller_error: null,
    };

    expect(isAppState(stateWithNullHomeserver)).toBe(true);
  });

  it("should accept nullable backup_controller_error field", () => {
    const stateWithNullError = {
      pubky: "pk:abc123def456",
      homeserver: "https://example.com",
      developer_mode: false,
      is_syncing: true,
      next_sync_time: 1672531200,
      data_dir_size: 1024,
      backup_controller_error: null,
    };

    expect(isAppState(stateWithNullError)).toBe(true);
  });

  it("should accept string backup_controller_error field", () => {
    const stateWithError = {
      pubky: "pk:abc123def456",
      homeserver: "https://example.com",
      developer_mode: false,
      is_syncing: false,
      next_sync_time: 1672531200,
      data_dir_size: 1024,
      backup_controller_error: "Connection failed",
    };

    expect(isAppState(stateWithError)).toBe(true);
  });

  it("should accept all nullable fields as null", () => {
    const minimalState = {
      pubky: null,
      homeserver: null,
      developer_mode: false,
      is_syncing: false,
      next_sync_time: 0,
      data_dir_size: 0,
      backup_controller_error: null,
    };

    expect(isAppState(minimalState)).toBe(true);
  });

  it("should reject wrong field types - developer_mode as string", () => {
    const invalidState = {
      pubky: "pk:abc123def456",
      homeserver: "https://example.com",
      developer_mode: "true",
      is_syncing: true,
      next_sync_time: 1672531200,
      data_dir_size: 1024,
      backup_controller_error: null,
    };

    expect(isAppState(invalidState)).toBe(false);
  });

  it("should reject wrong field types - next_sync_time as string", () => {
    const invalidState = {
      pubky: "pk:abc123def456",
      homeserver: "https://example.com",
      developer_mode: false,
      is_syncing: true,
      next_sync_time: "1672531200",
      data_dir_size: 1024,
      backup_controller_error: null,
    };

    expect(isAppState(invalidState)).toBe(false);
  });

  it("should reject wrong field types - data_dir_size as string", () => {
    const invalidState = {
      pubky: "pk:abc123def456",
      homeserver: "https://example.com",
      developer_mode: false,
      is_syncing: true,
      next_sync_time: 1672531200,
      data_dir_size: "1024",
      backup_controller_error: null,
    };

    expect(isAppState(invalidState)).toBe(false);
  });

  it("should reject wrong field types - is_syncing as number", () => {
    const invalidState = {
      pubky: "pk:abc123def456",
      homeserver: "https://example.com",
      developer_mode: false,
      is_syncing: 1,
      next_sync_time: 1672531200,
      data_dir_size: 1024,
      backup_controller_error: null,
    };

    expect(isAppState(invalidState)).toBe(false);
  });

  it("should reject wrong field types - pubky as number", () => {
    const invalidState = {
      pubky: 123,
      homeserver: "https://example.com",
      developer_mode: false,
      is_syncing: true,
      next_sync_time: 1672531200,
      data_dir_size: 1024,
      backup_controller_error: null,
    };

    expect(isAppState(invalidState)).toBe(false);
  });

  it("should reject wrong field types - homeserver as boolean", () => {
    const invalidState = {
      pubky: "pk:abc123def456",
      homeserver: true,
      developer_mode: false,
      is_syncing: true,
      next_sync_time: 1672531200,
      data_dir_size: 1024,
      backup_controller_error: null,
    };

    expect(isAppState(invalidState)).toBe(false);
  });

  it("should reject wrong field types - backup_controller_error as number", () => {
    const invalidState = {
      pubky: "pk:abc123def456",
      homeserver: "https://example.com",
      developer_mode: false,
      is_syncing: true,
      next_sync_time: 1672531200,
      data_dir_size: 1024,
      backup_controller_error: 404,
    };

    expect(isAppState(invalidState)).toBe(false);
  });

  it("should reject missing required fields - no pubky", () => {
    const incompleteState = {
      homeserver: "https://example.com",
      developer_mode: false,
      is_syncing: true,
      next_sync_time: 1672531200,
      data_dir_size: 1024,
      backup_controller_error: null,
    };

    expect(isAppState(incompleteState)).toBe(false);
  });

  it("should reject missing required fields - no developer_mode", () => {
    const incompleteState = {
      pubky: "pk:abc123def456",
      homeserver: "https://example.com",
      is_syncing: true,
      next_sync_time: 1672531200,
      data_dir_size: 1024,
      backup_controller_error: null,
    };

    expect(isAppState(incompleteState)).toBe(false);
  });

  it("should reject missing required fields - no next_sync_time", () => {
    const incompleteState = {
      pubky: "pk:abc123def456",
      homeserver: "https://example.com",
      developer_mode: false,
      is_syncing: true,
      data_dir_size: 1024,
      backup_controller_error: null,
    };

    expect(isAppState(incompleteState)).toBe(false);
  });

  it("should reject non-object values", () => {
    expect(isAppState(null)).toBe(false);
    expect(isAppState(undefined)).toBe(false);
    expect(isAppState("string")).toBe(false);
    expect(isAppState(123)).toBe(false);
    expect(isAppState(true)).toBe(false);
    expect(isAppState([])).toBe(false);
  });

  it("should reject empty objects", () => {
    expect(isAppState({})).toBe(false);
  });

  it("should accept objects with extra fields", () => {
    const stateWithExtra = {
      pubky: "pk:abc123def456",
      homeserver: "https://example.com",
      developer_mode: false,
      is_syncing: true,
      next_sync_time: 1672531200,
      data_dir_size: 1024,
      backup_controller_error: null,
      extraField: "this is extra",
    };

    expect(isAppState(stateWithExtra)).toBe(true);
  });
});
