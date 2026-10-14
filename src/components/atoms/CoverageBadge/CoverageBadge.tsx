import { cva } from "class-variance-authority";
import { cn } from "@/lib/utils";
import type { BackupCoverage } from "@/utils/coverage";
import { GlobeIcon, LockIcon } from "../Icons";

const coverageBadgeVariants = cva(
  "inline-flex items-center gap-1.5 px-2 py-1 rounded text-xs font-medium whitespace-nowrap",
  {
    variants: {
      coverage: {
        "public-and-private": "bg-status-synced-bg text-pubky-green",
        "public-only": "bg-button-active-bg text-text-light",
        "private-paused": "bg-status-error-bg text-pubky-red",
      },
    },
  },
);

const coverageText: Record<BackupCoverage, { label: string; title: string }> = {
  "public-and-private": {
    label: "Public + private",
    title: "Signed in: public and private data are backed up",
  },
  "public-only": {
    label: "Public only",
    title: "Not signed in: only public data is backed up",
  },
  "private-paused": {
    label: "Public only · sign-in expired",
    title: "The session expired: sign in again to back up private data",
  },
};

export interface CoverageBadgeProps {
  coverage: BackupCoverage;
  className?: string;
}

/** Which of a pubky's data is backed up: public only, or private data too. */
export function CoverageBadge({ coverage, className }: CoverageBadgeProps) {
  const { label, title } = coverageText[coverage];
  const Icon = coverage === "public-and-private" ? LockIcon : GlobeIcon;

  return (
    <span
      title={title}
      className={cn(coverageBadgeVariants({ coverage }), className)}
    >
      <Icon size={12} />
      {label}
    </span>
  );
}
