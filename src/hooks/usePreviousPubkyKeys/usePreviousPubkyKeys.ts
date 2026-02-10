import { useQuery } from "@tanstack/react-query";
import { getPreviousPubkyKeys } from "@/services";

/**
 * usePreviousPubkyKeys
 *
 * Query hook for fetching previously used pubky keys.
 * Used to populate autocomplete suggestions in the pubky input.
 *
 * @returns TanStack Query result with array of pubky strings
 *
 * @example
 * ```tsx
 * const { data: previousKeys = [], isLoading } = usePreviousPubkyKeys();
 *
 * return (
 *   <PubkyInput
 *     value={pubkyValue}
 *     onChange={setPubkyValue}
 *     suggestions={previousKeys}
 *     placeholder={previousKeys.length > 0 ? 'Enter your pubky...' : 'g1b6wp8bhhxt...'}
 *   />
 * );
 * ```
 */
export function usePreviousPubkyKeys() {
  return useQuery({
    queryKey: ["previousPubkyKeys"],
    queryFn: getPreviousPubkyKeys,
    staleTime: Infinity, // Keys don't change during session
  });
}
