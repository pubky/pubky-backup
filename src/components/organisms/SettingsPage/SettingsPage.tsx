import * as Atoms from "@/components/atoms";

export function SettingsPage() {
  return (
    <div className="flex flex-col items-center justify-center py-12 text-text-secondary flex-1">
      <Atoms.NavSettingsIcon size={48} className="mb-4 opacity-50" />
      <h2 className="text-lg font-semibold text-white mb-2">Settings</h2>
      <p className="text-sm">Coming soon</p>
    </div>
  );
}
