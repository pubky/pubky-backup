import { useState, useEffect } from "react";
import * as Atoms from "@/components/atoms";
import * as Stores from "@/stores";
import { cn } from "@/lib/utils";
import { open } from "@tauri-apps/plugin-dialog";
import {
  getConfig,
  setSyncInterval,
  setBackupLocation,
} from "@/services/tauri-commands";
import { handleBackendError } from "@/utils/error-handler";

const SYNC_INTERVALS = [
  { label: "30 sec", value: 30 },
  { label: "5 min", value: 300 },
  { label: "10 min", value: 600 },
  { label: "15 min", value: 900 },
  { label: "30 min", value: 1800 },
  { label: "60 min", value: 3600 },
];

export function SettingsPage() {
  const [backupLocation, setBackupLocationState] = useState<string | null>(
    null,
  );
  const [isMoving, setIsMoving] = useState(false);
  const [selectedInterval, setSelectedInterval] = useState<number | null>(null);
  const setNavDisabled = Stores.useUIStore((s) => s.setNavDisabled);

  // Load current config from backend on mount
  useEffect(() => {
    getConfig()
      .then((config) => {
        setSelectedInterval(config.sync_interval_secs);
        setBackupLocationState(config.keys_dir);
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

  const handleBrowse = async () => {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Choose Backup Location",
    });

    if (!selected) return; // User cancelled

    const previousLocation = backupLocation;
    setIsMoving(true);
    setNavDisabled(true);
    try {
      const newKeysDir = await setBackupLocation(selected);
      setBackupLocationState(newKeysDir);
    } catch (err) {
      handleBackendError(err);
      setBackupLocationState(previousLocation);
    } finally {
      setIsMoving(false);
      setNavDisabled(false);
    }
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
          {isMoving ? (
            <div className="flex items-center bg-[#303034] rounded-lg px-6 py-[18px] gap-6">
              <span className="text-sm font-bold leading-normal text-white">
                Copying data to new location...
              </span>
              <div className="flex-1 h-2 rounded-full bg-black/80 overflow-hidden">
                <div className="h-full rounded-full bg-pubky-purple animate-progress-indeterminate" />
              </div>
            </div>
          ) : (
            <div className="flex items-center bg-black/10 border border-dashed border-border-dashed rounded-lg px-6 py-4 gap-1">
              <span className="flex-1 text-base font-medium leading-normal text-white text-left truncate">
                {backupLocation ?? "Loading..."}
              </span>
              <Atoms.IconButton
                onClick={handleBrowse}
                disabled={backupLocation === null}
                className="w-9 h-9 rounded-full bg-surface-light shadow-[0_1px_2px_rgba(5,5,10,0.2)] flex items-center justify-center"
              >
                <Atoms.FolderIcon size={20} className="text-text-light" />
              </Atoms.IconButton>
            </div>
          )}
        </div>

        {/* Sync Interval */}
        <div className={cn("flex flex-col gap-4", isMoving && "opacity-30 pointer-events-none")}>
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
                  disabled={isLoading || isMoving}
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
