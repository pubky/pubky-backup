import { useUIStore } from "@/stores/uiStore";

/**
 * useKeys
 *
 * Hook for getting the list of pubky keys that are being backed up.
 * Returns the keys from the Zustand store's keyStates.
 *
 * @returns Array of pubky strings
 *
 * @example
 * ```tsx
 * const keys = useKeys();
 *
 * return (
 *   <PubkyInput
 *     value={pubkyValue}
 *     onChange={setPubkyValue}
 *     suggestions={keys}
 *     placeholder="Enter pubky to add..."
 *   />
 * );
 * ```
 */
export function useKeys(): string[] {
  const keyStates = useUIStore((s) => s.keyStates);
  return Object.keys(keyStates);
}
