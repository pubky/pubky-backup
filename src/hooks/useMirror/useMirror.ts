import { useCallback, useState } from "react";
import { cancelMirror, startMirror, stopMirror } from "@/services";

/**
 * useMirror
 *
 * Hook for mirroring a pubky's backup to a second homeserver, and stopping it.
 *
 * Starting a mirror only asks the backend for an approval link. The link and
 * the outcome arrive through the pubky's `mirror` state, so render that rather
 * than waiting on these functions.
 *
 * @returns Object with the mirror actions and a shared isPending state
 *
 * @example
 * ```tsx
 * const { startMirror, isPending } = useMirror();
 *
 * return (
 *   <button onClick={() => void startMirror(pubky, homeserver)} disabled={isPending}>
 *     Mirror
 *   </button>
 * );
 * ```
 */
export function useMirror() {
  const [isPending, setIsPending] = useState(false);

  const run = useCallback(async (command: () => Promise<unknown>) => {
    setIsPending(true);
    try {
      await command();
    } finally {
      setIsPending(false);
    }
  }, []);

  const start = useCallback(
    (pubky: string, homeserver: string) =>
      run(() => startMirror(pubky, homeserver)),
    [run],
  );
  const cancel = useCallback(
    (pubky: string) => run(() => cancelMirror(pubky)),
    [run],
  );
  const stop = useCallback(
    (pubky: string) => run(() => stopMirror(pubky)),
    [run],
  );

  return {
    startMirror: start,
    cancelMirror: cancel,
    stopMirror: stop,
    isPending,
  };
}
