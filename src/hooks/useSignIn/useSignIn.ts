import { useCallback, useState } from "react";
import { cancelSignIn, signOut, startSignIn } from "@/services";

/**
 * useSignIn
 *
 * Hook for signing a pubky in and out. A signed-in pubky has its private data
 * backed up as well as its public data.
 *
 * Starting a sign-in only asks the backend for an approval link. The link and
 * the outcome arrive through the pubky's `auth` state, so render that rather
 * than waiting on these functions.
 *
 * @returns Object with the sign-in actions and a shared isPending state
 *
 * @example
 * ```tsx
 * const { startSignIn, isPending } = useSignIn();
 *
 * return (
 *   <button onClick={() => void startSignIn(pubky)} disabled={isPending}>
 *     Sign in
 *   </button>
 * );
 * ```
 */
export function useSignIn() {
  const [isPending, setIsPending] = useState(false);

  const run = useCallback(
    async (command: (pubky: string) => Promise<unknown>, pubky: string) => {
      setIsPending(true);
      try {
        await command(pubky);
      } finally {
        setIsPending(false);
      }
    },
    [],
  );

  const start = useCallback((pubky: string) => run(startSignIn, pubky), [run]);
  const cancel = useCallback(
    (pubky: string) => run(cancelSignIn, pubky),
    [run],
  );
  const out = useCallback((pubky: string) => run(signOut, pubky), [run]);

  return {
    startSignIn: start,
    cancelSignIn: cancel,
    signOut: out,
    isPending,
  };
}
