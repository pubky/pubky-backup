import { useEffect, useState } from "react";
import { getLastPubky } from "@/services";
import { stripPubkyPrefix } from "@/utils/pubky";

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
 * ```
 */
export function useLastPubky() {
  const [lastPubky, setLastPubky] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    getLastPubky()
      .then((pubky) => {
        // Normalize to strip any "pubky" prefix for consistency
        setLastPubky(pubky ? stripPubkyPrefix(pubky) : null);
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
