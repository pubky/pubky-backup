import { Fragment, useCallback, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import * as Atoms from "@/components/atoms";
import * as Molecules from "@/components/molecules";
import * as Hooks from "@/hooks";
import * as Stores from "@/stores";
import * as Utils from "@/utils";
import * as Services from "@/services";
import { cn, Logger } from "@/lib";
import { AddKeyForm } from "../AddKeyForm";
import { MirrorCard } from "../MirrorCard";
import type { AuthStatus } from "@/stores/uiStore";

type SignInAction = "sign-in" | "sign-out";

/** What the sign-in button of a pubky in this state says and does, if it has one. */
function describeSignInAction(
  auth: AuthStatus,
  isSelected: boolean,
): { label: string; action: SignInAction } | null {
  switch (auth.type) {
    case "SignedOut":
      return { label: "Sign in", action: "sign-in" };
    case "SignedIn":
      // Only the selected pubky offers it; elsewhere the badge is enough
      return isSelected ? { label: "Sign out", action: "sign-out" } : null;
    case "SessionExpired":
      return { label: "Sign in again", action: "sign-in" };
    case "SignInFailed":
      return { label: "Try again", action: "sign-in" };
    case "AwaitingApproval":
      // The approval pop-up is open and has its own Cancel
      return null;
  }
}

interface KeyItemProps {
  pubky: string;
  isSelected: boolean;
  onSelect: () => void;
  onRemove: () => void;
}

function KeyItem({ pubky, isSelected, onSelect, onRemove }: KeyItemProps) {
  const coverage = Hooks.useBackupCoverage(pubky);
  const auth =
    Stores.useUIStore((s) => s.keyStates[pubky]?.auth) ?? Utils.SIGNED_OUT;
  const isMirrored = Stores.useUIStore(
    (s) => s.keyStates[pubky]?.mirror.type === "Mirroring",
  );
  const hasError = Stores.useUIStore(
    (s) => s.keyStates[pubky]?.status.type === "Error",
  );
  const signIn = Hooks.useSignIn();
  const signInAction = describeSignInAction(auth, isSelected);

  /** Run a sign-in command, showing its error to the user if it fails. */
  const attempt = async (
    what: string,
    command: (pubky: string) => Promise<void>,
  ) => {
    try {
      await command(pubky);
    } catch (error: unknown) {
      Logger.error("KeysPage", `Failed to ${what}`, { error });
      Utils.handleBackendError(error);
    }
  };

  const handleSignInAction = async (action: SignInAction) => {
    if (action === "sign-in") {
      await attempt("sign in", signIn.startSignIn);
      return;
    }
    // Undoing a sign-out needs the phone, and the button is next to Remove
    await attempt("sign out", async (pubky) => {
      const confirmed = await ask(
        `Sign out of ${Utils.displayPubky(pubky)}? Its private data stops being backed up until you sign in again.`,
        {
          title: "Sign out",
          kind: "warning",
          okLabel: "Sign out",
          cancelLabel: "Cancel",
        },
      );
      if (confirmed) await signIn.signOut(pubky);
    });
  };

  return (
    <>
      <div
        role="button"
        tabIndex={0}
        onClick={onSelect}
        onKeyDown={(e) => {
          // Keys pressed on its buttons are theirs
          if (e.target !== e.currentTarget) return;
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onSelect();
          }
        }}
        className={cn(
          "flex flex-col w-full px-4 py-2.5 rounded-lg gap-1",
          "border transition-colors duration-200",
          "text-left cursor-pointer",
          isSelected
            ? "bg-pubky-purple/10 border-pubky-purple"
            : "bg-[rgba(5,5,10,0.1)] border-border hover:border-text-secondary",
        )}
      >
        <div className="flex items-center gap-4">
          <span className="text-text-light font-medium text-base flex-1 whitespace-nowrap">
            {Utils.displayPubky(pubky)}
          </span>
          {/* Shrinks the badge, not the buttons, when the row is too narrow */}
          <div className="flex items-center gap-3 min-w-0">
            {hasError && <Atoms.ErrorMarker />}
            <Atoms.CoverageBadge
              coverage={coverage}
              isMirrored={isMirrored}
              className="min-w-0"
            />
            {signInAction && (
              <Atoms.Button
                variant={
                  signInAction.action === "sign-in" ? "primary" : "secondary"
                }
                onClick={(e) => {
                  e.stopPropagation();
                  void handleSignInAction(signInAction.action);
                }}
                disabled={signIn.isPending}
                className="py-1.5 px-3 shrink-0"
              >
                <span
                  className={cn(
                    "text-xs whitespace-nowrap",
                    signInAction.action === "sign-in"
                      ? "font-bold text-pubky-purple"
                      : "font-medium text-white",
                  )}
                >
                  {signInAction.label}
                </span>
              </Atoms.Button>
            )}
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                onRemove();
              }}
              title="Remove pubky"
              className="flex items-center justify-center shrink-0 w-8 h-8 rounded-full bg-surface-light shadow-[0_1px_2px_rgba(5,5,10,0.2)] cursor-pointer hover:opacity-80 transition-opacity"
            >
              <Atoms.TrashIcon size={16} />
            </button>
          </div>
        </div>
        {auth.type === "SignInFailed" && (
          <span className="text-xs text-pubky-red break-words">
            Sign-in failed: {auth.message}
          </span>
        )}
      </div>
      {/* Outside the row, so clicks in the pop-up don't select the pubky */}
      {auth.type === "AwaitingApproval" && (
        <Molecules.SignInApproval
          authorizationUrl={auth.authorization_url}
          title={`Sign in ${Utils.displayPubky(pubky)}`}
          instructions="Scan with Pubky Ring and approve to back up this pubky's private data."
          onCancel={() =>
            void attempt("cancel the sign-in", signIn.cancelSignIn)
          }
          cancelDisabled={signIn.isPending}
        />
      )}
    </>
  );
}

