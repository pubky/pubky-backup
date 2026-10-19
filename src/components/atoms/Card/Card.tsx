import { type ReactNode } from "react";
import { cn } from "@/lib/utils";

export interface CardProps {
  children: ReactNode;
  className?: string;
}

export function Card({ children, className }: CardProps) {
  return (
    <div
      className={cn(
        "w-full flex-1 min-h-0 flex flex-col justify-start items-stretch",
        "pt-6 px-8 pb-8 gap-6 overflow-y-auto",
        "bg-surface-dark",
        className,
      )}
    >
      {children}
    </div>
  );
}
