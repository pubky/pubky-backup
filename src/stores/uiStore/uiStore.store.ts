import { create } from "zustand";
import { devtools } from "zustand/middleware";
import {
  type UIStore,
  type Screen,
  type StatusMessageMode,
  uiInitialState,
  UIActionTypes,
} from "./uiStore.types";

export const useUIStore = create<UIStore>()(
  devtools(
    (set) => ({
      ...uiInitialState,

      setScreen: (screen: Screen) =>
        set({ currentScreen: screen }, false, UIActionTypes.SET_SCREEN),

      setStatusMessageMode: (mode: StatusMessageMode) =>
        set(
          { statusMessageMode: mode },
          false,
          UIActionTypes.SET_STATUS_MESSAGE_MODE,
        ),

      setPubkyInputValue: (value: string) =>
        set(
          { pubkyInputValue: value },
          false,
          UIActionTypes.SET_PUBKY_INPUT_VALUE,
        ),

      setHasAutoLoaded: (value: boolean) =>
        set({ hasAutoLoaded: value }, false, UIActionTypes.SET_HAS_AUTO_LOADED),

      showToast: (pubkyText: string) =>
        set(
          {
            toast: {
              visible: true,
              pubkyText,
            },
          },
          false,
          UIActionTypes.SHOW_TOAST,
        ),

      hideToast: () =>
        set(
          (state) => ({
            toast: {
              ...state.toast,
              visible: false,
            },
          }),
          false,
          UIActionTypes.HIDE_TOAST,
        ),
    }),
    {
      name: "ui-store",
      enabled: import.meta.env.MODE === "development",
    },
  ),
);
