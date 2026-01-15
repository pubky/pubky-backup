import { useMutation } from "@tanstack/react-query";
import { backupControllerClose } from "@/services/tauri-commands";

/**
 * Mutation hook for closing the backup controller
 */
export function useBackupControllerClose() {
  return useMutation({
    mutationFn: backupControllerClose,
  });
}
