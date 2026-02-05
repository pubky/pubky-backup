import { useMutation } from "@tanstack/react-query";
import { openDataDir } from "@/services";

/**
 * useOpenDataDir
 *
 * Mutation hook for opening the data directory in the file explorer.
 *
 * @returns TanStack Mutation result with mutate/mutateAsync functions
 *
 * @example
 * ```tsx
 * const { mutateAsync: openDir, isPending } = useOpenDataDir();
 *
 * const handleOpenDir = async () => {
 *   try {
 *     await openDir();
 *   } catch (error) {
 *     console.error('Failed to open data directory', error);
 *   }
 * };
 *
 * return (
 *   <button onClick={handleOpenDir} disabled={isPending}>
 *     Open Directory
 *   </button>
 * );
 * ```
 */
export function useOpenDataDir() {
  return useMutation({
    mutationFn: openDataDir,
  });
}
