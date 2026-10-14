import { useShallow } from "zustand/react/shallow";
import { useUIStore } from "@/stores/uiStore";
import {
  backupCoverage,
  SIGNED_OUT,
  type BackupCoverage,
} from "@/utils/coverage";

/**
 * useBackupCoverageByKey
 *
 * Like `useBackupCoverage`, for several pubkys at once.
 *
 * @param pubkys - The pubkys to look up
 * @returns The coverage of each pubky, keyed by pubky
 *
 * @example
 * ```tsx
 * const coverage = useBackupCoverageByKey(keys);
 * return keys.map((k) => <KeyOption key={k} coverage={coverage[k]} />);
 * ```
 */
export function useBackupCoverageByKey(
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
