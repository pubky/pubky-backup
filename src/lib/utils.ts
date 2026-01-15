import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

/**
 * Merge class names with tailwind-merge for proper overrides
 */
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
