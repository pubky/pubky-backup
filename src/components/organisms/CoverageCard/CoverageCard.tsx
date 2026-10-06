import { useState, type ReactNode } from "react";
import * as Atoms from "@/components/atoms";
import * as Molecules from "@/components/molecules";
import * as Hooks from "@/hooks";
import * as Stores from "@/stores";
import * as Utils from "@/utils";
import type { AuthStatus, MirrorStatus } from "@/stores/uiStore";
import { cn, Logger } from "@/lib";

type PrivateAction = "sign-in" | "cancel" | "sign-out";

interface PrivateCoverage {
  description: string;
  tone: "neutral" | "warning";
  actionLabel: string;
  action: PrivateAction;
}

/** What to tell the user about their private data, and what they can do next. */
function describePrivateCoverage(auth: AuthStatus): PrivateCoverage {
  switch (auth.type) {
    case "SignedOut":
      return {
        description: "Not backed up. Sign in with Pubky Ring to include it.",
        tone: "neutral",
        actionLabel: "Sign in",
        action: "sign-in",
      };
    case "AwaitingApproval":
      return {
        description:
          "Scan with Pubky Ring and approve. This only grants read access to your private data, and your key never leaves Pubky Ring.",
        tone: "neutral",
        actionLabel: "Cancel",
        action: "cancel",
      };
    case "SignedIn":
      return {
        description: "Backed up while you stay signed in.",
        tone: "neutral",
        actionLabel: "Sign out",
        action: "sign-out",
      };
    case "SessionExpired":
      return {
        description:
          "No longer backed up: your session expired. Sign in again to resume.",
        tone: "warning",
        actionLabel: "Sign in again",
        action: "sign-in",
      };
    case "SignInFailed":
      return {
        description: `Not backed up. Sign-in failed: ${auth.message}`,
        tone: "warning",
        actionLabel: "Try again",
        action: "sign-in",
      };
  }
}

type MirrorAction = "add" | "stop" | "approve-again" | "retry";

interface MirrorCoverage {
  description: string;
  tone: "neutral" | "warning";
  actionLabel: string;
  action: MirrorAction;
}

/** What to tell the user about their mirror, and what they can do next. */
function describeMirrorCoverage(mirror: MirrorStatus): MirrorCoverage {
  switch (mirror.type) {
    case "NotMirrored":
      return {
        description:
          "Not mirrored. Copy your backup to a second homeserver as well.",
        tone: "neutral",
        actionLabel: "Add mirror",
        action: "add",
      };
    case "AwaitingApproval":
      return {
        description: `Scan with Pubky Ring and approve. This grants write access, used only to copy your backup to ${Utils.displayPubky(mirror.homeserver)}. Your key never leaves Pubky Ring.`,
        tone: "neutral",
        actionLabel: "Cancel",
        action: "stop",
      };
    case "Mirroring":
      return {
        description: `Copied to ${Utils.displayPubky(mirror.homeserver)} after every sync.`,
        tone: "neutral",
        actionLabel: "Stop",
        action: "stop",
      };
    case "SessionExpired":
      return {
        description: `Paused: ${Utils.displayPubky(mirror.homeserver)} no longer accepts the session. Approve again to resume.`,
        tone: "warning",
        actionLabel: "Approve again",
        action: "approve-again",
      };
    case "Failing":
      return {
        description: `Copying to ${Utils.displayPubky(mirror.homeserver)} is failing: ${mirror.message}`,
        tone: "warning",
        actionLabel: "Stop",
        action: "stop",
      };
    case "SetupFailed":
      return {
        description: `Mirroring to ${Utils.displayPubky(mirror.homeserver)} failed: ${mirror.message}`,
        tone: "warning",
        actionLabel: "Try again",
        action: "retry",
      };
  }
}

interface CoverageRowProps {
  title: string;
  description: string;
  isCovered: boolean;
  tone?: "neutral" | "warning";
  action?: ReactNode;
}

