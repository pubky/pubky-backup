import { useMutation } from "@tanstack/react-query";
import { backupControllerClose } from "@/services";

/**
 * useBackupControllerClose
 *
 * Mutation hook for closing the backup controller.
 * Stops the active backup session and cleans up resources.
 *
 * @returns TanStack Mutation result with mutate/mutateAsync functions
 *
 * @example
 * ```tsx
 * const { mutateAsync: closeBackup, isPending } = useBackupControllerClose();
 *
 * const handleBack = async () => {
 *   try {
 *     await closeBackup();
 *   } catch (error) {
 *     // Controller may already be stopped - log but continue
 *     console.warn('Controller already stopped');
 *   }
 *   navigateToStartup();
 * };
 *
 * return (
 *   <button onClick={handleBack} disabled={isPending}>
 *     Back
 *   </button>
 * );
 * ```
 */
export function useBackupControllerClose() {
  return useMutation({
    mutationFn: backupControllerClose,
  });
}
