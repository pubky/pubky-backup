import { useState } from "react";
import * as Atoms from "@/components/atoms";
import * as Molecules from "@/components/molecules";
import * as Hooks from "@/hooks";
import * as Stores from "@/stores";
import * as Utils from "@/utils";
import type { MirrorStatus } from "@/stores/uiStore";
import { cn, Logger } from "@/lib";

type MirrorAction = "add" | "stop" | "approve-again" | "retry";

interface MirrorCoverage {
  description: string;
  tone: "neutral" | "warning";
  /** What the user can do next, the main step first */
  actions: { label: string; action: MirrorAction }[];
}

/** What to tell the user about their mirror, and what they can do next. */
function describeMirrorCoverage(mirror: MirrorStatus): MirrorCoverage {
  switch (mirror.type) {
    case "NotMirrored":
      return {
        description:
          "Not mirrored. Copy your backup to a second homeserver as well.",
        tone: "neutral",
        actions: [{ label: "Add mirror", action: "add" }],
      };
    case "AwaitingApproval":
      return {
        description: "Waiting for approval in Pubky Ring.",
        tone: "neutral",
        // The approval pop-up has its own Cancel
        actions: [],
      };
    case "Mirroring":
      return {
        description: `Copied to ${Utils.displayPubky(mirror.homeserver)} after every sync.`,
        tone: "neutral",
        actions: [{ label: "Stop", action: "stop" }],
      };
    case "SessionExpired":
      return {
        description: `Paused: ${Utils.displayPubky(mirror.homeserver)} no longer accepts the session. Approve again to resume.`,
        tone: "warning",
        actions: [
          { label: "Approve again", action: "approve-again" },
          { label: "Stop", action: "stop" },
        ],
      };
    case "Failing":
      return {
        description: `Copying to ${Utils.displayPubky(mirror.homeserver)} is failing: ${mirror.message}`,
        tone: "warning",
        actions: [{ label: "Stop", action: "stop" }],
      };
    case "SetupFailed":
      return {
        description: `Mirroring to ${Utils.displayPubky(mirror.homeserver)} failed: ${mirror.message}`,
        tone: "warning",
        actions: [{ label: "Try again", action: "retry" }],
      };
  }
}

interface ActionButtonProps {
  label: string;
  /** Primary buttons invite the next step; secondary ones undo or stop */
  isPrimary: boolean;
  onClick: () => void;
  disabled?: boolean;
}

function ActionButton({
  label,
  isPrimary,
  onClick,
  disabled,
}: ActionButtonProps) {
  return (
    <Atoms.Button
      variant={isPrimary ? "primary" : "secondary"}
      onClick={onClick}
      disabled={disabled}
      className="py-2 px-4 shrink-0"
    >
      <span
        className={cn(
          "text-sm",
          isPrimary ? "font-bold text-pubky-purple" : "font-medium text-white",
        )}
      >
        {label}
      </span>
    </Atoms.Button>
  );
}

interface MirrorFormProps {
  initialHomeserver: string;
  isPending: boolean;
  onSubmit: (homeserver: string) => void;
  onCancel: () => void;
}

/** Asks which homeserver to mirror to. */
function MirrorForm({
  initialHomeserver,
  isPending,
  onSubmit,
  onCancel,
}: MirrorFormProps) {
  const [homeserver, setHomeserver] = useState(initialHomeserver);
  const trimmed = homeserver.trim();

  return (
    <div className="flex flex-col gap-3">
      <span className="text-xs font-medium uppercase tracking-widest text-text-secondary">
        Mirror to another homeserver
      </span>
      <Molecules.PubkyInput
        value={homeserver}
        onChange={setHomeserver}
        placeholder="Homeserver key"
      />
      <span className="text-sm text-text-secondary">
        Your key must already have an account on that homeserver. Everything
        backed up is copied there, then every later change.
      </span>
      <div className="flex gap-2 self-end">
        <ActionButton label="Cancel" isPrimary={false} onClick={onCancel} />
        <ActionButton
          label="Request approval"
          isPrimary
          onClick={() => onSubmit(trimmed)}
          disabled={!trimmed || isPending}
        />
      </div>
    </div>
  );
}

