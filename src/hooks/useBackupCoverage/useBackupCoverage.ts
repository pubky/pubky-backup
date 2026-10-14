import { useUIStore } from "@/stores/uiStore";
import {
  backupCoverage,
  SIGNED_OUT,
  type BackupCoverage,
} from "@/utils/coverage";

/**
 * useBackupCoverage
 *
 * Which of a pubky's data is backed up: public only, or private data too.
 * A pubky without a known state yet, or no pubky, counts as signed out.
 *
 * @param pubky - The pubky to look up, if any
 * @returns The pubky's backup coverage
 *
 * @example
 * ```tsx
 * const coverage = useBackupCoverage(pubky);
 * return <CoverageBadge coverage={coverage} />;
 * ```
 */
export function useBackupCoverage(pubky: string | null): BackupCoverage {
  return useUIStore((s) =>
    backupCoverage(
      (pubky !== null ? s.keyStates[pubky]?.auth : undefined) ?? SIGNED_OUT,
    ),
  );
}
