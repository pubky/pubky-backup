import { useEffect, useRef, useCallback, useMemo } from "react";
import { useShallow } from "zustand/react/shallow";
import * as Atoms from "@/components/atoms";
import * as Molecules from "@/components/molecules";
import { ActionButtons } from "../ActionButtons";
import * as Stores from "@/stores";
import * as Hooks from "@/hooks";
import * as Utils from "@/utils";
import { Logger } from "@/lib";

const SNAPSHOT_MESSAGE_DURATION_MS = 3000;

export function DashboardForm() {
  const { statusMessageMode } = Stores.useUIStore(
    useShallow((s) => ({ statusMessageMode: s.statusMessageMode })),
  );
  const { setScreen, showToast, setStatusMessageMode } =
    Stores.useUIStore.getState();

  const { data: appState } = Hooks.useAppState();
  const { data: dataDirPath } = Hooks.useDataDirPath(appState?.pubky !== null);
  const forceSyncMutation = Hooks.useForceSync();
  const snapshotMutation = Hooks.useCreateSnapshot();
  const backupCloseMutation = Hooks.useBackupControllerClose();
  const openDataDirMutation = Hooks.useOpenDataDir();

  const pubky = appState?.pubky ?? null;
  const isSyncing = appState?.is_syncing ?? false;
  const nextSyncTime = appState?.next_sync_time ?? 0;
  const dataSize = appState?.data_dir_size ?? 0;
  const backupControllerError = appState?.backup_controller_error ?? null;

  const lastSyncTime = Hooks.useLastSyncTime(nextSyncTime, isSyncing);
  const countdownText = Hooks.useCountdown(nextSyncTime, isSyncing);

  // Snapshot message timeout ref
  const snapshotTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const handleBack = useCallback(async () => {
    try {
      await backupCloseMutation.mutateAsync();
    } catch (error: unknown) {
      // Intentionally not using handleBackendError here - we want to navigate
      // back regardless of whether the controller was already stopped
      Logger.error("DashboardForm", "Backup controller already stopped", {
        error,
      });
    }
    setScreen("startup");
  }, [backupCloseMutation, setScreen]);

  // Ref to avoid re-running effect when handleBack changes
  const handleBackRef = useRef(handleBack);
  handleBackRef.current = handleBack;

  // Handle backup controller errors
  useEffect(() => {
    if (backupControllerError !== null) {
      Utils.handleBackendError(backupControllerError);
      void handleBackRef.current();
    }
  }, [backupControllerError]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (snapshotTimeoutRef.current !== null) {
        clearTimeout(snapshotTimeoutRef.current);
      }
    };
  }, []);

  const handleCopy = () => {
    if (pubky === null) return;
    navigator.clipboard
      .writeText(pubky)
      .then(() => {
        showToast(pubky);
      })
      .catch((err: unknown) => {
        Logger.error("DashboardForm", "Failed to copy pubky", { error: err });
        alert("Failed to copy to clipboard");
      });
  };

  const handleForceSync = async () => {
    try {
      await forceSyncMutation.mutateAsync();
      Logger.debug("DashboardForm", "Force sync triggered");
    } catch (error: unknown) {
      Logger.error("DashboardForm", "Force sync failed", { error });
    }
  };

  const handleSnapshot = async () => {
    try {
      const snapshotPath = await snapshotMutation.mutateAsync();
      Logger.debug("DashboardForm", "Snapshot created", { snapshotPath });
      showSnapshotMessage("snapshot-success");
    } catch (error: unknown) {
      Logger.error("DashboardForm", "Snapshot creation failed", { error });
      showSnapshotMessage("snapshot-error");
    }
  };

  const showSnapshotMessage = (mode: "snapshot-success" | "snapshot-error") => {
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
  };

  const handleOpenDataDir = async () => {
    try {
      await openDataDirMutation.mutateAsync();
    } catch (error: unknown) {
      Logger.error("DashboardForm", "Failed to open data directory", { error });
    }
  };

  // Consolidated status information derived from state
  const statusInfo = useMemo(() => {
    if (statusMessageMode === "snapshot-success") {
      return {
        badge: "snapshot" as const,
        message: "snapshot-success" as const,
        text: "Snapshot created",
      };
    }
    if (statusMessageMode === "snapshot-error") {
      return {
        badge: "synced" as const,
        message: "error" as const,
        text: "Failed to create snapshot",
      };
    }
    if (isSyncing) {
      return {
        badge: "syncing" as const,
        message: "syncing" as const,
        text: "Syncing data...",
      };
    }
    return {
      badge: "synced" as const,
      message: "synced" as const,
      text: countdownText,
    };
  }, [statusMessageMode, isSyncing, countdownText]);

  return (
    <>
      {/* Header */}
      <Molecules.DashboardHeader
        pubkyDisplay={Utils.displayPubky(pubky)}
        status={statusInfo.badge}
        onBack={() => void handleBack()}
        onCopy={handleCopy}
      />

      {/* Sync message banner */}
      <Molecules.SyncMessage
        status={statusInfo.message}
        message={statusInfo.text}
      />

      {/* Info cards */}
      <div className="flex flex-col self-stretch p-3 bg-surface-light rounded-lg gap-0">
        <div className="flex justify-stretch items-stretch self-stretch">
          <Molecules.InfoCard
            icon={<Atoms.DatabaseIcon size={18} />}
            label="Backup Size"
            value={Utils.formatFileSize(dataSize)}
          />
          <Molecules.InfoCard
            icon={<Atoms.ClockIcon size={18} />}
            label="Last Sync"
            value={
              lastSyncTime !== null
                ? Utils.formatTimestamp(lastSyncTime)
                : "--"
            }
          />
        </div>
        <Molecules.InfoCard
          icon={<Atoms.FolderIcon size={18} />}
          label="Backup Location"
          value={dataDirPath ?? "--"}
          fullWidth
          action={
            <Atoms.IconButton
              variant="inline"
              onClick={() => void handleOpenDataDir()}
              title="Open data directory"
            >
              <Atoms.ExternalLinkIcon size={16} />
            </Atoms.IconButton>
          }
        />
      </div>

      {/* Action buttons */}
      <ActionButtons
        isSyncing={isSyncing}
        isCreatingSnapshot={snapshotMutation.isPending}
        isForceSyncing={forceSyncMutation.isPending}
        onSnapshot={() => void handleSnapshot()}
        onForceSync={() => void handleForceSync()}
      />
    </>
  );
}
