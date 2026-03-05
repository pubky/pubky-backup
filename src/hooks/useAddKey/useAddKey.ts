import { useMutation, useQueryClient } from "@tanstack/react-query";
import { addKey } from "@/services";

interface AddKeyParams {
  pubkyValue: string;
}

/**
 * useAddKey
 *
 * Mutation hook for adding a key and starting backup.
 *
 * @returns TanStack Mutation result with mutate/mutateAsync functions
 *
 * @example
 * ```tsx
 * const { mutateAsync: addKeyMutation, isPending } = useAddKey();
 *
 * const handleSubmit = async (pubky: string) => {
 *   try {
 *     await addKeyMutation({ pubkyValue: pubky });
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
  const queryClient = useQueryClient();

  return useMutation<void, unknown, AddKeyParams>({
    mutationFn: async ({ pubkyValue }: AddKeyParams) => {
      await addKey(pubkyValue);
    },
    onSuccess: () => {
      // Invalidate the keys query so the new key appears in the list
      void queryClient.invalidateQueries({ queryKey: ["keys"] });
    },
  });
}
