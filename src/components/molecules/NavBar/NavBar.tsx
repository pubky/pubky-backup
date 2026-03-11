import * as Atoms from "@/components/atoms";
import * as Stores from "@/stores";
import { cn } from "@/lib/utils";
import type { Page } from "@/stores/uiStore";

interface NavButtonProps {
  page: Page;
  currentPage: Page;
  disabled?: boolean;
  onClick: () => void;
  children: React.ReactNode;
  title: string;
}

function NavButton({
  page,
  currentPage,
  disabled,
  onClick,
  children,
  title,
}: NavButtonProps) {
  const isActive = page === currentPage;

  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={title}
      className={cn(
        "flex items-center justify-center w-7 h-7 rounded-md border-none transition-colors duration-200",
        disabled ? "cursor-not-allowed opacity-30" : "cursor-pointer",
        isActive
          ? "bg-surface-light text-white"
          : "bg-transparent text-text-secondary hover:text-white hover:bg-surface-light/50",
      )}
    >
      {children}
    </button>
  );
}

export function NavBar() {
  const currentPage = Stores.useUIStore((s) => s.currentPage);
  const setPage = Stores.useUIStore((s) => s.setPage);
  const navDisabled = Stores.useUIStore((s) => s.navDisabled);

  return (
    <nav className="flex items-center gap-0.5">
      <NavButton
        page="sync"
        currentPage={currentPage}
        disabled={navDisabled}
        onClick={() => setPage("sync")}
        title="Sync"
      >
        <Atoms.NavSyncIcon size={16} />
      </NavButton>
      <NavButton
        page="activity"
        currentPage={currentPage}
        disabled={navDisabled}
        onClick={() => setPage("activity")}
        title="Activity"
      >
        <Atoms.NavActivityIcon size={16} />
      </NavButton>
      <NavButton
        page="keys"
        currentPage={currentPage}
        disabled={navDisabled}
        onClick={() => setPage("keys")}
        title="Keys"
      >
        <Atoms.NavKeysIcon size={16} />
      </NavButton>
      <NavButton
        page="settings"
        currentPage={currentPage}
        disabled={navDisabled}
        onClick={() => setPage("settings")}
        title="Settings"
      >
        <Atoms.NavSettingsIcon size={16} />
      </NavButton>
    </nav>
  );
}
