import { useQuery } from "@tanstack/react-query";
import { getDataDirPath } from "@/services/tauri-commands";

/**
 * Query hook for fetching the data directory path
 */
export function useDataDirPath(enabled = true) {
  return useQuery({
    queryKey: ["dataDirPath"],
    queryFn: getDataDirPath,
    staleTime: Infinity, // Path doesn't change during session
    enabled,
  });
}
