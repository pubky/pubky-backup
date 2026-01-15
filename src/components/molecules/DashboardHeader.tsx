import {
  IconButton,
  BackIcon,
  CopyIcon,
  StatusBadge,
} from "@/components/atoms";
import type { StatusBadgeProps } from "@/components/atoms";

export interface DashboardHeaderProps {
  pubkyDisplay: string;
  status: StatusBadgeProps["status"];
  onBack: () => void;
  onCopy: () => void;
}

export function DashboardHeader({
  pubkyDisplay,
  status,
  onBack,
  onCopy,
}: DashboardHeaderProps) {
  return (
    <div className="flex items-center self-stretch gap-1.5">
      <div className="flex items-center gap-1.5 flex-1">
        <IconButton variant="inline" onClick={onBack} title="Back">
          <BackIcon size={16} />
        </IconButton>
        <h4 className="text-base font-bold text-white m-0">{pubkyDisplay}</h4>
        <IconButton variant="inline" onClick={onCopy} title="Copy full pubky">
          <CopyIcon size={16} />
        </IconButton>
      </div>
      <StatusBadge status={status} />
    </div>
  );
}
