import { useQuery } from "@tanstack/react-query";
import { fetchState } from "@/services";

const POLL_INTERVAL_MS = 200;

/**
 * useAppState
 *
 * Query hook for polling app state from the backend.
 * Polls every 200ms when enabled to keep UI in sync with backend state.
 *
 * @param enabled - Whether to enable polling (default: true)
 * @returns TanStack Query result with app state data
 *
 * @example
 * ```tsx
 * const { data: appState, isLoading, error } = useAppState();
 *
 * if (isLoading) return <Spinner />;
 * if (error) return <Error message={error.message} />;
 *
 * return (
 *   <div>
 *     <p>Pubky: {appState?.pubky}</p>
 *     <p>Syncing: {appState?.is_syncing ? 'Yes' : 'No'}</p>
 *   </div>
 * );
 * ```
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
