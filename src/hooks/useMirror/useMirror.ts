import { useCallback, useEffect, useState } from "react";
import { cancelMirror, getConfig, startMirror, stopMirror } from "@/services";
import { Logger } from "@/lib";

/**
 * useMirror
 *
 * Hook for mirroring a pubky's backup to a second homeserver, and stopping it.
 *
 * Starting a mirror only asks the backend for an approval link. The link and
 * the outcome arrive through the pubky's `mirror` state, so render that rather
 * than waiting on these functions.
 *
 * `isEnabled` says whether this build offers mirroring at all. It is false
 * until the backend has answered, so mirroring is never offered by mistake.
 *
 * @returns Object with the mirror actions, a shared isPending state and isEnabled
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
  const [isEnabled, setIsEnabled] = useState(false);

  useEffect(() => {
    let isMounted = true;
    getConfig()
      .then((config) => {
        if (isMounted) setIsEnabled(config.mirroring);
      })
      .catch((error: unknown) => {
        Logger.error("useMirror", "Failed to read whether mirroring is on", {
          error,
        });
      });
    return () => {
      isMounted = false;
    };
  }, []);

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
    isEnabled,
  };
}
