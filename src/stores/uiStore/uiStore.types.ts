export type Screen = "startup" | "dashboard";
export type StatusMessageMode = "sync" | "snapshot-success" | "snapshot-error";
export type ToastType = "success" | "error";
export type Page = "sync" | "activity" | "keys" | "settings";

export interface ToastState {
  visible: boolean;
  pubkyText: string;
  type: ToastType;
  message: string;
}

// Types matching Rust KeyState/KeyUpdate from pubky-backup-core
export type KeyErrorCode =
  | "HomeserverUnreachable"
  | "HomeserverNotFound"
  | "NoDataFound"
  | "StorageFull"
  | "NetworkError"
  | "Timeout"
  | "Internal";

export interface KeyError {
  code: KeyErrorCode;
  message: string;
  recoverable: boolean;
}

export type KeyStatus =
  | { type: "Starting" }
  | { type: "Syncing"; events_processed: number }
  | { type: "Idle" }
  | { type: "Stopped" }
  | { type: "Error" };

export interface KeyState {
  status: KeyStatus;
  data_size: number;
  last_sync: number | null;
  next_sync: number | null;
  error: KeyError | null;
  total_files: number | null;
  files_synced: number | null;
  bytes_downloaded: number | null;
}

export interface KeyUpdate {
  pubky: string;
  state: KeyState;
}

export interface UIState {
  currentScreen: Screen;
  currentPage: Page;
  statusMessageMode: StatusMessageMode;
  pubkyInputValue: string;
  hasAutoLoaded: boolean;
  toast: ToastState;
  // App state from backend
  keyStates: Record<string, KeyState>;
  viewedPubky: string | null;
  developerMode: boolean;
  navDisabled: boolean;
}

export interface UIActions {
  setScreen: (screen: Screen) => void;
  setPage: (page: Page) => void;
  setStatusMessageMode: (mode: StatusMessageMode) => void;
  setPubkyInputValue: (value: string) => void;
  setHasAutoLoaded: (value: boolean) => void;
  showToast: (pubkyText: string) => void;
  showErrorToast: (message: string) => void;
  hideToast: () => void;
  // Key state actions
  setKeyState: (pubky: string, state: KeyState) => void;
  setAllKeyStates: (states: Record<string, KeyState>) => void;
  removeKeyState: (pubky: string) => void;
  setViewedPubky: (pubky: string | null) => void;
  setDeveloperMode: (enabled: boolean) => void;
  setNavDisabled: (disabled: boolean) => void;
}

export type UIStore = UIState & UIActions;

export const uiInitialState: UIState = {
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
  viewedPubky: null,
  developerMode: false,
  navDisabled: false,
};

export enum UIActionTypes {
  SET_SCREEN = "SET_SCREEN",
  SET_PAGE = "SET_PAGE",
  SET_STATUS_MESSAGE_MODE = "SET_STATUS_MESSAGE_MODE",
  SET_PUBKY_INPUT_VALUE = "SET_PUBKY_INPUT_VALUE",
  SET_HAS_AUTO_LOADED = "SET_HAS_AUTO_LOADED",
  SHOW_TOAST = "SHOW_TOAST",
  SHOW_ERROR_TOAST = "SHOW_ERROR_TOAST",
  HIDE_TOAST = "HIDE_TOAST",
  SET_KEY_STATE = "SET_KEY_STATE",
  SET_ALL_KEY_STATES = "SET_ALL_KEY_STATES",
  REMOVE_KEY_STATE = "REMOVE_KEY_STATE",
  SET_VIEWED_PUBKY = "SET_VIEWED_PUBKY",
  SET_DEVELOPER_MODE = "SET_DEVELOPER_MODE",
  SET_NAV_DISABLED = "SET_NAV_DISABLED",
}
