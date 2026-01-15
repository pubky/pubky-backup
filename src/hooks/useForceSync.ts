import { useMutation } from "@tanstack/react-query";
import { forceSyncNow } from "@/services/tauri-commands";

/**
 * Mutation hook for forcing an immediate sync
 */
export function useForceSync() {
  return useMutation({
    mutationFn: forceSyncNow,
  });
}
