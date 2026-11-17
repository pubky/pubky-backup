const PUBKEY_DISPLAY_PREFIX_LENGTH = 5;
const PUBKEY_DISPLAY_SUFFIX_LENGTH = 5;
const PUBKY_PREFIX = "pubky";
/** Length of a pubky in bare z32 form */
const Z32_PUBKY_LENGTH = 52;

/**
 * Strip the "pubky" prefix from a pubky string if present.
 *
 * Only strips it when a full z32 key follows, as the backend does, since a
 * bare key can itself start with "pubky".
 * @param str - The pubky string that may have a "pubky" prefix
 */
export function stripPubkyPrefix(str: string): string {
  const isPrefixed =
    str.startsWith(PUBKY_PREFIX) &&
    str.length === PUBKY_PREFIX.length + Z32_PUBKY_LENGTH;
  return isPrefixed ? str.slice(PUBKY_PREFIX.length) : str;
}

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
  // Strip the "pubky" prefix if present (backend returns it, but we display without)
  const normalized = stripPubkyPrefix(str);
  if (normalized.length <= prefixLength + suffixLength + 3) return normalized;
  return (
    normalized.substring(0, prefixLength) +
    "..." +
    normalized.substring(normalized.length - suffixLength)
  );
}
