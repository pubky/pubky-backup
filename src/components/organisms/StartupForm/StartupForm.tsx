import { useCallback } from "react";
import * as Atoms from "@/components/atoms";
import * as Stores from "@/stores";
import * as Hooks from "@/hooks";
import { AddKeyForm } from "../AddKeyForm";

export function StartupForm() {
  const keys = Hooks.useKeys();
  const { isResuming } = Hooks.useResumeLastPubky();

  const handleAdded = useCallback(() => {
    // Show the new pubky's sync page, whatever page the dashboard was on before
    const { setPage, setScreen } = Stores.useUIStore.getState();
    setPage("sync");
    setScreen("dashboard");
  }, []);

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

          {isResuming ? (
            <div className="flex items-center justify-center gap-3 py-8 text-text-secondary">
              <Atoms.Spinner />
              <span className="text-sm">Opening your backup...</span>
            </div>
          ) : (
            <AddKeyForm
              onAdded={handleAdded}
              suggestions={keys}
              placeholder={placeholder}
            />
          )}
        </div>
      </Atoms.Card>
    </main>
  );
}
