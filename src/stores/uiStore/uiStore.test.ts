import { describe, it, expect, beforeEach } from "vitest";
import { useUIStore } from "./uiStore.store";
import type { KeyState } from "./uiStore.types";

const mockKeyState: KeyState = {
  status: { type: "Idle" },
  data_size: 1024,
  last_sync: 1672531200,
  next_sync: 1672531230,
  error: null,
  total_files: null,
  files_synced: null,
  bytes_downloaded: null,
};

const mockSyncingKeyState: KeyState = {
  status: { type: "Syncing", events_processed: 5 },
  data_size: 2048,
  last_sync: null,
  next_sync: null,
  error: null,
  total_files: 100,
  files_synced: 50,
  bytes_downloaded: 1024,
};

describe("useUIStore", () => {
  beforeEach(() => {
    // Reset store to initial state before each test
    useUIStore.setState({
      currentScreen: "startup",
      currentPage: "sync",
      statusMessageMode: "sync",
      pubkyInputValue: "",
      hasAutoLoaded: false,
      toast: {
        visible: false,
        pubkyText: "",
        type: "success",
        message: "",
      },
      keyStates: {},
      lastPubky: null,
      developerMode: false,
    });
  });

  describe("initial state", () => {
    it("should have startup as default screen", () => {
      const state = useUIStore.getState();
      expect(state.currentScreen).toBe("startup");
    });

    it("should have sync as default status message mode", () => {
      const state = useUIStore.getState();
      expect(state.statusMessageMode).toBe("sync");
    });

    it("should have empty pubky input value", () => {
      const state = useUIStore.getState();
      expect(state.pubkyInputValue).toBe("");
    });

    it("should have toast hidden by default", () => {
      const state = useUIStore.getState();
      expect(state.toast.visible).toBe(false);
      expect(state.toast.pubkyText).toBe("");
      expect(state.toast.type).toBe("success");
      expect(state.toast.message).toBe("");
    });

    it("should have hasAutoLoaded as false by default", () => {
      const state = useUIStore.getState();
      expect(state.hasAutoLoaded).toBe(false);
    });

    it("should have sync as default page", () => {
      const state = useUIStore.getState();
      expect(state.currentPage).toBe("sync");
    });
  });

  describe("setScreen", () => {
    it("should change screen to dashboard", () => {
      useUIStore.getState().setScreen("dashboard");
      expect(useUIStore.getState().currentScreen).toBe("dashboard");
    });

    it("should change screen back to startup", () => {
      useUIStore.getState().setScreen("dashboard");
      useUIStore.getState().setScreen("startup");
      expect(useUIStore.getState().currentScreen).toBe("startup");
    });
  });

  describe("setPage", () => {
    it("should change page to activity", () => {
      useUIStore.getState().setPage("activity");
      expect(useUIStore.getState().currentPage).toBe("activity");
    });

    it("should change page to keys", () => {
      useUIStore.getState().setPage("keys");
      expect(useUIStore.getState().currentPage).toBe("keys");
    });

    it("should change page to settings", () => {
      useUIStore.getState().setPage("settings");
      expect(useUIStore.getState().currentPage).toBe("settings");
    });

    it("should change page back to sync", () => {
      useUIStore.getState().setPage("settings");
      useUIStore.getState().setPage("sync");
      expect(useUIStore.getState().currentPage).toBe("sync");
    });
  });

  describe("setStatusMessageMode", () => {
    it("should change status message mode to snapshot-success", () => {
      useUIStore.getState().setStatusMessageMode("snapshot-success");
      expect(useUIStore.getState().statusMessageMode).toBe("snapshot-success");
    });

    it("should change status message mode to snapshot-error", () => {
      useUIStore.getState().setStatusMessageMode("snapshot-error");
      expect(useUIStore.getState().statusMessageMode).toBe("snapshot-error");
    });

    it("should change status message mode back to sync", () => {
      useUIStore.getState().setStatusMessageMode("snapshot-success");
      useUIStore.getState().setStatusMessageMode("sync");
      expect(useUIStore.getState().statusMessageMode).toBe("sync");
    });
  });

  describe("setPubkyInputValue", () => {
    it("should update pubky input value", () => {
      useUIStore.getState().setPubkyInputValue("test-pubky-value");
      expect(useUIStore.getState().pubkyInputValue).toBe("test-pubky-value");
    });

    it("should handle empty string", () => {
      useUIStore.getState().setPubkyInputValue("some-value");
      useUIStore.getState().setPubkyInputValue("");
      expect(useUIStore.getState().pubkyInputValue).toBe("");
    });

    it("should handle long pubky strings", () => {
      const longPubky = "a".repeat(100);
      useUIStore.getState().setPubkyInputValue(longPubky);
      expect(useUIStore.getState().pubkyInputValue).toBe(longPubky);
    });
  });

  describe("setHasAutoLoaded", () => {
    it("should update hasAutoLoaded to true", () => {
      useUIStore.getState().setHasAutoLoaded(true);
      expect(useUIStore.getState().hasAutoLoaded).toBe(true);
    });

    it("should update hasAutoLoaded back to false", () => {
      useUIStore.getState().setHasAutoLoaded(true);
      useUIStore.getState().setHasAutoLoaded(false);
      expect(useUIStore.getState().hasAutoLoaded).toBe(false);
    });
  });

  describe("showToast", () => {
    it("should show toast with pubky text", () => {
      useUIStore.getState().showToast("my-pubky-12345");

      const state = useUIStore.getState();
      expect(state.toast.visible).toBe(true);
      expect(state.toast.pubkyText).toBe("my-pubky-12345");
      expect(state.toast.type).toBe("success");
      expect(state.toast.message).toBe("Pubky copied to clipboard");
    });

    it("should update toast text when called multiple times", () => {
      useUIStore.getState().showToast("first-pubky");
      useUIStore.getState().showToast("second-pubky");

      const state = useUIStore.getState();
      expect(state.toast.visible).toBe(true);
      expect(state.toast.pubkyText).toBe("second-pubky");
    });
  });

  describe("showErrorToast", () => {
    it("should show error toast with message", () => {
      useUIStore.getState().showErrorToast("Something went wrong");

      const state = useUIStore.getState();
      expect(state.toast.visible).toBe(true);
      expect(state.toast.type).toBe("error");
      expect(state.toast.message).toBe("Something went wrong");
      expect(state.toast.pubkyText).toBe("");
    });

    it("should update error message when called multiple times", () => {
      useUIStore.getState().showErrorToast("First error");
      useUIStore.getState().showErrorToast("Second error");

      const state = useUIStore.getState();
      expect(state.toast.visible).toBe(true);
      expect(state.toast.message).toBe("Second error");
    });
  });

  describe("hideToast", () => {
    it("should hide toast while preserving pubky text", () => {
      useUIStore.getState().showToast("my-pubky");
      useUIStore.getState().hideToast();

      const state = useUIStore.getState();
      expect(state.toast.visible).toBe(false);
      expect(state.toast.pubkyText).toBe("my-pubky");
    });

    it("should be safe to call when already hidden", () => {
      useUIStore.getState().hideToast();

      const state = useUIStore.getState();
      expect(state.toast.visible).toBe(false);
    });
  });

  describe("state independence", () => {
    it("should not affect other state when changing screen", () => {
      useUIStore.getState().setPubkyInputValue("test-value");
      useUIStore.getState().showToast("toast-text");

      useUIStore.getState().setScreen("dashboard");

      const state = useUIStore.getState();
      expect(state.pubkyInputValue).toBe("test-value");
      expect(state.toast.visible).toBe(true);
      expect(state.toast.pubkyText).toBe("toast-text");
    });

    it("should not affect other state when changing status mode", () => {
      useUIStore.getState().setScreen("dashboard");
      useUIStore.getState().setPubkyInputValue("test-value");

      useUIStore.getState().setStatusMessageMode("snapshot-success");

      const state = useUIStore.getState();
      expect(state.currentScreen).toBe("dashboard");
      expect(state.pubkyInputValue).toBe("test-value");
    });
  });

  describe("setKeyState", () => {
    it("should add a new key state", () => {
      useUIStore.getState().setKeyState("pk:key1", mockKeyState);

      const state = useUIStore.getState();
      expect(state.keyStates["pk:key1"]).toEqual(mockKeyState);
    });

    it("should update an existing key state", () => {
      useUIStore.getState().setKeyState("pk:key1", mockKeyState);
      useUIStore.getState().setKeyState("pk:key1", mockSyncingKeyState);

      const state = useUIStore.getState();
      expect(state.keyStates["pk:key1"]).toEqual(mockSyncingKeyState);
    });

    it("should not affect other key states", () => {
      useUIStore.getState().setKeyState("pk:key1", mockKeyState);
      useUIStore.getState().setKeyState("pk:key2", mockSyncingKeyState);

      const state = useUIStore.getState();
      expect(state.keyStates["pk:key1"]).toEqual(mockKeyState);
      expect(state.keyStates["pk:key2"]).toEqual(mockSyncingKeyState);
    });

    it("should handle multiple keys", () => {
      useUIStore.getState().setKeyState("pk:key1", mockKeyState);
      useUIStore.getState().setKeyState("pk:key2", mockKeyState);
      useUIStore.getState().setKeyState("pk:key3", mockSyncingKeyState);

      const state = useUIStore.getState();
      expect(Object.keys(state.keyStates)).toHaveLength(3);
    });
  });

  describe("setAllKeyStates", () => {
    it("should replace all key states", () => {
      useUIStore.getState().setKeyState("pk:existing", mockKeyState);

      useUIStore.getState().setAllKeyStates({
        "pk:new1": mockKeyState,
        "pk:new2": mockSyncingKeyState,
      });

      const state = useUIStore.getState();
      expect(state.keyStates["pk:existing"]).toBeUndefined();
      expect(state.keyStates["pk:new1"]).toEqual(mockKeyState);
      expect(state.keyStates["pk:new2"]).toEqual(mockSyncingKeyState);
    });

    it("should handle empty object", () => {
      useUIStore.getState().setKeyState("pk:key1", mockKeyState);
      useUIStore.getState().setAllKeyStates({});

      const state = useUIStore.getState();
      expect(state.keyStates).toEqual({});
    });

    it("should not affect other state", () => {
      useUIStore.getState().setLastPubky("pk:viewed");
      useUIStore.getState().setDeveloperMode(true);

      useUIStore.getState().setAllKeyStates({
        "pk:key1": mockKeyState,
      });

      const state = useUIStore.getState();
      expect(state.lastPubky).toBe("pk:viewed");
      expect(state.developerMode).toBe(true);
    });
  });

  describe("removeKeyState", () => {
    it("should remove a key state", () => {
      useUIStore.getState().setKeyState("pk:key1", mockKeyState);
      useUIStore.getState().setKeyState("pk:key2", mockSyncingKeyState);

      useUIStore.getState().removeKeyState("pk:key1");

      const state = useUIStore.getState();
      expect(state.keyStates["pk:key1"]).toBeUndefined();
      expect(state.keyStates["pk:key2"]).toEqual(mockSyncingKeyState);
    });

    it("should handle removing non-existent key", () => {
      useUIStore.getState().setKeyState("pk:key1", mockKeyState);

      useUIStore.getState().removeKeyState("pk:nonexistent");

      const state = useUIStore.getState();
      expect(state.keyStates["pk:key1"]).toEqual(mockKeyState);
    });

    it("should handle removing from empty state", () => {
      useUIStore.getState().removeKeyState("pk:key1");

      const state = useUIStore.getState();
      expect(state.keyStates).toEqual({});
    });
  });

  describe("setLastPubky", () => {
    it("should set viewed pubky", () => {
      useUIStore.getState().setLastPubky("pk:mykey");

      const state = useUIStore.getState();
      expect(state.lastPubky).toBe("pk:mykey");
    });

    it("should update viewed pubky", () => {
      useUIStore.getState().setLastPubky("pk:first");
      useUIStore.getState().setLastPubky("pk:second");

      const state = useUIStore.getState();
      expect(state.lastPubky).toBe("pk:second");
    });

    it("should clear viewed pubky with null", () => {
      useUIStore.getState().setLastPubky("pk:mykey");
      useUIStore.getState().setLastPubky(null);

      const state = useUIStore.getState();
      expect(state.lastPubky).toBeNull();
    });

    it("should not affect key states", () => {
      useUIStore.getState().setKeyState("pk:key1", mockKeyState);
      useUIStore.getState().setLastPubky("pk:key1");

      const state = useUIStore.getState();
      expect(state.keyStates["pk:key1"]).toEqual(mockKeyState);
    });
  });

  describe("setDeveloperMode", () => {
    it("should enable developer mode", () => {
      useUIStore.getState().setDeveloperMode(true);

      const state = useUIStore.getState();
      expect(state.developerMode).toBe(true);
    });

    it("should disable developer mode", () => {
      useUIStore.getState().setDeveloperMode(true);
      useUIStore.getState().setDeveloperMode(false);

      const state = useUIStore.getState();
      expect(state.developerMode).toBe(false);
    });

    it("should not affect other state", () => {
      useUIStore.getState().setLastPubky("pk:mykey");
      useUIStore.getState().setKeyState("pk:key1", mockKeyState);

      useUIStore.getState().setDeveloperMode(true);

      const state = useUIStore.getState();
      expect(state.lastPubky).toBe("pk:mykey");
      expect(state.keyStates["pk:key1"]).toEqual(mockKeyState);
    });
  });

  describe("initial key state values", () => {
    it("should have empty keyStates by default", () => {
      const state = useUIStore.getState();
      expect(state.keyStates).toEqual({});
    });

    it("should have null lastPubky by default", () => {
      const state = useUIStore.getState();
      expect(state.lastPubky).toBeNull();
    });

    it("should have developerMode false by default", () => {
      const state = useUIStore.getState();
      expect(state.developerMode).toBe(false);
    });
  });
});
