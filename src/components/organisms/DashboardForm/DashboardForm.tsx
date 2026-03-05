import { useEffect, useRef, useMemo } from "react";
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
  const { showToast, setStatusMessageMode } = Stores.useUIStore.getState();

  const appState = Hooks.useAppState();
  const { dataDirPath } = Hooks.useDataDirPath();
  const { forceSync, isPending: isForceSyncing } = Hooks.useForceSync();
  const { createSnapshotFn, isPending: isCreatingSnapshot } = Hooks.useCreateSnapshot();
  const { openDir } = Hooks.useOpenDataDir();
  const keys = Hooks.useKeys();
  const { setViewedPubky } = Hooks.useSetViewedPubky();

  const pubky = appState.pubky;
  const isSyncing = appState.is_syncing;
  const nextSyncTime = appState.next_sync_time;
  const dataSize = appState.data_dir_size;

  const lastSyncTime = Hooks.useLastSyncTime(nextSyncTime, isSyncing);
  const countdownText = Hooks.useCountdown(nextSyncTime, isSyncing);

  // Snapshot message timeout ref
  const snapshotTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

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
    if (pubky === null) return;
    try {
      await forceSync(pubky);
      Logger.debug("DashboardForm", "Force sync triggered");
    } catch (error: unknown) {
      Logger.error("DashboardForm", "Force sync failed", { error });
    }
  };

  const handleSnapshot = async () => {
    if (pubky === null) return;
    try {
      const snapshotPath = await createSnapshotFn(pubky);
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
      await openDir();
    } catch (error: unknown) {
      Logger.error("DashboardForm", "Failed to open data directory", { error });
    }
  };

  const handleSelectKey = (selectedPubky: string) => {
    void setViewedPubky(selectedPubky);
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
        onCopy={handleCopy}
        keys={keys.map((k) => ({ pubky: k, isSelected: k === pubky }))}
        onSelectKey={handleSelectKey}
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
        isCreatingSnapshot={isCreatingSnapshot}
        isForceSyncing={isForceSyncing}
        onSnapshot={() => void handleSnapshot()}
        onForceSync={() => void handleForceSync()}
      />
    </>
  );
}
