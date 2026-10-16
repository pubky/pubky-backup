import type { AuthStatus, MirrorStatus } from "@/stores/uiStore";

/** The sign-in status of a pubky whose state is not known yet. */
export const SIGNED_OUT: AuthStatus = { type: "SignedOut" };

/** The mirror status of a pubky whose state is not known yet. */
export const NOT_MIRRORED: MirrorStatus = { type: "NotMirrored" };

/** Which of a pubky's data is backed up, as far as the UI tells it apart. */
export type BackupCoverage =
  | "public-and-private"
  | "public-only"
  /** Private data was backed up, but the session expired */
  | "private-paused";

/** Which of a pubky's data is backed up, given its sign-in status. */
export function backupCoverage(auth: AuthStatus): BackupCoverage {
  switch (auth.type) {
    case "SignedIn":
      return "public-and-private";
    case "SessionExpired":
      return "private-paused";
    case "SignedOut":
    case "AwaitingApproval":
    case "SignInFailed":
      return "public-only";
  }
}
