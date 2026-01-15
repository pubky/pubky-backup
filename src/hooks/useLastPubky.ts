import { useQuery } from "@tanstack/react-query";
import { getLastPubky } from "@/services/tauri-commands";

/**
 * Query hook for fetching the last used pubky (for auto-load)
 */
export function useLastPubky() {
  return useQuery({
    queryKey: ["lastPubky"],
    queryFn: getLastPubky,
    staleTime: Infinity, // Only needed once at startup
  });
}
