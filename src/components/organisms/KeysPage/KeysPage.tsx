import { useState } from "react";
import * as Atoms from "@/components/atoms";
import * as Molecules from "@/components/molecules";
import * as Hooks from "@/hooks";
import * as Stores from "@/stores";
import * as Utils from "@/utils";
import { cn, Logger } from "@/lib";

interface KeyItemProps {
  pubky: string;
  isSelected: boolean;
  onSelect: () => void;
  onCopy: () => void;
}

function KeyItem({ pubky, isSelected, onSelect, onCopy }: KeyItemProps) {
  return (
    <button
      type="button"
      onClick={onSelect}
      className={cn(
        "flex items-center justify-between w-full px-4 py-3 rounded-lg",
        "border transition-colors duration-200",
        "text-left cursor-pointer",
        isSelected
          ? "bg-pubky-purple/15 border-pubky-purple"
          : "bg-surface-light border-transparent hover:border-border"
      )}
    >
      <span className="text-white font-medium">
        {Utils.displayPubky(pubky)}
      </span>
      <div className="flex items-center gap-2">
        {isSelected && (
          <Atoms.CheckIcon size={16} className="text-pubky-purple" />
        )}
        <Atoms.IconButton
          variant="inline"
          onClick={(e) => {
            e.stopPropagation();
            onCopy();
          }}
          title="Copy pubky"
        >
          <Atoms.CopyIcon size={16} />
        </Atoms.IconButton>
      </div>
    </button>
  );
}

export function KeysPage() {
  const [showAddInput, setShowAddInput] = useState(false);
  const [newPubkyValue, setNewPubkyValue] = useState("");

  const { data: keys = [] } = Hooks.useKeys();
  const { data: appState } = Hooks.useAppState();
  const setViewedPubkyMutation = Hooks.useSetViewedPubky();
  const addKeyMutation = Hooks.useAddKey();
  const { showToast, setPage } = Stores.useUIStore.getState();

  // Normalize pubky by stripping the "pubky" prefix if present
  const rawPubky = appState?.pubky ?? null;
  const currentPubky = rawPubky ? Utils.stripPubkyPrefix(rawPubky) : null;
  const isAddingKey = addKeyMutation.isPending;
  const isSwitchingKey = setViewedPubkyMutation.isPending;

  const handleCopy = (pubky: string) => {
    navigator.clipboard
      .writeText(pubky)
      .then(() => {
        showToast(pubky);
      })
      .catch((err: unknown) => {
        Logger.error("KeysPage", "Failed to copy pubky", { error: err });
      });
  };

  const handleSelect = async (pubky: string) => {
    if (pubky === currentPubky) {
      setPage("sync");
      return;
    }

    try {
      await setViewedPubkyMutation.mutateAsync(pubky);
      setPage("sync");
    } catch (error: unknown) {
      Logger.error("KeysPage", "Failed to switch key", { error });
      Utils.handleBackendError(error);
    }
  };

  const handleAddPubky = () => {
    setShowAddInput(true);
  };

  const handleSubmitNewPubky = async () => {
    const trimmedValue = newPubkyValue.trim();
    if (!trimmedValue) return;

    try {
      await addKeyMutation.mutateAsync({ pubkyValue: trimmedValue });
      setNewPubkyValue("");
      setShowAddInput(false);
      setPage("sync");
    } catch (error: unknown) {
      Logger.error("KeysPage", "Failed to add new pubky", { error });
      Utils.handleBackendError(error);
    }
  };

  const handleCancelAdd = () => {
    setNewPubkyValue("");
    setShowAddInput(false);
  };

  return (
    <div className="flex flex-col gap-4 self-stretch">
      {/* Header */}
      <div className="text-left">
        <h2 className="text-lg font-semibold text-white mb-1">Manage keys</h2>
        <p className="text-sm text-text-secondary">
          Select a pubky to see its backup activity and status.
        </p>
        <p className="text-sm text-text-secondary">
          Don't worry, all your keys will be synced in the background.
        </p>
      </div>

      {/* Keys list */}
      <div className="flex flex-col gap-2">
        <span className="text-xs text-text-secondary uppercase tracking-wider">
          Your pubkys ({keys.length})
        </span>
        <div className="flex flex-col gap-2">
          {keys.map((pubky) => (
            <KeyItem
              key={pubky}
              pubky={pubky}
              isSelected={pubky === currentPubky}
              onSelect={() => handleSelect(pubky)}
              onCopy={() => handleCopy(pubky)}
            />
          ))}
        </div>
      </div>

      {/* Add pubky section */}
      {showAddInput ? (
        <div className="flex flex-col gap-2">
          <Molecules.PubkyInput
            value={newPubkyValue}
            onChange={setNewPubkyValue}
            placeholder="Enter pubky to add..."
          />
          <div className="flex gap-2">
            <Atoms.Button
              variant="secondary"
              onClick={handleCancelAdd}
              className="flex-1"
              disabled={isAddingKey}
            >
              <span className="text-white text-sm font-medium">Cancel</span>
            </Atoms.Button>
            <Atoms.Button
              onClick={() => void handleSubmitNewPubky()}
              className="flex-1"
              disabled={!newPubkyValue.trim() || isAddingKey}
            >
              <span className="text-sm font-bold text-pubky-purple">
                {isAddingKey ? "Adding..." : "Add"}
              </span>
            </Atoms.Button>
          </div>
        </div>
      ) : (
        <Atoms.Button
          variant="secondary"
          onClick={handleAddPubky}
          className="w-full"
          disabled={isSwitchingKey}
        >
          <span className="text-white text-sm font-medium">Add another pubky</span>
        </Atoms.Button>
      )}
    </div>
  );
}
