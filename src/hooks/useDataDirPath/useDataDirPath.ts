import { useQuery } from "@tanstack/react-query";
import { getDataDirPath } from "@/services";

/**
 * useDataDirPath
 *
 * Query hook for fetching the data directory path.
 * Returns the filesystem path where backup data is stored.
 *
 * @param enabled - Whether to enable the query (default: true)
 * @returns TanStack Query result with path string
 *
 * @example
 * ```tsx
 * const { data: appState } = useAppState();
 * const { data: dataDirPath } = useDataDirPath(appState?.pubky !== null);
 *
 * return (
 *   <InfoCard
 *     icon={<FolderIcon />}
 *     label="Backup Location"
 *     value={dataDirPath ?? '--'}
 *   />
 * );
 * ```
 */
export function useDataDirPath(enabled = true) {
  return useQuery({
    queryKey: ["dataDirPath"],
    queryFn: getDataDirPath,
    staleTime: Infinity, // Path doesn't change during session
    enabled,
  });
}
