import { handleBackendError } from "@/utils/error-handler";
import {
  fetchState,
  forceSyncNow,
  openDataDir,
  getDataDirPath,
  backupControllerClose,
  createSnapshot,
} from "@/types/tauri-commands";
import { getElementByIdStrict, getElementById } from "@/types/dom-helpers";

export class MainForm {
  private static readonly PUBKEY_DISPLAY_PREFIX_LENGTH = 5;
  private static readonly PUBKEY_DISPLAY_SUFFIX_LENGTH = 5;
  private static readonly STATUS_POLL_INTERVAL_MS = 200;
  private static readonly SYNC_MESSAGE_UPDATE_INTERVAL_MS = 1000;
  private static readonly TOAST_DISPLAY_DURATION_MS = 2000;

  private pubky: string | null = null;
  private developerMode: boolean = false;
  private isSyncing: boolean = false;
  private nextSyncTime: number = 0;
  private dataSize: number = 0;
  private dataDirPath: string | null = null;
  private backupControllerError: string | null = null;
  private lastSyncTime: number | null = null;
  private statusInterval: number | null = null;
  private syncMessageInterval: number | null = null;
  private isCreatingSnapshot: boolean = false;
  private snapshotMessageTimeout: number | null = null;

  init(): void {
    this.bindEvents();
    void this.loadStateOnInit();
    this.startStatusPolling();
    this.startSyncMessageUpdates();
  }

  cleanup(): void {
    this.stopStatusPolling();
    this.stopSyncMessageUpdates();
    this.clearSnapshotMessageTimeout();
  }

  private clearSnapshotMessageTimeout(): void {
    if (this.snapshotMessageTimeout !== null) {
      clearTimeout(this.snapshotMessageTimeout);
      this.snapshotMessageTimeout = null;
    }
  }

  private bindEvents(): void {
    // Copy pubky button
    getElementByIdStrict<HTMLButtonElement>("copy-pubky").addEventListener(
      "click",
      () => {
        if (this.pubky === null) return;

        navigator.clipboard
          .writeText(this.pubky)
          .then(() => {
            if (this.pubky !== null) {
              this.showToast(this.pubky);
            }
          })
          .catch((err: unknown) => {
            console.error("Failed to copy pubky:", err);
          });
      },
    );

    // Back button
    getElementByIdStrict<HTMLButtonElement>("back-btn").addEventListener(
      "click",
      () => {
        void this.returnToStartup();
      },
    );

    // Force sync button
    getElementByIdStrict<HTMLButtonElement>("force-sync-btn").addEventListener(
      "click",
      async () => {
        const forceSyncBtn =
          getElementByIdStrict<HTMLButtonElement>("force-sync-btn");
        try {
          forceSyncBtn.classList.add("activated");
          forceSyncBtn.disabled = true;
          await forceSyncNow();
          console.log("Force sync triggered");
          // Button will be re-enabled when sync status updates
        } catch (error: unknown) {
          console.error("Force sync failed:", error);
          forceSyncBtn.classList.remove("activated");
          forceSyncBtn.disabled = false;
        }
      },
    );

    // Open data directory button
    getElementByIdStrict<HTMLButtonElement>("open-data-dir").addEventListener(
      "click",
      async () => {
        try {
          await openDataDir();
        } catch (error: unknown) {
          console.error("Failed to open data directory:", error);
        }
      },
    );

    // Snapshot button
    getElementByIdStrict<HTMLButtonElement>("snapshot-btn").addEventListener(
      "click",
      async () => {
        const snapshotBtn =
          getElementByIdStrict<HTMLButtonElement>("snapshot-btn");
        try {
          snapshotBtn.disabled = true;
          this.isCreatingSnapshot = true;
          this.updateSnapshotButtonState();
          const snapshotPath = await createSnapshot();
          console.log("Snapshot created:", snapshotPath);
          this.showSnapshotSuccess();
        } catch (error: unknown) {
          console.error("Snapshot creation failed:", error);
          this.showSnapshotError();
        } finally {
          this.isCreatingSnapshot = false;
          snapshotBtn.disabled = false;
          this.updateSnapshotButtonState();
        }
      },
    );
  }

