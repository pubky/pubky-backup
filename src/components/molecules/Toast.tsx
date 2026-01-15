import { useEffect } from "react";
import { useUIStore } from "@/stores/useUIStore";
import { displayPubky } from "@/utils/pubky";
import { cn } from "@/lib/utils";

const TOAST_DISPLAY_DURATION_MS = 2000;

export function Toast() {
  const { toast, hideToast } = useUIStore();

  useEffect(() => {
    if (toast.visible) {
      const timer = setTimeout(() => {
        hideToast();
      }, TOAST_DISPLAY_DURATION_MS);
      return () => clearTimeout(timer);
    }
    return;
  }, [toast.visible, hideToast]);

  return (
    <div
      role="status"
      aria-live="polite"
      className={cn(
        "fixed bottom-8 left-1/2 -translate-x-1/2 w-[360px] max-w-[calc(100vw-64px)]",
        "flex flex-row items-center gap-2 p-6",
        "bg-gradient-to-r from-pubky-purple/10 to-surface-dark",
        "border border-pubky-purple rounded-lg",
        "shadow-[0_4px_6px_rgba(5,5,10,0.25),0_10px_15px_rgba(5,5,10,0.5)]",
        "backdrop-blur-[20px]",
        "transition-all duration-300 ease-out",
        "z-[1000]",
        toast.visible
          ? "opacity-100 translate-y-0"
          : "opacity-0 translate-y-5 pointer-events-none",
      )}
    >
      <div className="flex flex-col gap-1 flex-1">
        <div className="text-base font-bold text-text-muted text-left">
          Pubky copied to clipboard
        </div>
        <div className="text-sm font-normal text-text-muted/90 text-left overflow-hidden text-ellipsis whitespace-nowrap">
          {displayPubky(toast.pubkyText)}
        </div>
      </div>
    </div>
  );
}
