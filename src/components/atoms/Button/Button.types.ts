import { type MouseEvent, type ReactNode } from "react";

export type ButtonVariant = "primary" | "secondary";

export interface ButtonProps {
  children: ReactNode;
  onClick?: (event: MouseEvent<HTMLButtonElement>) => void;
  disabled?: boolean;
  className?: string;
  type?: "button" | "submit" | "reset";
  variant?: ButtonVariant | null;
}