  private async loadStateOnInit(): Promise<void> {
    try {
      const data = await fetchState();
      this.pubky = data.pubky;
      this.developerMode = data.developer_mode;
      this.isSyncing = data.is_syncing;
      this.nextSyncTime = data.next_sync_time;
      this.dataSize = data.data_dir_size || 0;
      this.backupControllerError = data.backup_controller_error;
      this.setHeader();
      this.updateSyncStatus();
      this.updateNextSyncCountdown();
      this.updateBackupSize();
      this.updateLastSync();
      void this.loadDataDirPath();
    } catch (error: unknown) {
      console.error("Error loading initial state:", error);
      getElementByIdStrict<HTMLElement>("main-form").classList.add("hidden");
    }
  }

  private displayPubky(
    str: string | null,
    length: number = MainForm.PUBKEY_DISPLAY_PREFIX_LENGTH,
  ): string {
    if (str === null) return "...";
    const endLength = MainForm.PUBKEY_DISPLAY_SUFFIX_LENGTH;
    if (str.length <= length + endLength + 3) return str;
    return (
      str.substring(0, length) + "..." + str.substring(str.length - endLength)
    );
  }

  private setHeader(): void {
    const backupHeader = getElementByIdStrict<HTMLElement>("main-form");
    const pubkyDisplay = getElementByIdStrict<HTMLElement>("pubky-display");
    if (this.pubky !== null) {
      pubkyDisplay.textContent = this.displayPubky(this.pubky);
      backupHeader.classList.remove("hidden");
    } else {
      console.log("Failed to find State data");
    }

    const devIndicator = getElementById<HTMLElement>("dev-indicator");
    if (devIndicator !== null) {
      if (this.developerMode) {
        devIndicator.classList.remove("hidden");
        console.log("Developer mode is enabled");
      } else {
        devIndicator.classList.add("hidden");
      }
    }
  }

  private startStatusPolling(): void {
    this.statusInterval = window.setInterval(() => {
      void this.fetchAndUpdateStatus();
    }, MainForm.STATUS_POLL_INTERVAL_MS);
  }

  private stopStatusPolling(): void {
    if (this.statusInterval !== null) {
      clearInterval(this.statusInterval);
      this.statusInterval = null;
    }
  }

  private async fetchAndUpdateStatus(): Promise<void> {
    try {
      const data = await fetchState();

      const previousNextSyncTime = this.nextSyncTime;
      this.isSyncing = data.is_syncing;
      this.nextSyncTime = data.next_sync_time;
      this.dataSize = data.data_dir_size || 0;
      this.backupControllerError = data.backup_controller_error;

      // Update lastSyncTime when sync completes
      // We detect this when next_sync_time gets updated to a new future timestamp (now + 30)
      // TODO: This needs work when poll time is settable by user
      const now = Math.floor(Date.now() / 1000);
      if (
        !this.isSyncing &&
        this.nextSyncTime > previousNextSyncTime &&
        this.nextSyncTime > now
      ) {
        // Sync just completed, next sync scheduled for 30 seconds from now
        this.lastSyncTime = now;
      }

      this.updateSyncStatus();
      this.updateBackupSize();
      this.updateLastSync();
      if (this.backupControllerError !== null) {
        handleBackendError(this.backupControllerError);
        void this.returnToStartup();
      }
    } catch (error: unknown) {
      console.error("Error fetching status:", error);
    }
  }

