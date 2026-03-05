import { useCallback, useState } from "react";
import { openDataDir } from "@/services";

/**
 * useOpenDataDir
 *
 * Hook for opening the data directory in the file explorer.
 *
 * @returns Object with openDir function and isPending state
 *
 * @example
 * ```tsx
 * const { openDir, isPending } = useOpenDataDir();
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
  const [isPending, setIsPending] = useState(false);

  const openDir = useCallback(async () => {
    setIsPending(true);
    try {
      await openDataDir();
    } finally {
      setIsPending(false);
    }
  }, []);

  return { openDir, isPending };
}
