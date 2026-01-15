import { useMutation } from "@tanstack/react-query";
import { forceSyncNow } from "@/services";

/**
 * useForceSync
 *
 * Mutation hook for forcing an immediate sync.
 * Triggers a manual backup sync regardless of the scheduled time.
 *
 * @returns TanStack Mutation result with mutate/mutateAsync functions
 *
 * @example
 * ```tsx
 * const { mutateAsync: forceSync, isPending } = useForceSync();
 *
 * const handleForceSync = async () => {
 *   try {
 *     await forceSync();
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
  return useMutation({
    mutationFn: forceSyncNow,
  });
}