  private updateSyncStatus(): void {
    const statusBadge = getElementByIdStrict<HTMLElement>("status-badge");
    const statusText = getElementByIdStrict<HTMLElement>("status-text");
    const syncMessage = getElementByIdStrict<HTMLElement>("sync-message");
    const syncMessageText =
      getElementByIdStrict<HTMLElement>("sync-message-text");
    const syncMessageIcon =
      getElementByIdStrict<HTMLElement>("sync-message-icon");
    const syncSpinnerIcon =
      getElementByIdStrict<HTMLElement>("sync-spinner-icon");
    const forceSyncBtn =
      getElementByIdStrict<HTMLButtonElement>("force-sync-btn");

    if (this.isSyncing) {
      // Update status badge
      statusBadge.classList.remove("synced");
      statusBadge.classList.add("syncing");
      statusText.textContent = "SYNCING";

      // Update sync message
      syncMessage.classList.remove("error");
      syncMessage.classList.add("syncing");
      syncMessageText.textContent = "Syncing data...";

      // Show spinner, hide check icon
      syncMessageIcon.classList.add("hidden");
      syncSpinnerIcon.classList.remove("hidden");

      // Keep force sync button disabled and activated while syncing
      forceSyncBtn.disabled = true;
    } else {
      // Update status badge
      statusBadge.classList.remove("syncing");
      statusBadge.classList.add("synced");
      statusText.textContent = "SYNCED";

      // Update sync message
      syncMessage.classList.remove("syncing", "error");

      // Show check icon, hide spinner
      syncMessageIcon.classList.remove("hidden");
      syncSpinnerIcon.classList.add("hidden");

      // Enable force sync button and remove activated state
      forceSyncBtn.disabled = false;
      forceSyncBtn.classList.remove("activated");
    }

    // Update snapshot button state
    this.updateSnapshotButtonState();
  }

  private updateNextSyncCountdown(): void {
    if (this.isSyncing) {
      return;
    }

    const syncMessageText =
      getElementByIdStrict<HTMLElement>("sync-message-text");
    const now = Math.floor(Date.now() / 1000);

    if (this.nextSyncTime > now) {
      const remaining = this.nextSyncTime - now;
      const minutes = Math.floor(remaining / 60);
      const seconds = remaining % 60;

      if (minutes > 0) {
        syncMessageText.textContent = `Next backup in ${minutes} minute${minutes !== 1 ? "s" : ""}...`;
      } else if (seconds > 0) {
        syncMessageText.textContent = `Next backup in ${seconds} second${seconds !== 1 ? "s" : ""}...`;
      } else {
        syncMessageText.textContent = "Syncing soon...";
      }
    } else {
      syncMessageText.textContent = "Data synchronized";
    }
  }

  private updateBackupSize(): void {
    const backupSizeValue =
      getElementByIdStrict<HTMLElement>("backup-size-value");
    backupSizeValue.textContent = this.formatFileSize(this.dataSize);
  }

  private updateLastSync(): void {
    const lastSyncValue = getElementByIdStrict<HTMLElement>("last-sync-value");
    if (this.lastSyncTime !== null && this.lastSyncTime > 0) {
      lastSyncValue.textContent = this.formatTimestamp(this.lastSyncTime);
    } else {
      lastSyncValue.textContent = "--";
    }
  }

  private formatTimestamp(unixTimestamp: number): string {
    const date = new Date(unixTimestamp * 1000);
    const hours = date.getHours();
    const minutes = date.getMinutes().toString().padStart(2, "0");
    const seconds = date.getSeconds().toString().padStart(2, "0");
    const ampm = hours >= 12 ? "PM" : "AM";
    const displayHours = hours % 12 || 12;
    return `${displayHours}:${minutes}:${seconds} ${ampm}`;
  }

  private async loadDataDirPath(): Promise<void> {
    try {
      this.dataDirPath = await getDataDirPath();
      const dataDirValue = getElementByIdStrict<HTMLElement>("data-dir-value");
      if (this.dataDirPath !== null) {
        dataDirValue.textContent = this.dataDirPath;
      } else {
        dataDirValue.textContent = "--";
      }
    } catch (error: unknown) {
      console.error("Error loading data directory path:", error);
      const dataDirValue = getElementByIdStrict<HTMLElement>("data-dir-value");
      dataDirValue.textContent = "Error";
    }
  }

