/**
 * Format a byte count into a human-readable string
 */
export function formatFileSize(bytes: number): string {
  if (bytes === 0) return "0 B";

  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"] as const;
  const maxIndex = sizes.length - 1;
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(k)), maxIndex);

  const size = bytes / Math.pow(k, i);
  const decimals = i === 0 ? 0 : size < 10 ? 2 : 1;

  return `${size.toFixed(decimals)} ${sizes[i]}`;
}

/**
 * Format a Unix timestamp into a human-readable time string (12-hour format)
 */
export function formatTimestamp(unixTimestamp: number): string {
  const date = new Date(unixTimestamp * 1000);
  const hours = date.getHours();
  const minutes = date.getMinutes().toString().padStart(2, "0");
  const seconds = date.getSeconds().toString().padStart(2, "0");
  const ampm = hours >= 12 ? "PM" : "AM";
  const displayHours = hours % 12 || 12;
  return `${displayHours}:${minutes}:${seconds} ${ampm}`;
}

/**
 * Format the countdown to next sync in a human-readable string
 */
export function formatCountdown(nextSyncTime: number): string {
  const now = Math.floor(Date.now() / 1000);

  if (nextSyncTime <= now) {
    return "Syncing soon...";
  }

  const remaining = nextSyncTime - now;
  const minutes = Math.floor(remaining / 60);
  const seconds = remaining % 60;

  if (minutes > 0) {
    return `Next backup in ${minutes} minute${minutes !== 1 ? "s" : ""}...`;
  } else if (seconds > 0) {
    return `Next backup in ${seconds} second${seconds !== 1 ? "s" : ""}...`;
  } else {
    return "Syncing soon...";
  }
}
