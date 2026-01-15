import { useQuery } from "@tanstack/react-query";
import { getPreviousPubkyKeys } from "@/services/tauri-commands";

/**
 * Query hook for fetching previously used pubky keys
 */
export function usePreviousPubkyKeys() {
  return useQuery({
    queryKey: ["previousPubkyKeys"],
    queryFn: getPreviousPubkyKeys,
    staleTime: Infinity, // Keys don't change during session
  });
}
