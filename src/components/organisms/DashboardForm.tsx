import { useEffect, useRef, useCallback } from "react";
import {
  IconButton,
  ExternalLinkIcon,
  DatabaseIcon,
  ClockIcon,
  FolderIcon,
} from "@/components/atoms";
import { DashboardHeader, InfoCard, SyncMessage } from "@/components/molecules";
import { ActionButtons } from "./ActionButtons";
import { useUIStore } from "@/stores/useUIStore";
import {
  useAppState,
  useDataDirPath,
  useLastSyncTime,
  useCountdown,
  useForceSync,
  useCreateSnapshot,
  useBackupControllerClose,
} from "@/hooks";
import { openDataDir } from "@/services/tauri-commands";
import { displayPubky, formatFileSize, formatTimestamp } from "@/utils";
import { handleBackendError } from "@/utils/error-handler";
import { cn } from "@/lib/utils";

const SNAPSHOT_MESSAGE_DURATION_MS = 3000;

export function DashboardForm() {
  const { setScreen, showToast, statusMessageMode, setStatusMessageMode } =
    useUIStore();

  const { data: appState } = useAppState();
  const { data: dataDirPath } = useDataDirPath(appState?.pubky !== null);
  const forceSyncMutation = useForceSync();
  const snapshotMutation = useCreateSnapshot();
  const backupCloseMutation = useBackupControllerClose();

  const pubky = appState?.pubky ?? null;
  const isSyncing = appState?.is_syncing ?? false;
  const nextSyncTime = appState?.next_sync_time ?? 0;
  const dataSize = appState?.data_dir_size ?? 0;
  const backupControllerError = appState?.backup_controller_error ?? null;

  const lastSyncTime = useLastSyncTime(nextSyncTime, isSyncing);
  const countdownText = useCountdown(nextSyncTime, isSyncing);

  // Snapshot message timeout ref
  const snapshotTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const handleBack = useCallback(async () => {
    try {
      await backupCloseMutation.mutateAsync();
    } catch (error: unknown) {
      // Intentionally not using handleBackendError here - we want to navigate
      // back regardless of whether the controller was already stopped
      console.error("Backup controller already stopped:", error);
    }
    setScreen("startup");
  }, [backupCloseMutation, setScreen]);

  // Handle backup controller errors
  useEffect(() => {
    if (backupControllerError !== null) {
      handleBackendError(backupControllerError);
      void handleBack();
    }
  }, [backupControllerError, handleBack]);

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
        console.error("Failed to copy pubky:", err);
        alert("Failed to copy to clipboard");
      });
  };

  const handleForceSync = async () => {
    try {
      await forceSyncMutation.mutateAsync();
      console.log("Force sync triggered");
    } catch (error: unknown) {
      console.error("Force sync failed:", error);
    }
  };

  const handleSnapshot = async () => {
    try {
      const snapshotPath = await snapshotMutation.mutateAsync();
      console.log("Snapshot created:", snapshotPath);
      showSnapshotMessage("snapshot-success");
    } catch (error: unknown) {
      console.error("Snapshot creation failed:", error);
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
      await openDataDir();
    } catch (error: unknown) {
      console.error("Failed to open data directory:", error);
    }
  };

  // Determine status badge state
  const getStatusBadgeStatus = () => {
    if (statusMessageMode === "snapshot-success") return "snapshot";
    if (isSyncing) return "syncing";
    return "synced";
  };

  // Determine sync message state
  const getSyncMessageStatus = () => {
    if (statusMessageMode === "snapshot-success") return "snapshot-success";
    if (statusMessageMode === "snapshot-error") return "error";
    if (isSyncing) return "syncing";
    return "synced";
  };

  // Determine sync message text
  const getSyncMessageText = () => {
    if (statusMessageMode === "snapshot-success") return "Snapshot created";
    if (statusMessageMode === "snapshot-error")
      return "Failed to create snapshot";
    if (isSyncing) return "Syncing data...";
    return countdownText;
  };

  return (
    <main className="flex flex-col items-center justify-start text-center relative">
      <div
        className={cn(
          "w-[360px] min-h-[380px] flex flex-col justify-center items-stretch",
          "p-4 pb-5 px-5 gap-3",
          "bg-surface-dark border border-border rounded-lg",
          "shadow-[0_8px_10px_rgba(5,5,10,0.25),0_20px_25px_rgba(5,5,10,0.5)]",
        )}
      >
        {/* Header */}
        <DashboardHeader
          pubkyDisplay={displayPubky(pubky)}
          status={getStatusBadgeStatus()}
          onBack={() => void handleBack()}
          onCopy={handleCopy}
        />

        {/* Sync message banner */}
        <SyncMessage
          status={getSyncMessageStatus()}
          message={getSyncMessageText()}
        />

        {/* Info cards */}
        <div className="flex flex-col self-stretch p-3 bg-surface-light rounded-lg gap-0">
          <div className="flex justify-stretch items-stretch self-stretch">
            <InfoCard
              icon={<DatabaseIcon size={18} />}
              label="Backup Size"
              value={formatFileSize(dataSize)}
            />
            <InfoCard
              icon={<ClockIcon size={18} />}
              label="Last Sync"
              value={
                lastSyncTime !== null ? formatTimestamp(lastSyncTime) : "--"
              }
            />
          </div>
          <InfoCard
            icon={<FolderIcon size={18} />}
            label="Backup Location"
            value={dataDirPath ?? "--"}
            fullWidth
            action={
              <IconButton
                variant="inline"
                onClick={() => void handleOpenDataDir()}
                title="Open data directory"
              >
                <ExternalLinkIcon size={16} />
              </IconButton>
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
      </div>
    </main>
  );
}
