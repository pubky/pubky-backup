import { invoke } from "@tauri-apps/api/core";

export class MainForm {
  constructor() {
    this.pubky = null;
    this.homeserver = null;
    this.developerMode = false;
    this.isSyncing = false;
    this.nextSyncTime = 0;
    this.dataSize = 0;
    this.dataDirPath = null;
    this.backupControllerError = null;
    this.lastSyncTime = null;
    this.statusInterval = null;
    this.countdownInterval = null;
  }

  init() {
    this.bindEvents();
    this.loadStateOnInit();
    this.startStatusPolling();
    this.startCountdown();
  }

  bindEvents() {
    // Copy pubky button
    document.getElementById("copy-pubky").addEventListener("click", () => {
      navigator.clipboard
        .writeText(this.pubky)
        .then(() => {
          console.log("Pubky copied to clipboard");
        })
        .catch((err) => {
          console.error("Failed to copy pubky:", err);
        });
    });

    // Back button
    document.getElementById("back-btn").addEventListener("click", async () => {
      await this.returnToStartup();
    });

    // Force sync button
    document
      .getElementById("force-sync-btn")
      .addEventListener("click", async () => {
        try {
          await invoke("force_sync_now");
          console.log("Force sync triggered");
        } catch (error) {
          console.error("Force sync failed:", error);
        }
      });

    // Open data directory button
    document
      .getElementById("open-data-dir")
      .addEventListener("click", async () => {
        try {
          await invoke("open_data_dir");
          console.log("Data directory opened");
        } catch (error) {
          console.error("Failed to open data directory:", error);
        }
      });
  }

  async loadStateOnInit() {
    try {
      const data = await invoke("fetch_state");
      this.pubky = data.pubky;
      this.homeserver = data.homeserver;
      this.developerMode = data.developer_mode;
      this.isSyncing = data.is_syncing;
      this.nextSyncTime = data.next_sync_time;
      this.dataSize = data.data_dir_size || 0;
      this.backupControllerError = data.backup_controller_error;
      this.lastSyncTime = data.last_sync_time || null;
      this.setHeader();
      this.updateSyncStatus();
      this.updateBackupSize();
      this.updateLastSync();
      this.loadDataDirPath();
    } catch (error) {
      console.error("Error loading initial state:", error);
      document.getElementById("main-form").classList.add("hidden");
    }
  }

  displayPubky(str, length = 5) {
    if (!str) return "...";
    const endLength = 5;
    if (str.length <= length + endLength + 3) return str;
    return str.substring(0, length) + "..." + str.substring(str.length - endLength);
  }

  setHeader() {
    const backupHeader = document.getElementById("main-form");
    const pubkyDisplay = document.getElementById("pubky-display");
    if (this.pubky) {
      pubkyDisplay.textContent = this.displayPubky(this.pubky);
      backupHeader.classList.remove("hidden");
    } else {
      console.log(`Failed to find State data`);
    }

    const devIndicator = document.getElementById("dev-indicator");
    if (this.developerMode && devIndicator) {
      devIndicator.classList.remove("hidden");
      console.log("Developer mode is enabled");
    } else if (devIndicator) {
      devIndicator.classList.add("hidden");
    }
  }

  startStatusPolling() {
    // Poll for status updates every 200 ms
    this.statusInterval = setInterval(async () => {
      await this.fetchAndUpdateStatus();
    }, 200);
  }

  stopStatusPolling() {
    if (this.statusInterval) {
      clearInterval(this.statusInterval);
      this.statusInterval = null;
    }
  }

  async fetchAndUpdateStatus() {
    try {
      const data = await invoke("fetch_state");

      this.isSyncing = data.is_syncing;
      this.nextSyncTime = data.next_sync_time;
      this.dataSize = data.data_dir_size || 0;
      this.backupControllerError = data.backup_controller_error;
      this.lastSyncTime = data.last_sync_time || null;
      this.updateSyncStatus();
      this.updateBackupSize();
      this.updateLastSync();
      if (this.backupControllerError) {
        alert(`Internal Error: ${this.backupControllerError}`);
        this.returnToStartup();
      }
    } catch (error) {
      console.error("Error fetching status:", error);
    }
  }

