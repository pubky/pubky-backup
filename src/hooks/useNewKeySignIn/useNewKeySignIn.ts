import { useCallback, useEffect, useRef, useState } from "react";
import { cancelSignInNewKey, signInNewKey } from "@/services";
import { useUIStore } from "@/stores/uiStore";
import { isBackendError } from "@/types/backend-errors";
import { stripPubkyPrefix } from "@/utils/pubky";
import { Logger } from "@/lib";

/** Where a sign-in for a new key is at. */
export type NewKeySignInState =
  | { type: "Idle" }
  | { type: "Starting" }
  | { type: "AwaitingApproval"; authorization_url: string }
  | { type: "Failed"; message: string };

const IDLE: NewKeySignInState = { type: "Idle" };

/**
 * useNewKeySignIn
 *
 * Hook for adding a key by signing in with Pubky Ring, so that its public and
 * private data are backed up without the user entering their pubky. Whichever
 * key approves the sign-in is backed up and becomes the viewed key.
 *
 * A sign-in still awaiting approval is cancelled when the component unmounts.
 *
 * @param onSignedIn - Called with the pubky once it is signed in and backed up
 * @returns The sign-in state, and functions to start and cancel it
 *
 * @example
 * ```tsx
 * const { state, start, cancel } = useNewKeySignIn((pubky) => goToDashboard());
 *
 * if (state.type === "AwaitingApproval") {
 *   return <QrCode value={state.authorization_url} />;
 * }
 * return <button onClick={start}>Sign in with Pubky Ring</button>;
 * ```
 */
export function useNewKeySignIn(onSignedIn: (pubky: string) => void) {
  const [state, setState] = useState<NewKeySignInState>(IDLE);
  const setLastPubky = useUIStore((s) => s.setLastPubky);
  // Each attempt gets a number, so outcomes of cancelled or replaced attempts
  // are ignored. Zero means none is in progress.
  const nextAttemptRef = useRef(1);
  const currentAttemptRef = useRef(0);

  const start = useCallback(async () => {
    const attempt = nextAttemptRef.current++;
    currentAttemptRef.current = attempt;
    const isCurrent = () => currentAttemptRef.current === attempt;
    setState({ type: "Starting" });

    try {
      const pubky = stripPubkyPrefix(
        await signInNewKey((url) => {
          if (isCurrent()) {
            setState({ type: "AwaitingApproval", authorization_url: url });
          }
        }),
      );
      if (!isCurrent()) return;
      currentAttemptRef.current = 0;
      setState(IDLE);
      setLastPubky(pubky);
      onSignedIn(pubky);
    } catch (error: unknown) {
      if (!isCurrent()) return;
      currentAttemptRef.current = 0;
      Logger.error("useNewKeySignIn", "Sign-in for a new key failed", {
        error,
      });
      setState({
        type: "Failed",
        message: isBackendError(error) ? error.message : String(error),
      });
    }
  }, [onSignedIn, setLastPubky]);

  const cancel = useCallback(() => {
    const wasInProgress = currentAttemptRef.current !== 0;
    currentAttemptRef.current = 0;
    setState(IDLE);
    if (!wasInProgress) return;
    cancelSignInNewKey().catch((error: unknown) => {
      Logger.error("useNewKeySignIn", "Failed to cancel sign-in", { error });
    });
  }, []);

  // Don't leave a sign-in pending once nobody can see its QR code
  useEffect(() => cancel, [cancel]);

  return { state, start, cancel };
}
