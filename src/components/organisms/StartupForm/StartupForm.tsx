import { useEffect, useCallback } from "react";
import { useShallow } from "zustand/react/shallow";
import * as Atoms from "@/components/atoms";
import * as Molecules from "@/components/molecules";
import * as Stores from "@/stores";
import * as Hooks from "@/hooks";
import * as Utils from "@/utils";
import { cn, Logger } from "@/lib";

export function StartupForm() {
  const { pubkyInputValue, hasAutoLoaded } = Stores.useUIStore(
    useShallow((s) => ({
      pubkyInputValue: s.pubkyInputValue,
      hasAutoLoaded: s.hasAutoLoaded,
    })),
  );
  const { setPubkyInputValue, setScreen, setHasAutoLoaded } =
    Stores.useUIStore.getState();
  const { addKey, isPending: isLoading } = Hooks.useAddKey();
  const keys = Hooks.useKeys();
  const { lastPubky } = Hooks.useLastPubky();

  const hasValue = pubkyInputValue.trim().length > 0;

  const handleAddKey = useCallback(
    async (value: string) => {
      const trimmedValue = value.trim();
      if (!trimmedValue) return;

      try {
        await addKey({ pubkyValue: trimmedValue });
        setScreen("dashboard");
      } catch (error: unknown) {
        Logger.error("StartupForm", "Add key error", { error });
        Utils.handleBackendError(error);
      }
    },
    [addKey, setScreen],
  );

  // Auto-load last pubky on mount (only once per app session)
  useEffect(() => {
    if (lastPubky && !hasAutoLoaded) {
      setHasAutoLoaded(true);
      setPubkyInputValue(lastPubky);
      void handleAddKey(lastPubky);
    }
  }, [
    lastPubky,
    hasAutoLoaded,
    setHasAutoLoaded,
    setPubkyInputValue,
    handleAddKey,
  ]);

  const handleSubmit = () => {
    void handleAddKey(pubkyInputValue);
  };

  // Determine placeholder based on available keys
  const placeholder =
    keys.length > 0 ? "Enter your pubky..." : "g1b6wp8bhhxt...";

  return (
    <main className="flex flex-col items-center justify-start text-center relative">
      <Atoms.Card>
        {/* Header */}
        <div className="flex flex-col items-center gap-2 py-4">
          <Atoms.PubkyLogo />
          <h1 className="text-lg font-normal text-text-secondary m-0">
            Securely mirror your Pubky data.
          </h1>
        </div>

        {/* Form content */}
        <div className="flex flex-col gap-4">
          <Molecules.PubkyInput
            value={pubkyInputValue}
            onChange={setPubkyInputValue}
            suggestions={keys}
            placeholder={placeholder}
          />

          <Atoms.Button
            onClick={handleSubmit}
            disabled={!hasValue || isLoading}
            className={cn(
              !hasValue && "opacity-30",
              hasValue && !isLoading && "opacity-100",
              isLoading && "opacity-100 bg-surface-dark border-border",
            )}
          >
            <span className="text-sm font-bold text-pubky-purple">Backup</span>
            <Atoms.RefreshIcon
              className={cn(
                "w-4 h-4 text-pubky-purple",
                isLoading && "animate-spin",
              )}
              size={16}
            />
          </Atoms.Button>
        </div>
      </Atoms.Card>
    </main>
  );
}
