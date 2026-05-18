import { useEffect } from "react";
import * as Stores from "@/stores";
import * as Utils from "@/utils";
import { cn } from "@/lib/utils";

const TOAST_DISPLAY_DURATION_MS = 2000;
const ERROR_TOAST_DISPLAY_DURATION_MS = 4000;

export function Toast() {
  const { toast, hideToast } = Stores.useUIStore();

  const isError = toast.type === "error";
  const duration = isError
    ? ERROR_TOAST_DISPLAY_DURATION_MS
    : TOAST_DISPLAY_DURATION_MS;

  useEffect(() => {
    if (toast.visible) {
      const timer = setTimeout(() => {
        hideToast();
      }, duration);
      return () => clearTimeout(timer);
    }
    return;
  }, [toast.visible, hideToast, duration]);

  return (
    <div
      role="status"
      aria-live="polite"
      className={cn(
        "fixed bottom-8 left-1/2 -translate-x-1/2 w-[360px] max-w-[calc(100vw-64px)]",
        "flex flex-row items-center gap-2 p-6",
        "shadow-[0_4px_6px_rgba(5,5,10,0.25),0_10px_15px_rgba(5,5,10,0.5)]",
        "backdrop-blur-[20px]",
        "transition-all duration-300 ease-out",
        "z-[1000]",
        "rounded-lg border",
        isError
          ? "bg-gradient-to-r from-red-500/10 to-surface-dark border-red-500"
          : "bg-gradient-to-r from-pubky-purple/10 to-surface-dark border-pubky-purple",
        toast.visible
          ? "opacity-100 translate-y-0"
          : "opacity-0 translate-y-5 pointer-events-none",
      )}
    >
      <div className="flex flex-col gap-1 flex-1">
        <div
          className={cn(
            "text-base font-bold text-left break-words",
            isError ? "text-red-400" : "text-text-muted",
          )}
        >
          {toast.message}
        </div>
        {toast.pubkyText && (
          <div className="text-sm font-normal text-text-muted/90 text-left overflow-hidden text-ellipsis whitespace-nowrap">
            {Utils.displayPubky(toast.pubkyText)}
          </div>
        )}
      </div>
    </div>
  );
}
