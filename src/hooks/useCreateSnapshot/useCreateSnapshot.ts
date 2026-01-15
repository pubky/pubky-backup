import { useMutation } from "@tanstack/react-query";
import { createSnapshot } from "@/services";

/**
 * useCreateSnapshot
 *
 * Mutation hook for creating a backup snapshot.
 * Returns the path to the created snapshot file on success.
 *
 * @returns TanStack Mutation result with mutate/mutateAsync functions
 *
 * @example
 * ```tsx
 * const { mutateAsync: snapshot, isPending } = useCreateSnapshot();
 *
 * const handleSnapshot = async () => {
 *   try {
 *     const snapshotPath = await snapshot();
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
  return useMutation({
    mutationFn: createSnapshot,
  });
}
