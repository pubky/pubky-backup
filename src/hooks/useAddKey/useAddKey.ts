import { useCallback, useState } from "react";
import { addKey } from "@/services";
import { useUIStore } from "@/stores/uiStore";
import { stripPubkyPrefix } from "@/utils/pubky";

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
  const setLastPubky = useUIStore((s) => s.setLastPubky);

  const addKeyFn = useCallback(
    async ({ pubkyValue }: AddKeyParams): Promise<string> => {
      setIsPending(true);
      try {
        const normalizedPubky = stripPubkyPrefix(await addKey(pubkyValue));
        // Set the added key as the viewed key
        setLastPubky(normalizedPubky);
        return normalizedPubky;
      } finally {
        setIsPending(false);
      }
    },
    [setLastPubky],
  );

  return { addKey: addKeyFn, isPending };
}
