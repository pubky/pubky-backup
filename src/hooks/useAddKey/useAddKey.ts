import { useCallback, useState } from "react";
import { addKey } from "@/services";
import { useUIStore } from "@/stores/uiStore";

interface AddKeyParams {
  pubkyValue: string;
}

/**
 * useAddKey
 *
 * Hook for adding a key and starting backup.
 * Updates the viewed pubky in the store after successful add.
 *
 * @returns Object with addKey function and isPending state
 *
 * @example
 * ```tsx
 * const { addKey, isPending } = useAddKey();
 *
 * const handleSubmit = async (pubky: string) => {
 *   try {
 *     await addKey({ pubkyValue: pubky });
 *     setScreen('dashboard');
 *   } catch (error) {
 *     handleError(error);
 *   }
 * };
 *
 * return (
 *   <button onClick={() => handleSubmit(pubkyInput)} disabled={isPending}>
 *     {isPending ? 'Starting...' : 'Start Backup'}
 *   </button>
 * );
 * ```
 */
export function useAddKey() {
  const [isPending, setIsPending] = useState(false);
  const setViewedPubky = useUIStore((s) => s.setViewedPubky);

  const addKeyFn = useCallback(
    async ({ pubkyValue }: AddKeyParams): Promise<string> => {
      setIsPending(true);
      try {
        const normalizedPubky = await addKey(pubkyValue);
        // Set the added key as the viewed key
        setViewedPubky(normalizedPubky);
        return normalizedPubky;
      } finally {
        setIsPending(false);
      }
    },
    [setViewedPubky],
  );

  return { addKey: addKeyFn, isPending };
}
