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
        "w-full flex flex-col justify-center items-stretch",
        "pt-6 px-8 pb-8 gap-6",
        "bg-surface-dark",
        className,
      )}
    >
      {children}
    </div>
  );
}
