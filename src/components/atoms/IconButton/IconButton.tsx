import type { ButtonHTMLAttributes, ReactNode } from "react";
import { cn } from "@/lib/utils";

export interface IconButtonProps
  extends ButtonHTMLAttributes<HTMLButtonElement> {
  children: ReactNode;
  variant?: "default" | "inline";
}

export function IconButton({
  children,
  variant = "default",
  className,
  disabled,
  ...props
}: IconButtonProps) {
  const baseStyles =
    variant === "inline"
      ? "inline-flex items-center justify-center p-0 bg-transparent border-none text-white cursor-pointer transition-opacity duration-200 hover:opacity-70 disabled:opacity-50 disabled:cursor-not-allowed"
      : "flex items-center justify-center p-1.5 w-7 h-7 bg-transparent border-none rounded cursor-pointer text-white transition-colors duration-200 hover:bg-button-hover-bg disabled:opacity-50 disabled:cursor-not-allowed";

  return (
    <button
      type="button"
      className={cn(baseStyles, className)}
      disabled={disabled}
      {...props}
    >
      {children}
    </button>
  );
}
