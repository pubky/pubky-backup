import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const statusBadgeVariants = cva(
  "inline-flex items-center justify-end px-2 py-1 rounded text-xs font-medium tracking-widest uppercase",
  {
    variants: {
      status: {
        synced: "bg-pubky-green text-surface-dark",
        syncing: "bg-pubky-purple text-white",
        snapshot: "bg-pubky-blue text-surface-dark",
      },
    },
    defaultVariants: {
      status: "synced",
    },
  },
);

export interface StatusBadgeProps
  extends VariantProps<typeof statusBadgeVariants> {
  className?: string;
}

const statusTextMap = {
  syncing: "SYNCING",
  snapshot: "SNAPSHOT!",
  synced: "SYNCED",
} as const;

export function StatusBadge({ status, className }: StatusBadgeProps) {
  const text = statusTextMap[status ?? "synced"];

  return (
    <div className={cn(statusBadgeVariants({ status }), className)}>{text}</div>
  );
}
