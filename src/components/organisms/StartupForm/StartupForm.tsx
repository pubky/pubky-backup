import { useEffect, useCallback } from "react";
import { useShallow } from "zustand/react/shallow";
import * as Atoms from "@/components/atoms";
import * as Stores from "@/stores";
import * as Hooks from "@/hooks";
import * as Utils from "@/utils";
import { Logger } from "@/lib";
import { AddKeyForm } from "../AddKeyForm";

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

  const handleSignedIn = useCallback(() => {
    setScreen("dashboard");
  }, [setScreen]);

  // Determine placeholder based on available keys
  const placeholder =
    keys.length > 0 ? "Enter your pubky..." : "g1b6wp8bhhxt...";

  return (
    <main className="flex flex-col items-center justify-start text-center relative">
      <Atoms.Card>
        <div className="flex flex-col items-stretch gap-6">
          {/* Header */}
          <div className="flex flex-col items-center gap-2 pt-4">
            <Atoms.PubkyLogo />
            <h1 className="text-xl font-normal text-text-secondary m-0">
              Securely mirror your Pubky data.
            </h1>
          </div>

          <AddKeyForm
            onSignedIn={handleSignedIn}
            pubkyValue={pubkyInputValue}
            onPubkyChange={setPubkyInputValue}
            onSubmitPubky={(value) => void handleAddKey(value)}
            isAddingPubky={isLoading}
            suggestions={keys}
            placeholder={placeholder}
          />
        </div>
      </Atoms.Card>
    </main>
  );
}
