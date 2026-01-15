import { describe, it, expect, beforeEach } from "vitest";
import { useUIStore } from "./uiStore.store";

describe("useUIStore", () => {
  beforeEach(() => {
    // Reset store to initial state before each test
    useUIStore.setState({
      currentScreen: "startup",
      statusMessageMode: "sync",
      pubkyInputValue: "",
      hasAutoLoaded: false,
      toast: {
        visible: false,
        pubkyText: "",
      },
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
    });

    it("should have hasAutoLoaded as false by default", () => {
      const state = useUIStore.getState();
      expect(state.hasAutoLoaded).toBe(false);
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
    });

    it("should update toast text when called multiple times", () => {
      useUIStore.getState().showToast("first-pubky");
      useUIStore.getState().showToast("second-pubky");

      const state = useUIStore.getState();
      expect(state.toast.visible).toBe(true);
      expect(state.toast.pubkyText).toBe("second-pubky");
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
});
