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
      <button
        type="button"
        onClick={onSnapshot}
        disabled={snapshotDisabled}
        className={cn(
          "flex justify-center items-center gap-2",
          "py-5 px-8 rounded-full flex-1",
          "bg-button-hover-bg border border-border",
          "shadow-[0_1px_2px_rgba(5,5,10,0.2)]",
          "cursor-pointer transition-all duration-200",
          "hover:bg-button-active-bg",
          "disabled:opacity-50 disabled:cursor-not-allowed",
          isCreatingSnapshot && "opacity-70",
        )}
      >
        {isCreatingSnapshot ? (
          <Atoms.Spinner className="w-4 h-4 text-white" size={16} />
        ) : (
          <Atoms.SnapshotIcon className="w-4 h-4 text-white" size={16} />
        )}
        <span className="text-sm font-bold text-white">Create Snapshot</span>
      </button>

      {/* Force Sync button */}
      <button
        type="button"
        onClick={onForceSync}
        disabled={forceSyncDisabled}
        className={cn(
          "flex justify-center items-center gap-2",
          "py-5 px-8 rounded-full self-stretch",
          "bg-pubky-purple/15 border border-pubky-purple",
          "shadow-[0_1px_2px_rgba(5,5,10,0.2)]",
          "cursor-pointer transition-all duration-200",
          "hover:opacity-80",
          "disabled:opacity-30 disabled:cursor-not-allowed",
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
      </button>
    </div>
  );
}
