import type { ReactNode } from "react";
import { cva, type VariantProps } from "class-variance-authority";
import * as Atoms from "@/components/atoms";
import { cn } from "@/lib/utils";

const syncMessageVariants = cva(
  "flex flex-row justify-between items-center self-stretch py-4 px-6 rounded-lg gap-2",
  {
    variants: {
      status: {
        synced: "bg-status-synced-bg",
        syncing: "bg-status-syncing-bg",
        error: "bg-status-error-bg",
        "snapshot-success": "bg-status-snapshot-bg",
      },
    },
    defaultVariants: {
      status: "synced",
    },
  },
);

const textVariants = cva("text-sm font-bold", {
  variants: {
    status: {
      synced: "text-pubky-green",
      syncing: "text-pubky-purple",
      error: "text-pubky-red",
      "snapshot-success": "text-pubky-blue",
    },
  },
  defaultVariants: {
    status: "synced",
  },
});

const iconVariants = cva("w-6 h-6 shrink-0", {
  variants: {
    status: {
      synced: "text-pubky-green",
      syncing: "text-pubky-purple",
      error: "text-pubky-red",
      "snapshot-success": "text-pubky-blue",
    },
  },
  defaultVariants: {
    status: "synced",
  },
});

export interface SyncMessageProps
  extends VariantProps<typeof syncMessageVariants> {
  message: string;
  /** Shown in place of the status icon, such as a button to act on the message */
  action?: ReactNode;
  className?: string;
}

export function SyncMessage({
  status,
  message,
  action,
  className,
}: SyncMessageProps) {
  return (
    <div className={cn(syncMessageVariants({ status }), className)}>
      <div className="flex flex-col items-start gap-0.5 flex-1 min-w-0">
        {/* A long message, such as a raw backend error, is cut to three
            lines so it can't push the rest of the screen out of the window */}
        <span
          title={message}
          className={cn(
            textVariants({ status }),
            "break-words max-w-full text-left line-clamp-3",
          )}
        >
          {message}
        </span>
      </div>
      <div className="flex items-center gap-2">
        {action ??
          (status === "syncing" ? (
            <Atoms.Spinner className={iconVariants({ status })} size={24} />
          ) : status === "error" ? (
            <Atoms.AlertIcon className={iconVariants({ status })} size={24} />
          ) : (
            <Atoms.CheckIcon className={iconVariants({ status })} size={24} />
          ))}
      </div>
    </div>
  );
}
