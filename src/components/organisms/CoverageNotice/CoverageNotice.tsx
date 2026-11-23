import * as Atoms from "@/components/atoms";
import * as Stores from "@/stores";
import * as Utils from "@/utils";
import type { AuthStatus, MirrorStatus } from "@/stores/uiStore";

/**
 * What stopped being backed up, if anything. A pubky that was never signed in
 * needs no notice: its coverage badge already says "Public only".
 */
function describeProblem(
  auth: AuthStatus,
  mirror: MirrorStatus,
): string | null {
  if (auth.type === "SessionExpired") {
    return "Private data is no longer backed up: your session expired.";
  }
  if (auth.type === "SignInFailed") {
    return "Sign-in failed, so private data isn't backed up.";
  }
  if (mirror.type === "SessionExpired") {
    return "Mirroring is paused until you approve it again.";
  }
  if (mirror.type === "Failing" || mirror.type === "SetupFailed") {
    return "Mirroring isn't working.";
  }
  return null;
}

export interface CoverageNoticeProps {
  pubky: string;
  /** Opens the controls for signing in and mirroring */
  onOpen: () => void;
}

/**
 * A one-line warning when a pubky's private data or mirror stopped being
 * backed up, with a button to the controls that fix it.
 */
export function CoverageNotice({ pubky, onOpen }: CoverageNoticeProps) {
  const auth =
    Stores.useUIStore((s) => s.keyStates[pubky]?.auth) ?? Utils.SIGNED_OUT;
  const mirror =
    Stores.useUIStore((s) => s.keyStates[pubky]?.mirror) ?? Utils.NOT_MIRRORED;
  const problem = describeProblem(auth, mirror);
  if (problem === null) return null;

  return (
    <div className="flex items-center self-stretch gap-3 py-3 px-4 rounded-lg text-left bg-status-error-bg">
      <Atoms.AlertIcon size={16} className="text-pubky-red shrink-0" />
      <span className="flex-1 min-w-0 text-sm font-bold text-pubky-red">
        {problem}
      </span>
      <Atoms.Button
        variant="secondary"
        onClick={onOpen}
        className="py-2 px-4 shrink-0"
      >
        <span className="text-sm font-medium text-white">Fix</span>
      </Atoms.Button>
    </div>
  );
}
