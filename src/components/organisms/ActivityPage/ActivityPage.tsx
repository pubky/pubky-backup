import { useEffect, useState, useCallback } from "react";
import { useUIStore } from "@/stores/uiStore";
import type { ActivityEntry } from "@/stores/uiStore";
import { getActivity, openSnapshotsDir } from "@/services/tauri-commands";
import { formatRelativeTime } from "@/utils/format";
import * as Atoms from "@/components/atoms";

const MOCK_ACTIVITY: ActivityEntry[] = [
  { type: "files_backed_up", message: "35 new files backed up", timestamp: Math.floor(Date.now() / 1000) - 60 },
  { type: "files_backed_up", message: "10 new files backed up", timestamp: Math.floor(Date.now() / 1000) - 300 },
  { type: "snapshot_created", message: "Created snapshot v14", timestamp: Math.floor(Date.now() / 1000) - 420 },
  { type: "sync_failed", message: "no internet connection", timestamp: Math.floor(Date.now() / 1000) - 600 },
  { type: "initial_backup", message: "Initial backup successful", timestamp: Math.floor(Date.now() / 1000) - 1800 },
  { type: "files_backed_up", message: "35 new files backed up", timestamp: Math.floor(Date.now() / 1000) - 60 },
  { type: "files_backed_up", message: "10 new files backed up", timestamp: Math.floor(Date.now() / 1000) - 300 },
  { type: "snapshot_created", message: "Created snapshot v14", timestamp: Math.floor(Date.now() / 1000) - 420 },
  { type: "sync_failed", message: "no internet connection", timestamp: Math.floor(Date.now() / 1000) - 600 },
  { type: "initial_backup", message: "Initial backup successful", timestamp: Math.floor(Date.now() / 1000) - 1800 },
];

export function ActivityPage() {
  const [entries, setEntries] = useState<ActivityEntry[]>([]);
  const lastPubky = useUIStore((s) => s.lastPubky);
  const developerMode = useUIStore((s) => s.developerMode);
  const keyState = useUIStore((s) =>
    s.lastPubky ? s.keyStates[s.lastPubky] : undefined,
  );

  const fetchActivity = useCallback(async () => {
    if (!lastPubky) return;
    if (developerMode) {
      setEntries(MOCK_ACTIVITY);
      return;
    }
    try {
      const result = await getActivity(lastPubky);
      setEntries(result);
    } catch {
      // Silently ignore - activity is non-critical
    }
  }, [lastPubky, developerMode]);

  // Fetch on mount and when lastPubky changes
  useEffect(() => {
    fetchActivity();
  }, [fetchActivity]);

  // Refetch when key transitions to Idle
  useEffect(() => {
    if (keyState?.status.type === "Idle") {
      fetchActivity();
    }
  }, [keyState?.status.type, fetchActivity]);

  if (!lastPubky) {
    return null;
  }

  return (
    <div className="flex flex-col gap-4 text-left self-stretch">
      <div className="flex flex-col gap-1">
        <h2 className="text-xl font-bold text-white m-0">Activity</h2>
        <p className="text-base font-medium text-text-light m-0">
          All manual and automatic sync attempts and related errors are recorded
          and displayed here for review.
        </p>
      </div>
      <div className="flex flex-col gap-0.5 overflow-y-auto max-h-[300px] pr-2">
        {entries.length === 0 ? (
          <p className="text-sm text-text-secondary text-center py-8">
            No activity yet
          </p>
        ) : (
          entries.map((entry, i) => (
            <div
              key={`${entry.timestamp}-${i}`}
              className="flex flex-col gap-0 py-2 border-b border-border"
            >
              <div className="flex gap-4 items-center">
                <div
                  className={`size-2 rounded-full shrink-0 ${
                    entry.type === "sync_failed"
                      ? "bg-red-500"
                      : "bg-green-500"
                  }`}
                />
                <span className="text-sm font-bold text-white leading-5">
                  {entry.type === "sync_failed" ? (
                    <>
                      Sync failed{" "}
                      <span className="font-normal text-text-secondary">
                        ({entry.message})
                      </span>
                    </>
                  ) : (
                    entry.message
                  )}
                </span>
                {entry.type === "snapshot_created" && (
                  <button
                    type="button"
                    title="Open snapshots folder"
                    onClick={() => void openSnapshotsDir(lastPubky)}
                    className="shrink-0 ml-[-8px] text-text-secondary hover:text-white transition-colors cursor-pointer bg-transparent border-none p-0"
                  >
                    <Atoms.ExternalLinkIcon size={16} />
                  </button>
                )}
              </div>
              <span className="text-sm text-text-secondary leading-5 pl-6">
                {formatRelativeTime(entry.timestamp)}
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
