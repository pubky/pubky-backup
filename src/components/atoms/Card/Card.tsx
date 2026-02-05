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
        "w-[360px] min-h-[380px] flex flex-col justify-center items-stretch",
        "p-4 pb-5 px-5 gap-3",
        "bg-surface-dark border border-border rounded-lg",
        "shadow-[0_8px_10px_rgba(5,5,10,0.25),0_20px_25px_rgba(5,5,10,0.5)]",
        className,
      )}
    >
      {children}
    </div>
  );
}
