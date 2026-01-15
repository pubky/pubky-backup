import { cva, type VariantProps } from "class-variance-authority";
import * as Atoms from "@/components/atoms";
import { cn } from "@/lib/utils";

const syncMessageVariants = cva(
  "flex flex-row justify-between items-center self-stretch py-2.5 px-3.5 rounded-md gap-2",
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

const textVariants = cva("text-xs font-bold", {
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

const iconVariants = cva("w-[18px] h-[18px] shrink-0", {
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
  className?: string;
}

export function SyncMessage({ status, message, className }: SyncMessageProps) {
  return (
    <div className={cn(syncMessageVariants({ status }), className)}>
      <div className="flex flex-col items-start gap-0.5 flex-1">
        <span className={textVariants({ status })}>{message}</span>
      </div>
      <div className="flex items-center gap-2">
        {status === "syncing" ? (
          <Atoms.Spinner className={iconVariants({ status })} size={16} />
        ) : (
          <Atoms.CheckIcon className={iconVariants({ status })} size={18} />
        )}
      </div>
    </div>
  );
}
