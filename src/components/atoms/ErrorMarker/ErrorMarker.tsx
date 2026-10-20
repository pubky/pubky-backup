import { AlertIcon } from "../Icons";

export interface ErrorMarkerProps {
  size?: number;
}

/** Marks a pubky whose backup is in error */
export function ErrorMarker({ size = 16 }: ErrorMarkerProps) {
  return (
    <span title="Backup failed">
      <AlertIcon size={size} className="text-pubky-red" />
    </span>
  );
}
