import { useMutation } from "@tanstack/react-query";
import { initAppState, backupControllerBegin } from "@/services";

interface InitializeParams {
  pubkyValue: string;
}

/**
 * useInitialize
 *
 * Mutation hook for initializing the app and starting backup.
 * Calls initAppState followed by backupControllerBegin.
 *
 * @returns TanStack Mutation result with mutate/mutateAsync functions
 *
 * @example
 * ```tsx
 * const { mutateAsync: initialize, isPending } = useInitialize();
 *
 * const handleSubmit = async (pubky: string) => {
 *   try {
 *     await initialize({ pubkyValue: pubky });
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
export function useInitialize() {
  return useMutation({
    mutationFn: async ({ pubkyValue }: InitializeParams) => {
      await initAppState(pubkyValue);
      await backupControllerBegin();
    },
  });
}
