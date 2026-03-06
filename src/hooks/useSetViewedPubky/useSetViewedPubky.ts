import { useCallback, useState } from "react";
import { useUIStore } from "@/stores/uiStore";
import { setLastPubky } from "@/services";
import { stripPubkyPrefix } from "@/utils/pubky";

/**
 * useSetViewedPubky
 *
 * Hook for switching which pubky is currently being viewed in the UI.
 * Updates the Zustand store immediately and persists to backend for next launch.
 *
 * @returns Object with setViewedPubky function and isPending state
 *
 * @example
 * ```tsx
 * const { setViewedPubky, isPending } = useSetViewedPubky();
 *
 * const handleSelect = async (pubky: string) => {
 *   await setViewedPubky(pubky);
 *   // Navigate to sync page
 * };
 * ```
 */
export function useSetViewedPubky() {
  const setViewedPubkyStore = useUIStore((s) => s.setViewedPubky);
  const [isPending, setIsPending] = useState(false);

  const setViewedPubky = useCallback(
    async (pubky: string) => {
      // Normalize to strip any "pubky" prefix
      const normalized = stripPubkyPrefix(pubky);
      setIsPending(true);
      try {
        // Update Zustand store immediately
        setViewedPubkyStore(normalized);

        // Persist to backend for next app launch
        await setLastPubky(normalized);
      } finally {
        setIsPending(false);
      }
    },
    [setViewedPubkyStore],
  );

  return { setViewedPubky, isPending };
}
