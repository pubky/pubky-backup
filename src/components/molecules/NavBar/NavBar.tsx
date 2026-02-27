import * as Atoms from "@/components/atoms";
import * as Stores from "@/stores";
import { cn } from "@/lib/utils";
import type { Page } from "@/stores/uiStore";

interface NavButtonProps {
  page: Page;
  currentPage: Page;
  onClick: () => void;
  children: React.ReactNode;
  title: string;
}

function NavButton({
  page,
  currentPage,
  onClick,
  children,
  title,
}: NavButtonProps) {
  const isActive = page === currentPage;

  return (
    <button
      type="button"
      onClick={onClick}
      title={title}
      className={cn(
        "flex items-center justify-center w-7 h-7 rounded-md border-none cursor-pointer transition-colors duration-200",
        isActive
          ? "bg-surface-light text-white"
          : "bg-transparent text-text-secondary hover:text-white hover:bg-surface-light/50"
      )}
    >
      {children}
    </button>
  );
}

export function NavBar() {
  const currentPage = Stores.useUIStore((s) => s.currentPage);
  const setPage = Stores.useUIStore((s) => s.setPage);

  return (
    <nav className="flex items-center gap-0.5">
      <NavButton
        page="sync"
        currentPage={currentPage}
        onClick={() => setPage("sync")}
        title="Sync"
      >
        <Atoms.NavSyncIcon size={16} />
      </NavButton>
      <NavButton
        page="activity"
        currentPage={currentPage}
        onClick={() => setPage("activity")}
        title="Activity"
      >
        <Atoms.NavActivityIcon size={16} />
      </NavButton>
      <NavButton
        page="keys"
        currentPage={currentPage}
        onClick={() => setPage("keys")}
        title="Keys"
      >
        <Atoms.NavKeysIcon size={16} />
      </NavButton>
      <NavButton
        page="settings"
        currentPage={currentPage}
        onClick={() => setPage("settings")}
        title="Settings"
      >
        <Atoms.NavSettingsIcon size={16} />
      </NavButton>
    </nav>
  );
}
