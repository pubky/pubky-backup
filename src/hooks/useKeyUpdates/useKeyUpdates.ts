import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { useUIStore } from "@/stores/uiStore";
import type { KeyUpdate } from "@/stores/uiStore";

/**
 * useKeyUpdates
 *
 * Hook that listens for key-update events from the Tauri backend
 * and updates the Zustand store with new key states.
 *
 * Should be called once at the app root to establish the event listener.
 *
 * @example
 * ```tsx
 * function App() {
 *   useKeyUpdates();
 *   return <MainContent />;
 * }
 * ```
 */
export function useKeyUpdates() {
  const setKeyState = useUIStore((state) => state.setKeyState);

  useEffect(() => {
    let cancelled = false;
    let unlisten: UnlistenFn | undefined;

    listen<KeyUpdate>("key-update", (event) => {
      if (!cancelled) {
        setKeyState(event.payload.pubky, event.payload.state);
      }
    }).then((fn) => {
      if (cancelled) {
        // Component unmounted before listen resolved, clean up immediately
        fn();
      } else {
        unlisten = fn;
      }
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [setKeyState]);
}
