import { useCallback, useState } from "react";
import { useUIStore } from "@/stores/uiStore";
import { setLastPubky } from "@/services";
import { stripPubkyPrefix } from "@/utils/pubky";

/**
 * useSetLastPubky
 *
 * Hook for switching which pubky is currently being viewed in the UI.
 * Updates the Zustand store immediately and persists to backend for next launch.
 *
 * @returns Object with setLastPubky function and isPending state
 *
 * @example
 * ```tsx
 * const { setLastPubky, isPending } = useSetLastPubky();
 *
 * const handleSelect = async (pubky: string) => {
 *   await setLastPubky(pubky);
 *   // Navigate to sync page
 * };
 * ```
 */
export function useSetLastPubky() {
  const setLastPubkyStore = useUIStore((s) => s.setLastPubky);
  const [isPending, setIsPending] = useState(false);

  const setLastPubkyFn = useCallback(
    async (pubky: string) => {
      // Normalize to strip any "pubky" prefix
      const normalized = stripPubkyPrefix(pubky);
      setIsPending(true);
      try {
        // Update Zustand store immediately
        setLastPubkyStore(normalized);

        // Persist to backend for next app launch
        await setLastPubky(normalized);
      } finally {
        setIsPending(false);
      }
    },
    [setLastPubkyStore],
  );

  return { setLastPubky: setLastPubkyFn, isPending };
}
