import * as Atoms from "@/components/atoms";
import * as Stores from "@/stores";
import * as Utils from "@/utils";
import type { AuthStatus, MirrorStatus } from "@/stores/uiStore";
import { cn } from "@/lib";

interface Notice {
  text: string;
  tone: "neutral" | "warning";
  /** Names where the button leads, not the action taken there */
  actionLabel: string;
}

/**
 * What to tell the owner about their private data and mirror, if anything.
 * An approval waiting in Pubky Ring comes first, as the controls show its QR
 * code before anything else, then something that stopped being backed up,
 * then private data that was never backed up.
 */
function describeNotice(auth: AuthStatus, mirror: MirrorStatus): Notice | null {
  if (auth.type === "AwaitingApproval") {
    return {
      text: "A sign-in is waiting for approval in Pubky Ring.",
      tone: "neutral",
      actionLabel: "Show",
    };
  }
  if (mirror.type === "AwaitingApproval") {
    return {
      text: "A mirror is waiting for approval in Pubky Ring.",
      tone: "neutral",
      actionLabel: "Show",
    };
  }
  if (auth.type === "SessionExpired") {
    return {
      text: "Private data is no longer backed up: your session expired.",
      tone: "warning",
      actionLabel: "Fix",
    };
  }
  if (auth.type === "SignInFailed") {
    return {
      text: "Sign-in failed, so private data isn't backed up.",
      tone: "warning",
      actionLabel: "Fix",
    };
  }
  if (mirror.type === "SessionExpired") {
    return {
      text: "Mirroring is paused until you approve it again.",
      tone: "warning",
      actionLabel: "Fix",
    };
  }
  if (mirror.type === "Failing" || mirror.type === "SetupFailed") {
    return {
      text: "Mirroring isn't working.",
      tone: "warning",
      actionLabel: "Fix",
    };
  }
  if (auth.type === "SignedOut") {
    return {
      text: "Private data isn't backed up.",
      tone: "neutral",
      actionLabel: "Set up",
    };
  }
  return null;
}

export interface CoverageNoticeProps {
  pubky: string;
  /** Opens the controls for signing in and mirroring */
  onOpen: () => void;
}

/**
 * A one-line note on a pubky's private data and mirror, with a button to the
 * controls that act on it. Shows nothing while everything is backed up.
 */
export function CoverageNotice({ pubky, onOpen }: CoverageNoticeProps) {
  const auth =
    Stores.useUIStore((s) => s.keyStates[pubky]?.auth) ?? Utils.SIGNED_OUT;
  const mirror =
    Stores.useUIStore((s) => s.keyStates[pubky]?.mirror) ?? Utils.NOT_MIRRORED;
  const notice = describeNotice(auth, mirror);
  if (notice === null) return null;

  const isWarning = notice.tone === "warning";

  return (
    <div
      className={cn(
        "flex items-center self-stretch gap-3 py-3 px-4 rounded-lg text-left",
        isWarning ? "bg-status-error-bg" : "bg-surface-light",
      )}
    >
      {isWarning && (
        <Atoms.AlertIcon size={16} className="text-pubky-red shrink-0" />
      )}
      <span
        className={cn(
          "flex-1 min-w-0 text-sm",
          isWarning ? "font-bold text-pubky-red" : "text-text-light",
        )}
      >
        {notice.text}
      </span>
      <Atoms.Button
        variant="secondary"
        onClick={onOpen}
        className="py-2 px-4 shrink-0"
      >
        <span className="text-sm font-medium text-white">
          {notice.actionLabel}
        </span>
      </Atoms.Button>
    </div>
  );
}
