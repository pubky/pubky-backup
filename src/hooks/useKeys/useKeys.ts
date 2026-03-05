import { useQuery } from "@tanstack/react-query";
import { getKeys } from "@/services";

/**
 * useKeys
 *
 * Query hook for fetching pubky keys that have data stored.
 * Used to populate autocomplete suggestions in the pubky input.
 *
 * @returns TanStack Query result with array of pubky strings
 *
 * @example
 * ```tsx
 * const { data: keys = [], isLoading } = useKeys();
 *
 * return (
 *   <PubkyInput
 *     value={pubkyValue}
 *     onChange={setPubkyValue}
 *     suggestions={keys}
 *     placeholder={keys.length > 0 ? 'Enter your pubky...' : 'g1b6wp8bhhxt...'}
 *   />
 * );
 * ```
 */
export function useKeys() {
  return useQuery({
    queryKey: ["keys"],
    queryFn: getKeys,
    staleTime: Infinity, // Keys don't change during session
  });
}
