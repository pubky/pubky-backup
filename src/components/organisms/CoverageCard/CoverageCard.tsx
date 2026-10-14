import { type ReactNode } from "react";
import * as Atoms from "@/components/atoms";
import * as Molecules from "@/components/molecules";
import * as Hooks from "@/hooks";
import * as Stores from "@/stores";
import * as Utils from "@/utils";
import type { AuthStatus } from "@/stores/uiStore";
import { cn, Logger } from "@/lib";

const SIGNED_OUT: AuthStatus = { type: "SignedOut" };

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
 * Shows whether a pubky's private data is backed up, and lets its owner sign
 * in to add it or sign out. Public data is always backed up, which the
 * dashboard header shows.
 */
export function CoverageCard({ pubky }: CoverageCardProps) {
  const auth = Stores.useUIStore((s) => s.keyStates[pubky]?.auth) ?? SIGNED_OUT;
  const { startSignIn, cancelSignIn, signOut, isPending } = Hooks.useSignIn();

  const privateCoverage = describePrivateCoverage(auth);

  const handleAction = async () => {
    const actions: Record<PrivateAction, (pubky: string) => Promise<void>> = {
      "sign-in": startSignIn,
      cancel: cancelSignIn,
      "sign-out": signOut,
    };
    try {
      await actions[privateCoverage.action](pubky);
    } catch (error: unknown) {
      Logger.error("CoverageCard", `Failed to ${privateCoverage.action}`, {
        error,
      });
      Utils.handleBackendError(error);
    }
  };

  const actionButton = (
    <Atoms.Button
      variant={privateCoverage.action === "sign-in" ? "primary" : "secondary"}
      onClick={() => void handleAction()}
      disabled={isPending}
      className="py-2 px-4 shrink-0"
    >
      <span
        className={cn(
          "text-sm",
          privateCoverage.action === "sign-in"
            ? "font-bold text-pubky-purple"
            : "font-medium text-white",
        )}
      >
        {privateCoverage.actionLabel}
      </span>
    </Atoms.Button>
  );

  // While a sign-in awaits approval the card shows only that, which keeps the
  // dashboard within the window's maximum height
  if (auth.type === "AwaitingApproval") {
    return (
      <CoverageCardFrame title="Sign in to back up private data">
        <Molecules.SignInApproval
          authorizationUrl={auth.authorization_url}
          instructions={privateCoverage.description}
          action={actionButton}
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
        action={actionButton}
      />
    </CoverageCardFrame>
  );
}
