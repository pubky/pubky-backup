import { useEffect, useState, useCallback } from "react";
import { useUIStore } from "@/stores/uiStore";
import type { ActivityEntry } from "@/stores/uiStore";
import { getActivity } from "@/services/tauri-commands";
import { formatRelativeTime } from "@/utils/format";

export function ActivityPage() {
  const [entries, setEntries] = useState<ActivityEntry[]>([]);
  const lastPubky = useUIStore((s) => s.lastPubky);
  const keyState = useUIStore((s) =>
    s.lastPubky ? s.keyStates[s.lastPubky] : undefined,
  );

  const fetchActivity = useCallback(async () => {
    if (!lastPubky) return;
    try {
      const result = await getActivity(lastPubky);
      setEntries(result);
    } catch {
      // Silently ignore - activity is non-critical
    }
  }, [lastPubky]);

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
    <div className="flex flex-col gap-0">
      <p className="text-xs text-text-secondary mb-4">
        All manual and automatic sync attempts and related errors are recorded
        and displayed here for review.
      </p>
      {entries.length === 0 ? (
        <p className="text-sm text-text-secondary text-center py-8">
          No activity yet
        </p>
      ) : (
        entries.map((entry, i) => (
          <div
            key={`${entry.timestamp}-${i}`}
            className="flex items-start gap-3 py-3 border-b border-white/10 last:border-b-0"
          >
            <div
              className={`w-2 h-2 rounded-full mt-1.5 shrink-0 ${
                entry.type === "sync_failed" ? "bg-red-500" : "bg-green-500"
              }`}
            />
            <div className="flex flex-col gap-0.5 min-w-0">
              <span className="text-sm text-white font-semibold">
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
              <span className="text-xs text-text-secondary">
                {formatRelativeTime(entry.timestamp)}
              </span>
            </div>
          </div>
        ))
      )}
    </div>
  );
}
