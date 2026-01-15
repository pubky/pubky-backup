const PUBKEY_DISPLAY_PREFIX_LENGTH = 5;
const PUBKEY_DISPLAY_SUFFIX_LENGTH = 5;

/**
 * Truncate a pubky string for display with ellipsis
 * @param str - The pubky string to truncate, or null
 * @param prefixLength - Number of characters to show at the start (default: 5)
 * @param suffixLength - Number of characters to show at the end (default: 5)
 */
export function displayPubky(
  str: string | null,
  prefixLength: number = PUBKEY_DISPLAY_PREFIX_LENGTH,
  suffixLength: number = PUBKEY_DISPLAY_SUFFIX_LENGTH,
): string {
  if (str === null) return "...";
  if (str.length <= prefixLength + suffixLength + 3) return str;
  return (
    str.substring(0, prefixLength) +
    "..." +
    str.substring(str.length - suffixLength)
  );
}
