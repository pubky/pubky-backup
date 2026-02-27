import { useMutation } from "@tanstack/react-query";
import { setViewedPubky } from "@/services";

/**
 * useSetViewedPubky
 *
 * Mutation hook for switching which pubky is currently being viewed in the UI.
 * The backend syncs all keys concurrently - this only controls which key's
 * data is displayed.
 *
 * @returns TanStack Mutation result with mutate/mutateAsync functions
 *
 * @example
 * ```tsx
 * const { mutateAsync: switchKey, isPending } = useSetViewedPubky();
 *
 * const handleSelect = async (pubky: string) => {
 *   await switchKey(pubky);
 *   // Navigate to sync page
 * };
 * ```
 */
export function useSetViewedPubky() {
  return useMutation({
    mutationFn: setViewedPubky,
  });
}
