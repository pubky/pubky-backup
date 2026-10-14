import { useShallow } from "zustand/react/shallow";
import { useUIStore } from "@/stores/uiStore";
import type { AuthStatus } from "@/stores/uiStore";
import { backupCoverage, type BackupCoverage } from "@/utils/coverage";

const SIGNED_OUT: AuthStatus = { type: "SignedOut" };

/**
 * useBackupCoverage
 *
 * Which of each pubky's data is backed up: public only, or private data too.
 * A pubky without a known state counts as signed out.
 *
 * @param pubkys - The pubkys to look up
 * @returns The coverage of each pubky, keyed by pubky
 *
 * @example
 * ```tsx
 * const coverage = useBackupCoverage([pubky]);
 * return <CoverageBadge coverage={coverage[pubky]} />;
 * ```
 */
export function useBackupCoverage(
  pubkys: string[],
): Record<string, BackupCoverage> {
  return useUIStore(
    useShallow((s) =>
      Object.fromEntries(
        pubkys.map((pubky) => [
          pubky,
          backupCoverage(s.keyStates[pubky]?.auth ?? SIGNED_OUT),
        ]),
      ),
    ),
  );
}
