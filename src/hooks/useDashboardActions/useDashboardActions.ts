import { useEffect, useRef, useCallback } from "react";
import * as Stores from "@/stores";
import * as Hooks from "@/hooks";
import { Logger } from "@/lib";

const SNAPSHOT_MESSAGE_DURATION_MS = 3000;

/**
 * Actions returned by useDashboardActions hook
 */
export interface DashboardActions {
  handleCopy: (pubky: string | null) => void;
  handleForceSync: (pubky: string | null) => Promise<void>;
  handleSnapshot: (pubky: string | null) => Promise<void>;
  handleOpenDataDir: () => Promise<void>;
  handleSelectKey: (selectedPubky: string) => void;
  isForceSyncing: boolean;
  isCreatingSnapshot: boolean;
}

/**
 * useDashboardActions
 *
 * Hook that consolidates all dashboard action handlers.
 * Manages snapshot message timeouts and wraps backend operations with logging.
 *
 * @returns Dashboard action handlers and loading states
 *
 * @example
 * ```tsx
 * const { handleCopy, handleForceSync, isForceSyncing } = useDashboardActions();
 *
 * return (
 *   <button onClick={() => handleForceSync(pubky)} disabled={isForceSyncing}>
 *     {isForceSyncing ? 'Syncing...' : 'Force Sync'}
 *   </button>
 * );
 * ```
 */
export function useDashboardActions(): DashboardActions {
  const { showToast, showErrorToast, setStatusMessageMode } =
    Stores.useUIStore.getState();

  const { forceSync, isPending: isForceSyncing } = Hooks.useForceSync();
  const { createSnapshotFn, isPending: isCreatingSnapshot } =
    Hooks.useCreateSnapshot();
  const { openDir } = Hooks.useOpenDataDir();
  const { setLastPubky } = Hooks.useSetLastPubky();

  // Snapshot message timeout ref
  const snapshotTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Cleanup on unmount: clear timeout and reset snapshot message mode
  useEffect(() => {
    return () => {
      if (snapshotTimeoutRef.current !== null) {
        clearTimeout(snapshotTimeoutRef.current);
      }
      setStatusMessageMode("sync");
    };
  }, [setStatusMessageMode]);

  const showSnapshotMessage = useCallback(
    (mode: "snapshot-success" | "snapshot-error") => {
      // Clear any existing timeout
      if (snapshotTimeoutRef.current !== null) {
        clearTimeout(snapshotTimeoutRef.current);
      }

      setStatusMessageMode(mode);

      // Revert after timeout
      snapshotTimeoutRef.current = setTimeout(() => {
        setStatusMessageMode("sync");
        snapshotTimeoutRef.current = null;
      }, SNAPSHOT_MESSAGE_DURATION_MS);
    },
    [setStatusMessageMode],
  );

  const handleCopy = useCallback(
    (pubky: string | null) => {
      if (pubky === null) return;
      navigator.clipboard
        .writeText(pubky)
        .then(() => {
          showToast(pubky);
        })
        .catch((err: unknown) => {
          Logger.error("DashboardActions", "Failed to copy pubky", {
            error: err,
          });
          showErrorToast("Failed to copy to clipboard");
        });
    },
    [showToast],
  );

  const handleForceSync = useCallback(
    async (pubky: string | null) => {
      if (pubky === null) return;
      try {
        await forceSync(pubky);
        Logger.debug("DashboardActions", "Force sync triggered");
      } catch (error: unknown) {
        Logger.error("DashboardActions", "Force sync failed", { error });
      }
    },
    [forceSync],
  );

  const handleSnapshot = useCallback(
    async (pubky: string | null) => {
      if (pubky === null) return;
      try {
        const snapshotPath = await createSnapshotFn(pubky);
        Logger.debug("DashboardActions", "Snapshot created", { snapshotPath });
        showSnapshotMessage("snapshot-success");
      } catch (error: unknown) {
        Logger.error("DashboardActions", "Snapshot creation failed", { error });
        showSnapshotMessage("snapshot-error");
      }
    },
    [createSnapshotFn, showSnapshotMessage],
  );

  const handleOpenDataDir = useCallback(async () => {
    try {
      await openDir();
    } catch (error: unknown) {
      Logger.error("DashboardActions", "Failed to open data directory", {
        error,
      });
    }
  }, [openDir]);

  const handleSelectKey = useCallback(
    (selectedPubky: string) => {
      void setLastPubky(selectedPubky);
    },
    [setLastPubky],
  );

  return {
    handleCopy,
    handleForceSync,
    handleSnapshot,
    handleOpenDataDir,
    handleSelectKey,
    isForceSyncing,
    isCreatingSnapshot,
  };
}
