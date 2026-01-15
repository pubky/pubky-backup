import { useMutation } from "@tanstack/react-query";
import { createSnapshot } from "@/services/tauri-commands";

/**
 * Mutation hook for creating a snapshot
 * Returns the path to the created snapshot file
 */
export function useCreateSnapshot() {
  return useMutation({
    mutationFn: createSnapshot,
  });
}