export interface MirrorCardProps {
  pubky: string;
}

/**
 * Shows whether a pubky's backup is mirrored to a second homeserver, and lets
 * its owner set up or stop a mirror. Renders nothing in builds that don't
 * offer mirroring.
 */
export function MirrorCard({ pubky }: MirrorCardProps) {
  const mirror =
    Stores.useUIStore((s) => s.keyStates[pubky]?.mirror) ?? Utils.NOT_MIRRORED;
  const mirroring = Hooks.useMirror();
  const [mirrorForm, setMirrorForm] = useState<{ homeserver: string } | null>(
    null,
  );

  if (!mirroring.isEnabled) return null;

  const coverage = describeMirrorCoverage(mirror);

  /** Run a backend action, showing its error to the user if it fails. */
  const attempt = async (what: string, action: () => Promise<void>) => {
    try {
      await action();
      return true;
    } catch (error: unknown) {
      Logger.error("MirrorCard", `Failed to ${what}`, { error });
      Utils.handleBackendError(error);
      return false;
    }
  };

  const cancel = () =>
    attempt("cancel the mirror", () => mirroring.cancelMirror(pubky));

  const handleAction = (action: MirrorAction) => {
    const homeserver = mirror.type === "NotMirrored" ? "" : mirror.homeserver;
    switch (action) {
      case "add":
      case "retry":
        setMirrorForm({ homeserver });
        return;
      case "approve-again":
        void attempt("request the mirror again", () =>
          mirroring.startMirror(pubky, homeserver),
        );
        return;
      case "stop":
        void attempt("stop the mirror", () => mirroring.stopMirror(pubky));
        return;
    }
  };

  const handleSubmit = async (homeserver: string) => {
    // The form closes once the request is accepted; the QR code follows in the key's state
    if (
      await attempt("request the mirror", () =>
        mirroring.startMirror(pubky, homeserver),
      )
    ) {
      setMirrorForm(null);
    }
  };

  return (
    <div className="flex flex-col self-stretch gap-3 p-5 bg-surface-light rounded-lg text-left">
      {mirrorForm !== null ? (
        <MirrorForm
          initialHomeserver={mirrorForm.homeserver}
          isPending={mirroring.isPending}
          onSubmit={(homeserver) => void handleSubmit(homeserver)}
          onCancel={() => setMirrorForm(null)}
        />
      ) : (
        <div className="flex items-center gap-3">
          <div
            className={cn(
              "flex items-center justify-center size-5 shrink-0 rounded-full border",
              mirror.type === "Mirroring"
                ? "bg-status-synced-bg border-pubky-green text-pubky-green"
                : "border-border-dashed text-transparent",
            )}
          >
            <Atoms.CheckmarkIcon size={12} />
          </div>
          <div className="flex flex-col flex-1 min-w-0">
            <strong className="text-base font-bold text-white">Mirror</strong>
            <span
              className={cn(
                "text-sm break-words",
                coverage.tone === "warning"
                  ? "text-pubky-red"
                  : "text-text-secondary",
              )}
            >
              {coverage.description}
            </span>
          </div>
          {coverage.actions.length > 0 && (
            <div className="flex flex-col items-end gap-2 shrink-0">
              {coverage.actions.map(({ label, action }, i) => (
                <ActionButton
                  key={action}
                  label={label}
                  // Only the main step is highlighted, and stopping never is
                  isPrimary={i === 0 && action !== "stop"}
                  onClick={() => handleAction(action)}
                  disabled={mirroring.isPending}
                />
              ))}
            </div>
          )}
        </div>
      )}
      {mirror.type === "AwaitingApproval" && (
        <Molecules.SignInApproval
          authorizationUrl={mirror.authorization_url}
          title="Approve the mirror"
          instructions={`Scan with Pubky Ring and approve. This grants write access, used only to copy your backup to ${Utils.displayPubky(mirror.homeserver)}. Your key never leaves Pubky Ring.`}
          onCancel={() => void cancel()}
          cancelDisabled={mirroring.isPending}
        />
      )}
    </div>
  );
}