  private formatFileSize(bytes: number): string {
    if (bytes === 0) return "0 B";

    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB", "TB"] as const;
    const i = Math.floor(Math.log(bytes) / Math.log(k));

    const size = bytes / Math.pow(k, i);
    const decimals = i === 0 ? 0 : size < 10 ? 2 : 1;

    return `${size.toFixed(decimals)} ${sizes[i] ?? "TB"}`;
  }

  private startSyncMessageUpdates(): void {
    // Update sync message with next backup countdown
    this.syncMessageInterval = window.setInterval(() => {
      this.updateNextSyncCountdown();
    }, MainForm.SYNC_MESSAGE_UPDATE_INTERVAL_MS);
  }

  private stopSyncMessageUpdates(): void {
    if (this.syncMessageInterval !== null) {
      clearInterval(this.syncMessageInterval);
      this.syncMessageInterval = null;
    }
  }

  private showToast(pubkyText: string): void {
    const toast = getElementByIdStrict<HTMLElement>("toast");
    const toastDescription =
      getElementByIdStrict<HTMLElement>("toast-description");
    toastDescription.textContent = this.displayPubky(pubkyText);
    toast.classList.add("show");

    // Hide the toast after duration
    setTimeout(() => {
      toast.classList.remove("show");
    }, MainForm.TOAST_DISPLAY_DURATION_MS);
  }

  private async returnToStartup(): Promise<void> {
    try {
      // Try to stop backup controller, but don't fail if it's already stopped
      try {
        await backupControllerClose();
      } catch (closeError: unknown) {
        console.error("Backup controller already stopped:", closeError);
      }

      this.stopStatusPolling();
      this.stopSyncMessageUpdates();

      // Navigate back to startup screen
      document.querySelectorAll(".screen").forEach((screen) => {
        screen.classList.add("hidden");
      });
      getElementByIdStrict<HTMLElement>("startup-screen").classList.remove(
        "hidden",
      );
    } catch (error: unknown) {
      console.error("Error returning to startup:", error);
    }
  }

  private updateSnapshotButtonState(): void {
    const snapshotBtn =
      getElementByIdStrict<HTMLButtonElement>("snapshot-btn");

    // Disable snapshot button during sync or while creating snapshot
    if (this.isSyncing || this.isCreatingSnapshot) {
      snapshotBtn.disabled = true;
    } else {
      snapshotBtn.disabled = false;
    }

    // Add visual state class when creating snapshot
    if (this.isCreatingSnapshot) {
      snapshotBtn.classList.add("creating");
    } else {
      snapshotBtn.classList.remove("creating");
    }
  }

  private showSnapshotSuccess(): void {
    const syncMessage = getElementByIdStrict<HTMLElement>("sync-message");
    const syncMessageText =
      getElementByIdStrict<HTMLElement>("sync-message-text");

    // Clear any existing snapshot message timeout
    this.clearSnapshotMessageTimeout();

    // Show success state
    syncMessage.classList.remove("error", "syncing");
    syncMessage.classList.add("snapshot-success");
    syncMessageText.textContent = "Snapshot created";

    // Revert after 3 seconds
    this.snapshotMessageTimeout = window.setTimeout(() => {
      syncMessage.classList.remove("snapshot-success");
      this.updateNextSyncCountdown();
      this.snapshotMessageTimeout = null;
    }, 3000);
  }

  private showSnapshotError(): void {
    const syncMessage = getElementByIdStrict<HTMLElement>("sync-message");
    const syncMessageText =
      getElementByIdStrict<HTMLElement>("sync-message-text");

    // Clear any existing snapshot message timeout
    this.clearSnapshotMessageTimeout();

    // Show error state
    syncMessage.classList.remove("syncing", "snapshot-success");
    syncMessage.classList.add("error");
    syncMessageText.textContent = "Failed to create snapshot";

    // Revert after 3 seconds
    this.snapshotMessageTimeout = window.setTimeout(() => {
      syncMessage.classList.remove("error");
      this.updateNextSyncCountdown();
      this.snapshotMessageTimeout = null;
    }, 3000);
  }
}
