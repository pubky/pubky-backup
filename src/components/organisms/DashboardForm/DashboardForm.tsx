import * as Atoms from "@/components/atoms";
import * as Molecules from "@/components/molecules";
import { ActionButtons } from "../ActionButtons";
import * as Hooks from "@/hooks";
import * as Utils from "@/utils";

export function DashboardForm() {
  const {
    pubky,
    isSyncing,
    dataSize,
    lastSyncTime,
    dataDirPath,
    keys,
    statusInfo,
  } = Hooks.useDashboardState();

  const {
    handleCopy,
    handleForceSync,
    handleSnapshot,
    handleOpenDataDir,
    handleSelectKey,
    isForceSyncing,
    isCreatingSnapshot,
  } = Hooks.useDashboardActions();

  return (
    <>
      {/* Header */}
      <Molecules.DashboardHeader
        pubkyDisplay={Utils.displayPubky(pubky)}
        status={statusInfo.badge}
        onCopy={() => handleCopy(pubky)}
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
        onSnapshot={() => void handleSnapshot(pubky)}
        onForceSync={() => void handleForceSync(pubky)}
      />
    </>
  );
}
