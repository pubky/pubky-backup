import { cva } from "class-variance-authority";
import { cn } from "@/lib/utils";
import type { ButtonProps } from "./Button.types";

export const buttonVariants = cva(
  [
    "flex justify-center items-center gap-2",
    "py-5 px-8 rounded-full",
    "shadow-[0_1px_2px_rgba(5,5,10,0.2)]",
    "cursor-pointer transition-all duration-200",
    "disabled:cursor-not-allowed",
  ],
  {
    variants: {
      variant: {
        primary: [
          "bg-pubky-purple/15 border border-pubky-purple",
          "hover:opacity-80",
          "disabled:opacity-30",
        ],
        secondary: [
          "bg-button-hover-bg border border-border",
          "hover:bg-button-active-bg",
          "disabled:opacity-50",
        ],
      },
    },
    defaultVariants: {
      variant: "primary",
    },
  },
);

export function Button({
  children,
  onClick,
  disabled,
  className,
  variant,
  type = "button",
}: ButtonProps) {
  return (
    <button
      type={type}
      onClick={onClick}
      disabled={disabled}
      className={cn(buttonVariants({ variant }), className)}
    >
      {children}
    </button>
  );
}
