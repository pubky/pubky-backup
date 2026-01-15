import { cn } from "@/lib/utils";

export interface SpinnerProps {
  className?: string;
  size?: number;
}

export function Spinner({ className, size = 16 }: SpinnerProps) {
  return (
    <svg
      className={cn("animate-spin", className)}
      viewBox="0 0 16 16"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      width={size}
      height={size}
    >
      <path
        d="M14 8C14 11.3137 11.3137 14 8 14C4.68629 14 2 11.3137 2 8C2 4.68629 4.68629 2 8 2"
        stroke="currentColor"
        strokeWidth="1.33"
        strokeLinecap="round"
      />
    </svg>
  );
}