  updateSyncStatus() {
    const statusBadge = document.getElementById("status-badge");
    const statusText = document.getElementById("status-text");
    const syncMessage = document.getElementById("sync-message");
    const syncMessageText = document.getElementById("sync-message-text");
    const forceSyncBtn = document.getElementById("force-sync-btn");

    if (this.isSyncing) {
      // Update status badge
      statusBadge.classList.remove("synced");
      statusBadge.classList.add("syncing");
      statusText.textContent = "SYNCING";

      // Update sync message
      syncMessage.classList.add("syncing");
      syncMessageText.textContent = "Syncing data...";

      // Disable force sync button while syncing
      forceSyncBtn.disabled = true;
    } else {
      // Update status badge
      statusBadge.classList.remove("syncing");
      statusBadge.classList.add("synced");
      statusText.textContent = "SYNCED";

      // Update sync message
      syncMessage.classList.remove("syncing");
      syncMessageText.textContent = "Data synchronized";

      // Enable force sync button
      forceSyncBtn.disabled = false;
    }
  }

  updateBackupSize() {
    const backupSizeValue = document.getElementById("backup-size-value");
    backupSizeValue.textContent = this.formatFileSize(this.dataSize);
  }

  updateLastSync() {
    const lastSyncValue = document.getElementById("last-sync-value");
    if (this.lastSyncTime && this.lastSyncTime > 0) {
      lastSyncValue.textContent = this.formatTimestamp(this.lastSyncTime);
    } else {
      lastSyncValue.textContent = "--";
    }
  }

  formatTimestamp(unixTimestamp) {
    const date = new Date(unixTimestamp * 1000);
    const hours = date.getHours();
    const minutes = date.getMinutes().toString().padStart(2, "0");
    const seconds = date.getSeconds().toString().padStart(2, "0");
    const ampm = hours >= 12 ? "PM" : "AM";
    const displayHours = hours % 12 || 12;
    return `${displayHours}:${minutes}:${seconds} ${ampm}`;
  }

  async loadDataDirPath() {
    try {
      this.dataDirPath = await invoke("get_data_dir_path");
      const dataDirValue = document.getElementById("data-dir-value");
      if (this.dataDirPath) {
        dataDirValue.textContent = this.dataDirPath;
      } else {
        dataDirValue.textContent = "--";
      }
    } catch (error) {
      console.error("Error loading data directory path:", error);
      const dataDirValue = document.getElementById("data-dir-value");
      dataDirValue.textContent = "Error";
    }
  }

  formatFileSize(bytes) {
    if (bytes === 0) return "0 B";

    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB", "TB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));

    const size = bytes / Math.pow(k, i);
    const decimals = i === 0 ? 0 : size < 10 ? 2 : 1;

    return `${size.toFixed(decimals)} ${sizes[i]}`;
  }

  startCountdown() {
    // Update countdown every second
    this.countdownInterval = setInterval(() => {
      this.updateCountdown();
    }, 1000);
  }

  stopCountdown() {
    if (this.countdownInterval) {
      clearInterval(this.countdownInterval);
      this.countdownInterval = null;
    }
  }

  updateCountdown() {
    const countdownElement = document.getElementById("countdown-timer");
    const now = Math.floor(Date.now() / 1000);

    if (this.nextSyncTime > now) {
      const remaining = this.nextSyncTime - now;
      const minutes = Math.floor(remaining / 60);
      const seconds = remaining % 60;

      if (minutes > 0) {
        countdownElement.textContent = `${minutes}m ${seconds}s`;
      } else {
        countdownElement.textContent = `${seconds}s`;
      }
    } else {
      countdownElement.textContent = "0s";
    }
  }

  async returnToStartup() {
    try {
      // Try to stop backup controller, but don't fail if it's already stopped
      try {
        await invoke("backup_controller_close");
      } catch (closeError) {
        console.log("Backup controller already stopped:", closeError);
      }

      this.stopStatusPolling();
      this.stopCountdown();

      // Navigate back to startup screen
      document.querySelectorAll(".screen").forEach((screen) => {
        screen.classList.add("hidden");
      });
      document.getElementById("startup-screen").classList.remove("hidden");
    } catch (error) {
      console.error("Error returning to startup:", error);
    }
  }
}
