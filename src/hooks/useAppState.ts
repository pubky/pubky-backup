import { useQuery } from "@tanstack/react-query";
import { fetchState } from "@/services/tauri-commands";

const POLL_INTERVAL_MS = 200;

/**
 * Query hook for polling app state from the backend
 * Polls every 200ms when enabled
 */
export function useAppState(enabled = true) {
  return useQuery({
    queryKey: ["appState"],
    queryFn: fetchState,
    refetchInterval: POLL_INTERVAL_MS,
    refetchIntervalInBackground: false,
    enabled,
  });
}
