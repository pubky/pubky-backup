import { useEffect, useState } from "react";
import { getDataDirPath } from "@/services";

/**
 * useDataDirPath
 *
 * Hook for fetching the data directory path.
 * Returns the filesystem path where backup data is stored.
 * Only fetches once since the path doesn't change during session.
 *
 * @returns Object with dataDirPath value and isLoading state
 *
 * @example
 * ```tsx
 * const { dataDirPath } = useDataDirPath();
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
export function useDataDirPath() {
  const [dataDirPath, setDataDirPath] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    getDataDirPath()
      .then((path) => {
        setDataDirPath(path);
      })
      .catch(() => {
        // Ignore errors
      })
      .finally(() => {
        setIsLoading(false);
      });
  }, []);

  return { dataDirPath, isLoading };
}
