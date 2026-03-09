import { useEffect, useState } from "react";
import { getConfig } from "@/services";

/**
 * useBackupLocation
 *
 * Hook for fetching the keys directory path (backup location).
 * Re-fetches on every mount so it picks up changes after a move.
 *
 * @returns Object with backupLocation value and isLoading state
 *
 * @example
 * ```tsx
 * const { backupLocation } = useBackupLocation();
 *
 * return (
 *   <InfoCard
 *     icon={<FolderIcon />}
 *     label="Backup Location"
 *     value={backupLocation ?? '--'}
 *   />
 * );
 * ```
 */
export function useBackupLocation() {
  const [backupLocation, setBackupLocation] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    getConfig()
      .then((config) => {
        setBackupLocation(config.keys_dir);
      })
      .catch(() => {
        // Ignore errors
      })
      .finally(() => {
        setIsLoading(false);
      });
  }, []);

  return { backupLocation, isLoading };
}