function CoverageRow({
  title,
  description,
  isCovered,
  tone = "neutral",
  action,
}: CoverageRowProps) {
  return (
    <div className="flex items-center gap-3">
      <div
        className={cn(
          "flex items-center justify-center size-5 shrink-0 rounded-full border",
          isCovered
            ? "bg-status-synced-bg border-pubky-green text-pubky-green"
            : "border-border-dashed text-transparent",
        )}
      >
        <Atoms.CheckmarkIcon size={12} />
      </div>
      <div className="flex flex-col flex-1 min-w-0">
        <strong className="text-base font-bold text-white">{title}</strong>
        <span
          className={cn(
            "text-sm",
            tone === "warning" ? "text-pubky-red" : "text-text-secondary",
          )}
        >
          {description}
        </span>
      </div>
      {action}
    </div>
  );
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

interface CoverageCardFrameProps {
  title?: string;
  children: ReactNode;
}

function CoverageCardFrame({ title, children }: CoverageCardFrameProps) {
  return (
    // Padding matches the info cards above, which nest two p-3 containers
    <div className="flex flex-col self-stretch gap-3 p-6 bg-surface-light rounded-lg text-left">
      {title && (
        <span className="text-xs font-medium uppercase tracking-widest text-text-secondary">
          {title}
        </span>
      )}
      {children}
    </div>
  );
}

export interface CoverageCardProps {
  pubky: string;
}

/**
 * Shows whether a pubky's private data is backed up and whether its backup is
 * mirrored to a second homeserver, and lets its owner sign in, sign out, and
 * set up or stop a mirror. Public data is always backed up, which the
 * dashboard header shows.
 */
export function CoverageCard({ pubky }: CoverageCardProps) {
  const auth =
    Stores.useUIStore((s) => s.keyStates[pubky]?.auth) ?? Utils.SIGNED_OUT;
  const mirror =
    Stores.useUIStore((s) => s.keyStates[pubky]?.mirror) ?? Utils.NOT_MIRRORED;
  const signIn = Hooks.useSignIn();
  const mirroring = Hooks.useMirror();
  const isPending = signIn.isPending || mirroring.isPending;
  const [mirrorForm, setMirrorForm] = useState<{ homeserver: string } | null>(
    null,
  );

  const privateCoverage = describePrivateCoverage(auth);
  const mirrorCoverage = describeMirrorCoverage(mirror);

  /** Run a backend action, showing its error to the user if it fails. */
  const attempt = async (what: string, action: () => Promise<void>) => {
    try {
      await action();
      return true;
    } catch (error: unknown) {
      Logger.error("CoverageCard", `Failed to ${what}`, { error });
      Utils.handleBackendError(error);
      return false;
    }
  };

  const handlePrivateAction = () => {
    const actions: Record<PrivateAction, (pubky: string) => Promise<void>> = {
      "sign-in": signIn.startSignIn,
      cancel: signIn.cancelSignIn,
      "sign-out": signIn.signOut,
    };
    void attempt(privateCoverage.action, () =>
      actions[privateCoverage.action](pubky),
    );
  };

  const handleMirrorAction = () => {
    const homeserver = mirror.type === "NotMirrored" ? "" : mirror.homeserver;
    switch (mirrorCoverage.action) {
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

  const handleMirrorSubmit = async (homeserver: string) => {
    // The form closes once the request is accepted; the QR code follows in the key's state
    if (
      await attempt("request the mirror", () =>
        mirroring.startMirror(pubky, homeserver),
      )
    ) {
      setMirrorForm(null);
    }
  };

  // While a request awaits approval the card shows only that, which keeps the
  // dashboard within the window's maximum height
  if (auth.type === "AwaitingApproval") {
    return (
      <CoverageCardFrame title="Sign in to back up private data">
        <Molecules.SignInApproval
          authorizationUrl={auth.authorization_url}
          instructions={privateCoverage.description}
          action={
            <ActionButton
              label="Cancel"
              isPrimary={false}
              onClick={() =>
                void attempt("cancel", () => signIn.cancelSignIn(pubky))
              }
              disabled={isPending}
            />
          }
        />
      </CoverageCardFrame>
    );
  }
  if (mirror.type === "AwaitingApproval") {
    return (
      <CoverageCardFrame title="Approve the mirror">
        <Molecules.SignInApproval
          authorizationUrl={mirror.authorization_url}
          instructions={mirrorCoverage.description}
          action={
            <ActionButton
              label="Cancel"
              isPrimary={false}
              onClick={() =>
                void attempt("cancel the mirror", () =>
                  mirroring.cancelMirror(pubky),
                )
              }
              disabled={isPending}
            />
          }
        />
      </CoverageCardFrame>
    );
  }
  if (mirrorForm !== null) {
    return (
      <CoverageCardFrame title="Mirror to another homeserver">
        <MirrorForm
          initialHomeserver={mirrorForm.homeserver}
          isPending={isPending}
          onSubmit={(homeserver) => void handleMirrorSubmit(homeserver)}
          onCancel={() => setMirrorForm(null)}
        />
      </CoverageCardFrame>
    );
  }

  return (
    <CoverageCardFrame>
      <CoverageRow
        title="Private data"
        description={privateCoverage.description}
        tone={privateCoverage.tone}
        isCovered={auth.type === "SignedIn"}
        action={
          <ActionButton
            label={privateCoverage.actionLabel}
            isPrimary={privateCoverage.action === "sign-in"}
            onClick={handlePrivateAction}
            disabled={isPending}
          />
        }
      />
      <CoverageRow
        title="Mirror"
        description={mirrorCoverage.description}
        tone={mirrorCoverage.tone}
        isCovered={mirror.type === "Mirroring"}
        action={
          <ActionButton
            label={mirrorCoverage.actionLabel}
            isPrimary={mirrorCoverage.action !== "stop"}
            onClick={handleMirrorAction}
            disabled={isPending}
          />
        }
      />
    </CoverageCardFrame>
  );
}
