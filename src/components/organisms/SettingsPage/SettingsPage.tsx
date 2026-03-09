import { useState, useEffect } from "react";
import * as Atoms from "@/components/atoms";
import { cn } from "@/lib/utils";
import { getConfig, setSyncInterval } from "@/services/tauri-commands";

const SYNC_INTERVALS = [
  { label: "30 sec", value: 30 },
  { label: "5 min", value: 300 },
  { label: "10 min", value: 600 },
  { label: "15 min", value: 900 },
  { label: "30 min", value: 1800 },
  { label: "60 min", value: 3600 },
];

export function SettingsPage() {
  const [backupLocation, _setBackupLocation] = useState(
    "/Documents/PubkyBackup/",
  );
  const [selectedInterval, setSelectedInterval] = useState<number | null>(null);

  // Load current interval from backend on mount
  useEffect(() => {
    getConfig()
      .then((config) => {
        setSelectedInterval(config.sync_interval_secs);
      })
      .catch((err) => {
        console.error("Failed to load config:", err);
      });
  }, []);

  const handleIntervalChange = async (secs: number) => {
    const previousInterval = selectedInterval;
    setSelectedInterval(secs);
    try {
      await setSyncInterval(secs);
    } catch (err) {
      console.error("Failed to set sync interval:", err);
      setSelectedInterval(previousInterval);
    }
  };

  const handleBrowse = () => {
    // TODO: wire to Tauri file dialog
  };

  return (
    <div className="flex flex-col gap-6 flex-1">
      {/* Header */}
      <div className="flex flex-col gap-1">
        <h2 className="text-xl font-bold leading-normal text-white text-left">
          Settings
        </h2>
        <p className="text-base font-medium leading-normal text-text-light text-left">
          Configure your backup preferences.
        </p>
      </div>

      {/* Settings content */}
      <div className="flex flex-col gap-6">
        {/* Backup location */}
        <div className="flex flex-col gap-2">
          <label className="text-xs font-medium tracking-[0.1em] uppercase text-text-secondary text-left">
            Choose Backup location
          </label>
          <div className="flex items-center bg-black/10 border border-dashed border-border-dashed rounded-lg px-6 py-4 gap-1">
            <span className="flex-1 text-base font-medium leading-normal text-white text-left truncate">
              {backupLocation}
            </span>
            <Atoms.IconButton
              onClick={handleBrowse}
              className="w-9 h-9 rounded-full bg-surface-light shadow-[0_1px_2px_rgba(5,5,10,0.2)] flex items-center justify-center"
            >
              <Atoms.FolderIcon size={20} className="text-text-light" />
            </Atoms.IconButton>
          </div>
        </div>

        {/* Sync Interval */}
        <div className="flex flex-col gap-4">
          <label className="text-xs font-medium tracking-[0.1em] uppercase text-text-secondary text-left">
            Sync Interval
          </label>
          <div className="grid grid-cols-3 gap-4">
            {SYNC_INTERVALS.map(({ label, value }) => {
              const isLoading = selectedInterval === null;
              const isSelected = selectedInterval === value;
              return (
                <button
                  key={value}
                  type="button"
                  disabled={isLoading}
                  onClick={() => handleIntervalChange(value)}
                  className={cn(
                    "flex items-center justify-center gap-2 h-10 rounded-full transition-all duration-200",
                    "shadow-[0_1px_2px_rgba(5,5,10,0.2)]",
                    "bg-pubky-purple/15",
                    isLoading
                      ? "opacity-50 cursor-not-allowed"
                      : "cursor-pointer",
                    isSelected
                      ? "border border-pubky-purple"
                      : "border border-border hover:border-pubky-purple/50",
                  )}
                >
                  {isSelected && (
                    <Atoms.CheckmarkIcon
                      size={16}
                      className="text-pubky-purple"
                    />
                  )}
                  <span
                    className={cn(
                      "text-sm leading-normal",
                      isSelected
                        ? "font-bold text-pubky-purple"
                        : "font-normal text-white",
                    )}
                  >
                    {label}
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      </div>

    </div>
  );
}
