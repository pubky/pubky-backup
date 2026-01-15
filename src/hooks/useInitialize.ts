import { useMutation } from "@tanstack/react-query";
import { initAppState, backupControllerBegin } from "@/services/tauri-commands";

interface InitializeParams {
  pubkyValue: string;
}

/**
 * Mutation hook for initializing the app and starting backup
 */
export function useInitialize() {
  return useMutation({
    mutationFn: async ({ pubkyValue }: InitializeParams) => {
      await initAppState(pubkyValue);
      await backupControllerBegin();
    },
  });
}