export function KeysPage() {
  const [showAddInput, setShowAddInput] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);

  const keys = Hooks.useKeys();
  const lastPubky = Stores.useUIStore((s) => s.lastPubky);
  const setNavDisabled = Stores.useUIStore((s) => s.setNavDisabled);
  const { setLastPubky } = Hooks.useSetLastPubky();

  // lastPubky is already normalized (z32 without prefix) from the backend
  const currentPubky = lastPubky;

  const handleRemove = async (pubky: string) => {
    try {
      const confirmed = await ask(
        `Stop backing up ${Utils.displayPubky(pubky)}? Its backed-up files stay on this computer.`,
        {
          title: "Remove pubky",
          kind: "warning",
          okLabel: "Remove",
          cancelLabel: "Cancel",
        },
      );
      if (!confirmed) return;

      setIsDeleting(true);
      setNavDisabled(true);
      await Services.removeKey(pubky);

      // If we removed the currently viewed key, select another one or go to startup screen
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
    } finally {
      setIsDeleting(false);
      setNavDisabled(false);
    }
  };

  const handleSelect = async (pubky: string) => {
    if (pubky === currentPubky) return;

    try {
      await setLastPubky(pubky);
    } catch (error: unknown) {
      Logger.error("KeysPage", "Failed to switch key", { error });
      Utils.handleBackendError(error);
    }
  };

  const handleAdded = useCallback(() => {
    setShowAddInput(false);
    Stores.useUIStore.getState().setPage("sync");
  }, []);

  if (showAddInput) {
    return (
      <div className="flex flex-col gap-6 self-stretch">
        <div className="flex items-start gap-4 text-left">
          <div className="flex flex-col gap-1 flex-1">
            <h2 className="text-xl font-bold text-white m-0">Add a pubky</h2>
            <p className="text-base font-medium text-text-light m-0">
              It is backed up alongside your other pubkys.
            </p>
          </div>
          <Atoms.Button
            variant="secondary"
            onClick={() => setShowAddInput(false)}
            className="py-2 px-4 shrink-0"
          >
            <span className="text-white text-sm font-medium">Cancel</span>
          </Atoms.Button>
        </div>
        <AddKeyForm onAdded={handleAdded} placeholder="Enter pubky to add..." />
      </div>
    );
  }

  return (
    <div className="flex flex-1 min-h-0 flex-col gap-6 self-stretch">
      {/* Header */}
      <div className="flex flex-col gap-1 text-left">
        <h2 className="text-xl font-bold text-white m-0">Manage pubkys</h2>
        <p className="text-base font-medium text-text-light m-0">
          Select a pubky to manage its backup.
        </p>
      </div>

      {/* Keys list */}
      {isDeleting ? (
        <div className="flex items-center bg-[#303034] rounded-lg px-6 py-[18px] gap-6">
          <span className="text-sm font-bold leading-normal text-white">
            Deleting key data...
          </span>
          <div className="flex-1 h-2 rounded-full bg-black/80 overflow-hidden">
            <div className="h-full rounded-full bg-pubky-purple animate-progress-indeterminate" />
          </div>
        </div>
      ) : (
        <>
          <div className="flex flex-1 min-h-0 flex-col gap-2">
            <span className="text-xs font-medium text-text-secondary uppercase tracking-widest">
              Your pubkys ({keys.length})
            </span>
            {/* Scrolls on its own, so the header and the add button stay put */}
            <div className="flex flex-1 min-h-0 flex-col gap-2 overflow-y-auto pr-2">
              {keys.map((pubky) => (
                <Fragment key={pubky}>
                  <KeyItem
                    pubky={pubky}
                    isSelected={pubky === currentPubky}
                    onSelect={() => handleSelect(pubky)}
                    onRemove={() => handleRemove(pubky)}
                  />
                  {/* Mirroring, under the pubky it acts on */}
                  {pubky === currentPubky && <MirrorCard pubky={pubky} />}
                </Fragment>
              ))}
            </div>
          </div>

          <Atoms.Button
            variant="secondary"
            onClick={() => setShowAddInput(true)}
            className="w-full shrink-0"
          >
            <span className="text-text-light text-sm font-bold">
              Add another pubky
            </span>
          </Atoms.Button>
        </>
      )}
    </div>
  );
}
