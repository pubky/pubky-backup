export type Screen = "startup" | "dashboard";
export type StatusMessageMode = "sync" | "snapshot-success" | "snapshot-error";
export type ToastType = "success" | "error";

export interface ToastState {
  visible: boolean;
  pubkyText: string;
  type: ToastType;
  message: string;
}

export interface UIState {
  currentScreen: Screen;
  statusMessageMode: StatusMessageMode;
  pubkyInputValue: string;
  hasAutoLoaded: boolean;
  toast: ToastState;
}

export interface UIActions {
  setScreen: (screen: Screen) => void;
  setStatusMessageMode: (mode: StatusMessageMode) => void;
  setPubkyInputValue: (value: string) => void;
  setHasAutoLoaded: (value: boolean) => void;
  showToast: (pubkyText: string) => void;
  showErrorToast: (message: string) => void;
  hideToast: () => void;
}

export type UIStore = UIState & UIActions;

export const uiInitialState: UIState = {
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
};

export enum UIActionTypes {
  SET_SCREEN = "SET_SCREEN",
  SET_STATUS_MESSAGE_MODE = "SET_STATUS_MESSAGE_MODE",
  SET_PUBKY_INPUT_VALUE = "SET_PUBKY_INPUT_VALUE",
  SET_HAS_AUTO_LOADED = "SET_HAS_AUTO_LOADED",
  SHOW_TOAST = "SHOW_TOAST",
  SHOW_ERROR_TOAST = "SHOW_ERROR_TOAST",
  HIDE_TOAST = "HIDE_TOAST",
}
