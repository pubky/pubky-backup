import { useState } from "react";
import * as Atoms from "@/components/atoms";
import * as Molecules from "@/components/molecules";
import * as Hooks from "@/hooks";
import * as Stores from "@/stores";
import * as Utils from "@/utils";
import * as Services from "@/services";
import { cn, Logger } from "@/lib";

interface KeyItemProps {
  pubky: string;
  isSelected: boolean;
  onSelect: () => void;
  onRemove: () => void;
}

function KeyItem({ pubky, isSelected, onSelect, onRemove }: KeyItemProps) {
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onSelect}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onSelect();
        }
      }}
      className={cn(
        "flex items-center w-full px-5 py-4 rounded-lg gap-4",
        "border transition-colors duration-200",
        "text-left cursor-pointer",
        isSelected
          ? "bg-pubky-purple/10 border-pubky-purple"
          : "bg-[rgba(5,5,10,0.1)] border-border hover:border-text-secondary",
      )}
    >
      <span className="text-text-light font-medium text-base flex-1">
        {Utils.displayPubky(pubky)}
      </span>
      <div className="flex items-center gap-4">
        {isSelected && (
          <Atoms.CheckIcon size={16} className="text-text-light" />
        )}
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
          title="Remove pubky"
          className="flex items-center justify-center w-9 h-9 rounded-full bg-surface-light shadow-[0_1px_2px_rgba(5,5,10,0.2)] cursor-pointer hover:opacity-80 transition-opacity"
        >
          <Atoms.TrashIcon size={16} />
        </button>
      </div>
    </div>
  );
}

export function KeysPage() {
  const [showAddInput, setShowAddInput] = useState(false);
  const [newPubkyValue, setNewPubkyValue] = useState("");

  const keys = Hooks.useKeys();
  const lastPubky = Stores.useUIStore((s) => s.lastPubky);
  const { setLastPubky } = Hooks.useSetLastPubky();
  const { addKey, isPending: isAddingKey } = Hooks.useAddKey();
  const { setPage } = Stores.useUIStore.getState();

  // lastPubky is already normalized (z32 without prefix) from the backend
  const currentPubky = lastPubky;

  const handleRemove = async (pubky: string) => {
    try {
      await Services.deleteKey(pubky);

      // If we deleted the currently viewed key, select another one or go to startup screen
      if (pubky === currentPubky) {
        const remainingKeys = keys.filter((k) => k !== pubky);
        const nextKey = remainingKeys[0];
        if (nextKey) {
          await setLastPubky(nextKey);
        } else {
          // No keys left, clear state and return to startup screen
          Stores.useUIStore.getState().setLastPubky(null);
          Stores.useUIStore.getState().setScreen("startup");
        }
      }

      // Remove from UI state after handling viewed key transition
      Stores.useUIStore.getState().removeKeyState(pubky);
    } catch (error: unknown) {
      Logger.error("KeysPage", "Failed to delete pubky", { error });
      Utils.handleBackendError(error);
    }
  };

  const handleSelect = async (pubky: string) => {
    if (pubky === currentPubky) {
      setPage("sync");
      return;
    }

    try {
      await setLastPubky(pubky);
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
      await addKey({ pubkyValue: trimmedValue });
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
    <div className="flex flex-col gap-6 self-stretch">
      {/* Header */}
      <div className="flex flex-col gap-1 text-left">
        <h2 className="text-xl font-bold text-white m-0">Manage keys</h2>
        <p className="text-base font-medium text-text-light m-0">
          Select a pubky to see its backup activity and status.
          {"\n"}Don't worry, all your keys will be synced in the background.
        </p>
      </div>

      {/* Keys list */}
      <div className="flex flex-col gap-2">
        <span className="text-xs font-medium text-text-secondary uppercase tracking-widest">
          Your pubkys ({keys.length})
        </span>
        <div className="flex flex-col gap-2">
          {keys.map((pubky) => (
            <KeyItem
              key={pubky}
              pubky={pubky}
              isSelected={pubky === currentPubky}
              onSelect={() => handleSelect(pubky)}
              onRemove={() => handleRemove(pubky)}
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
        >
          <span className="text-text-light text-sm font-bold">
            Add another pubky
          </span>
        </Atoms.Button>
      )}
    </div>
  );
}
