import { useState } from "react";
import * as Atoms from "@/components/atoms";
import * as Molecules from "@/components/molecules";
import * as Hooks from "@/hooks";
import * as Utils from "@/utils";
import { cn, Logger } from "@/lib";

export interface AddKeyFormProps {
  /** Called with the pubky once it is added, either way */
  onAdded: (pubky: string) => void;
  suggestions?: string[];
  placeholder?: string;
}

/**
 * Adds a pubky to back up, either by signing in with Pubky Ring, which backs
 * up public and private data, or by entering the pubky, which backs up its
 * public data only.
 */
export function AddKeyForm({
  onAdded,
  suggestions,
  placeholder = "Paste a pubky...",
}: AddKeyFormProps) {
  const [pubkyValue, setPubkyValue] = useState("");
  const { addKey, isPending: isAddingPubky } = Hooks.useAddKey();
  const hasValue = pubkyValue.trim().length > 0;

  const handleSubmitPubky = async () => {
    try {
      onAdded(await addKey({ pubkyValue: pubkyValue.trim() }));
    } catch (error: unknown) {
      Logger.error("AddKeyForm", "Failed to add pubky", { error });
      Utils.handleBackendError(error);
    }
  };

  return (
    <div className="flex flex-col gap-6 text-left">
      {/* Signing in: public and private data */}
      <section className="flex flex-col gap-3 p-6 bg-surface-light rounded-lg">
        <div className="flex flex-col gap-1">
          <h3 className="flex items-center gap-2 text-base font-bold text-white m-0">
            <Atoms.LockIcon size={16} className="text-pubky-green" />
            Public and private data
          </h3>
          <p className="text-sm text-text-secondary m-0">
            Sign in with Pubky Ring. It only grants read access, and your key
            never leaves Pubky Ring.
          </p>
        </div>
        <SignInSection onSignedIn={onAdded} disabled={isAddingPubky} />
      </section>

      <div className="flex items-center gap-3" aria-hidden>
        <div className="flex-1 h-px bg-border-dashed" />
        <span className="text-xs font-medium uppercase tracking-widest text-text-secondary">
          or
        </span>
        <div className="flex-1 h-px bg-border-dashed" />
      </div>

      {/* Entering a pubky: public data only */}
      <section className="flex flex-col gap-3">
        <div className="flex flex-col gap-1">
          <h3 className="flex items-center gap-2 text-base font-bold text-white m-0">
            <Atoms.GlobeIcon size={16} className="text-text-light" />
            Public data only
          </h3>
          <p className="text-sm text-text-secondary m-0">
            Enter any pubky. You can sign in to add its private data later.
          </p>
        </div>
        <Molecules.PubkyInput
          value={pubkyValue}
          onChange={setPubkyValue}
          suggestions={suggestions}
          placeholder={placeholder}
        />
        <Atoms.Button
          variant="secondary"
          onClick={() => void handleSubmitPubky()}
          disabled={!hasValue || isAddingPubky}
        >
          <span className="text-sm font-bold text-white">
            Back up public data
          </span>
          <Atoms.RefreshIcon
            className={cn(
              "w-4 h-4 text-white",
              isAddingPubky && "animate-spin",
            )}
            size={16}
          />
        </Atoms.Button>
      </section>
    </div>
  );
}

interface SignInSectionProps {
  onSignedIn: (pubky: string) => void;
  disabled: boolean;
}

/** The sign-in button, or the QR code to approve the sign-in with. */
function SignInSection({ onSignedIn, disabled }: SignInSectionProps) {
  const { state, start, cancel } = Hooks.useNewKeySignIn(onSignedIn);

  if (state.type === "AwaitingApproval") {
    return (
      <Molecules.SignInApproval
        authorizationUrl={state.authorization_url}
        instructions="Scan with Pubky Ring and approve. Backing up starts as soon as you do."
        action={
          <Atoms.Button
            variant="secondary"
            onClick={cancel}
            className="py-2 px-4"
          >
            <span className="text-white text-sm font-medium">Cancel</span>
          </Atoms.Button>
        }
      />
    );
  }

  const isStarting = state.type === "Starting";
  return (
    <div className="flex flex-col gap-2">
      {state.type === "Failed" && (
        <span role="alert" className="text-sm text-pubky-red">
          Sign-in failed: {state.message}
        </span>
      )}
      <Atoms.Button
        onClick={() => void start()}
        disabled={disabled || isStarting}
      >
        <span className="text-sm font-bold text-pubky-purple">
          {isStarting
            ? "Preparing sign-in..."
            : state.type === "Failed"
              ? "Try again"
              : "Sign in with Pubky Ring"}
        </span>
      </Atoms.Button>
    </div>
  );
}
