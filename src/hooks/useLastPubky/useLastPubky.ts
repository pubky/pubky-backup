import { useEffect, useState } from "react";
import { getLastPubky } from "@/services";

/**
 * useLastPubky
 *
 * Hook for fetching the last used pubky on initial load.
 * Only fetches once on component mount.
 *
 * @returns Object with lastPubky value and isLoading state
 *
 * @example
 * ```tsx
 * const { lastPubky, isLoading } = useLastPubky();
 *
 * useEffect(() => {
 *   if (lastPubky && !hasAutoLoaded) {
 *     setHasAutoLoaded(true);
 *     setPubkyInputValue(lastPubky);
 *     handleInitialize(lastPubky);
 *   }
 * }, [lastPubky, hasAutoLoaded]);
 * ```
 */
export function useLastPubky() {
  const [lastPubky, setLastPubky] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    getLastPubky()
      .then((pubky) => {
        setLastPubky(pubky);
      })
      .catch(() => {
        // Ignore errors - lastPubky is optional
      })
      .finally(() => {
        setIsLoading(false);
      });
  }, []);

  return { lastPubky, isLoading };
}
