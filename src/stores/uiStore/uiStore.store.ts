import { create } from "zustand";
import { devtools } from "zustand/middleware";
import {
  type UIStore,
  type Screen,
  type Page,
  type StatusMessageMode,
  type KeyState,
  uiInitialState,
  UIActionTypes,
} from "./uiStore.types";

export const useUIStore = create<UIStore>()(
  devtools(
    (set) => ({
      ...uiInitialState,

      setScreen: (screen: Screen) =>
        set({ currentScreen: screen }, false, UIActionTypes.SET_SCREEN),

      setPage: (page: Page) =>
        set({ currentPage: page }, false, UIActionTypes.SET_PAGE),

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
              type: "success",
              message: "Pubky copied to clipboard",
            },
          },
          false,
          UIActionTypes.SHOW_TOAST,
        ),

      showErrorToast: (message: string) =>
        set(
          {
            toast: {
              visible: true,
              pubkyText: "",
              type: "error",
              message,
            },
          },
          false,
          UIActionTypes.SHOW_ERROR_TOAST,
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

      setKeyState: (pubky: string, state: KeyState) =>
        set(
          (s) => ({
            keyStates: { ...s.keyStates, [pubky]: state },
          }),
          false,
          UIActionTypes.SET_KEY_STATE,
        ),

      setAllKeyStates: (states: Record<string, KeyState>) =>
        set({ keyStates: states }, false, UIActionTypes.SET_ALL_KEY_STATES),

      removeKeyState: (pubky: string) =>
        set(
          (s) => {
            const { [pubky]: _, ...rest } = s.keyStates;
            return { keyStates: rest };
          },
          false,
          UIActionTypes.REMOVE_KEY_STATE,
        ),

      setViewedPubky: (pubky: string | null) =>
        set({ viewedPubky: pubky }, false, UIActionTypes.SET_VIEWED_PUBKY),

      setDeveloperMode: (enabled: boolean) =>
        set(
          { developerMode: enabled },
          false,
          UIActionTypes.SET_DEVELOPER_MODE,
        ),

      setNavDisabled: (disabled: boolean) =>
        set({ navDisabled: disabled }, false, UIActionTypes.SET_NAV_DISABLED),
    }),
    {
      name: "ui-store",
      enabled: import.meta.env.MODE === "development",
    },
  ),
);
