import { create } from "zustand";

export type Screen = "startup" | "dashboard";
export type StatusMessageMode = "sync" | "snapshot-success" | "snapshot-error";

interface ToastState {
  visible: boolean;
  pubkyText: string;
}

interface UIState {
  currentScreen: Screen;
  statusMessageMode: StatusMessageMode;
  pubkyInputValue: string;
  hasAutoLoaded: boolean;
  toast: ToastState;
  // Actions
  setScreen: (screen: Screen) => void;
  setStatusMessageMode: (mode: StatusMessageMode) => void;
  setPubkyInputValue: (value: string) => void;
  setHasAutoLoaded: (value: boolean) => void;
  showToast: (pubkyText: string) => void;
  hideToast: () => void;
}

export const useUIStore = create<UIState>((set) => ({
  currentScreen: "startup",
  statusMessageMode: "sync",
  pubkyInputValue: "",
  hasAutoLoaded: false,
  toast: {
    visible: false,
    pubkyText: "",
  },

  setScreen: (screen) => set({ currentScreen: screen }),
  setStatusMessageMode: (mode) => set({ statusMessageMode: mode }),
  setPubkyInputValue: (value) => set({ pubkyInputValue: value }),
  setHasAutoLoaded: (value) => set({ hasAutoLoaded: value }),

  showToast: (pubkyText) =>
    set({
      toast: {
        visible: true,
        pubkyText,
      },
    }),

  hideToast: () =>
    set((state) => ({
      toast: {
        ...state.toast,
        visible: false,
      },
    })),
}));
