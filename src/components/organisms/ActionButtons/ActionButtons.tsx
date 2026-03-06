import * as Atoms from "@/components/atoms";
import { cn } from "@/lib/utils";

export interface ActionButtonsProps {
  isSyncing: boolean;
  isCreatingSnapshot: boolean;
  isForceSyncing: boolean;
  onSnapshot: () => void;
  onForceSync: () => void;
}

export function ActionButtons({
  isSyncing,
  isCreatingSnapshot,
  isForceSyncing,
  onSnapshot,
  onForceSync,
}: ActionButtonsProps) {
  const snapshotDisabled = isSyncing || isCreatingSnapshot;
  const forceSyncDisabled = isSyncing || isForceSyncing;

  return (
    <div className="flex flex-row justify-end items-center self-stretch gap-3">
      {/* Snapshot button */}
      <Atoms.Button
        variant="secondary"
        onClick={onSnapshot}
        disabled={snapshotDisabled}
        className={cn("flex-1", isCreatingSnapshot && "opacity-70")}
      >
        {isCreatingSnapshot ? (
          <Atoms.Spinner className="w-4 h-4 text-white" size={16} />
        ) : (
          <Atoms.SnapshotIcon className="w-4 h-4 text-white" size={16} />
        )}
        <span className="text-sm font-bold text-white whitespace-nowrap">
          Create Snapshot
        </span>
      </Atoms.Button>

      {/* Force Sync button */}
      <Atoms.Button
        variant="primary"
        onClick={onForceSync}
        disabled={forceSyncDisabled}
        className={cn(
          "flex-1",
          isForceSyncing && "bg-surface-dark border-border",
        )}
      >
        <span className="text-sm font-bold text-pubky-purple">Force Sync</span>
        <Atoms.RefreshIcon
          className={cn(
            "w-4 h-4 text-pubky-purple",
            isForceSyncing && "animate-spin",
          )}
          size={16}
        />
      </Atoms.Button>
    </div>
  );
}
