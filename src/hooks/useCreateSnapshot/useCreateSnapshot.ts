import { useCallback, useState } from "react";
import { createSnapshot } from "@/services";

/**
 * useCreateSnapshot
 *
 * Hook for creating a backup snapshot for a specific pubky.
 * Returns the path to the created snapshot file on success.
 *
 * @returns Object with createSnapshotFn function and isPending state
 *
 * @example
 * ```tsx
 * const { createSnapshotFn, isPending } = useCreateSnapshot();
 *
 * const handleSnapshot = async () => {
 *   try {
 *     const snapshotPath = await createSnapshotFn(pubky);
 *     console.log('Snapshot created at:', snapshotPath);
 *     showSuccessMessage();
 *   } catch (error) {
 *     showErrorMessage();
 *   }
 * };
 *
 * return (
 *   <button onClick={handleSnapshot} disabled={isPending}>
 *     {isPending ? 'Creating...' : 'Create Snapshot'}
 *   </button>
 * );
 * ```
 */
export function useCreateSnapshot() {
  const [isPending, setIsPending] = useState(false);

  const createSnapshotFn = useCallback(
    async (pubky: string): Promise<string> => {
      setIsPending(true);
      try {
        return await createSnapshot(pubky);
      } finally {
        setIsPending(false);
      }
    },
    [],
  );

  return { createSnapshotFn, isPending };
}
