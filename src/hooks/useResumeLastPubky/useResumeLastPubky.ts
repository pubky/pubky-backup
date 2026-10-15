import { useEffect } from "react";
import { useUIStore } from "@/stores/uiStore";
import { handleBackendError } from "@/utils/error-handler";
import { Logger } from "@/lib";
import { useAddKey } from "../useAddKey";
import { useLastPubky } from "../useLastPubky";

/**
 * useResumeLastPubky
 *
 * Resumes the pubky that was viewed when the app last ran: backs it up again
 * and opens the dashboard. Runs once per app session, so returning to the
 * startup screen later doesn't jump straight back to the dashboard.
 *
 * @returns Whether the last pubky is still being looked up or resumed
 *
 * @example
 * ```tsx
 * const { isResuming } = useResumeLastPubky();
 *
 * return isResuming ? <Spinner /> : <AddKeyForm onAdded={openDashboard} />;
 * ```
 */
export function useResumeLastPubky() {
  const hasAutoLoaded = useUIStore((s) => s.hasAutoLoaded);
  const { setHasAutoLoaded, setScreen } = useUIStore.getState();
  const { lastPubky, isLoading } = useLastPubky();
  const { addKey, isPending } = useAddKey();

  useEffect(() => {
    if (!lastPubky || hasAutoLoaded) return;
    setHasAutoLoaded(true);
    addKey({ pubkyValue: lastPubky })
      .then(() => setScreen("dashboard"))
      .catch((error: unknown) => {
        Logger.error("useResumeLastPubky", "Failed to resume last pubky", {
          error,
        });
        handleBackendError(error);
      });
  }, [lastPubky, hasAutoLoaded, addKey, setHasAutoLoaded, setScreen]);

  // Until the last pubky is known and resumed, the startup form would only flash
  const isAboutToResume = !hasAutoLoaded && (isLoading || lastPubky !== null);
  return { isResuming: isAboutToResume || isPending };
}
