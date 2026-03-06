import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export interface InfoCardProps {
  icon: ReactNode;
  label: string;
  value: string;
  action?: ReactNode;
  fullWidth?: boolean;
  className?: string;
}

export function InfoCard({
  icon,
  label,
  value,
  action,
  fullWidth = false,
  className,
}: InfoCardProps) {
  return (
    <div
      className={cn(
        "grid grid-cols-[18px_1fr] gap-x-2.5 gap-y-2 p-3 bg-surface-light rounded-lg text-left",
        fullWidth ? "self-stretch w-full" : "flex-1",
        className,
      )}
    >
      <div className="col-span-1 row-span-1 text-text-secondary">{icon}</div>
      <span className="col-span-1 row-span-1 text-xs font-medium uppercase tracking-widest text-text-secondary pt-0.5">
        {label}
      </span>
      <div className="col-span-2 row-span-1 flex items-center gap-2">
        <strong
          className={cn(
            "text-base font-bold text-white m-0",
            fullWidth &&
              "overflow-hidden text-ellipsis whitespace-nowrap max-w-fit",
          )}
        >
          {value}
        </strong>
        {action}
      </div>
    </div>
  );
}
