import { useQuery } from "@tanstack/react-query";
import { getLastPubky } from "@/services";

/**
 * useLastPubky
 *
 * Query hook for fetching the last used pubky.
 * Used for auto-loading the previous pubky on app startup.
 *
 * @returns TanStack Query result with pubky string or null
 *
 * @example
 * ```tsx
 * const { data: lastPubky } = useLastPubky();
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
  return useQuery({
    queryKey: ["lastPubky"],
    queryFn: getLastPubky,
    staleTime: Infinity, // Only needed once at startup
  });
}
