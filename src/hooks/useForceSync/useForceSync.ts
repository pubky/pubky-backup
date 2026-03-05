import { useCallback, useState } from "react";
import { forceSyncNow } from "@/services";

/**
 * useForceSync
 *
 * Hook for forcing an immediate sync for a specific pubky.
 * Triggers a manual backup sync regardless of the scheduled time.
 *
 * @returns Object with forceSync function and isPending state
 *
 * @example
 * ```tsx
 * const { forceSync, isPending } = useForceSync();
 *
 * const handleForceSync = async () => {
 *   try {
 *     await forceSync(pubky);
 *     console.log('Sync triggered successfully');
 *   } catch (error) {
 *     console.error('Failed to trigger sync', error);
 *   }
 * };
 *
 * return (
 *   <button onClick={handleForceSync} disabled={isPending}>
 *     {isPending ? 'Syncing...' : 'Sync Now'}
 *   </button>
 * );
 * ```
 */
export function useForceSync() {
  const [isPending, setIsPending] = useState(false);

  const forceSync = useCallback(async (pubky: string) => {
    setIsPending(true);
    try {
      await forceSyncNow(pubky);
    } finally {
      setIsPending(false);
    }
  }, []);

  return { forceSync, isPending };
}
