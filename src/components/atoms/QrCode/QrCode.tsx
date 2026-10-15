import { QRCodeSVG } from "qrcode.react";
import { cn } from "@/lib/utils";

export interface QrCodeProps {
  value: string;
  /** Accessible name, since the code itself can't be read */
  label: string;
  size?: number;
  className?: string;
}

export function QrCode({ value, label, size = 160, className }: QrCodeProps) {
  return (
    // QR scanners need a light background with a quiet margin around the code
    <div className={cn("inline-flex p-3 bg-white rounded-lg", className)}>
      <QRCodeSVG value={value} size={size} title={label} />
    </div>
  );
}
