import * as Atoms from "@/components/atoms";

export function ActivityPage() {
  return (
    <div className="flex flex-col items-center justify-center py-12 text-text-secondary flex-1">
      <Atoms.NavActivityIcon size={48} className="mb-4 opacity-50" />
      <h2 className="text-lg font-semibold text-white mb-2">Activity</h2>
      <p className="text-sm">Coming soon</p>
    </div>
  );
}
