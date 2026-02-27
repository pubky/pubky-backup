import * as Atoms from "@/components/atoms";

export interface DashboardHeaderProps {
  pubkyDisplay: string;
  status: Atoms.StatusBadgeProps["status"];
  onCopy: () => void;
}

export function DashboardHeader({
  pubkyDisplay,
  status,
  onCopy,
}: DashboardHeaderProps) {
  return (
    <div className="flex items-center self-stretch gap-1.5">
      <div className="flex items-center gap-1.5 flex-1">
        <h4 className="text-base font-bold text-white m-0">{pubkyDisplay}</h4>
        <Atoms.IconButton
          variant="inline"
          onClick={onCopy}
          title="Copy full pubky"
        >
          <Atoms.CopyIcon size={16} />
        </Atoms.IconButton>
      </div>
      <Atoms.StatusBadge status={status} />
    </div>
  );
}
