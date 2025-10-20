import { invoke } from "@tauri-apps/api/core";

export class GreetForm {
  constructor(onSuccess) {
    this.onSuccess = onSuccess;
    this.developerModeEnabled = false;
  }

  resetContinueButton() {
    const continueBtn = document.getElementById("continue-btn");
    const spinner = document.getElementById("continue-spinner");
    if (continueBtn) {
      continueBtn.disabled = false;
    }
    if (spinner) {
      spinner.classList.add("hidden");
    }
  }

  async init() {
    this.bindEvents();
    await this.checkDevMode();
    await this.autoLoadDeveloperState();
  }

  bindEvents() {
    const continueBtn = document.getElementById("continue-btn");
    const privateKeyInput = document.getElementById("private-key-input");

    privateKeyInput.addEventListener("focus", () => {
      privateKeyInput.placeholder = "";
    });

    privateKeyInput.addEventListener("input", () => {
      privateKeyInput.placeholder = "";
    });

    continueBtn.addEventListener("click", async (e) => {
      e.preventDefault();
      const privateKeyValue = privateKeyInput.value.trim();
      if (!privateKeyValue) {
        alert("Please enter your private key.");
        return;
      }
      await this.initializeAndStart(privateKeyValue);
    });
  }

  async initializeAndStart(privateKeyValue) {
    const continueBtn = document.getElementById("continue-btn");
    const spinner = document.getElementById("continue-spinner");

    continueBtn.disabled = true;
    if (spinner) {
      spinner.classList.remove("hidden");
    }

    try {
      await invoke("init_app_state", { privateKeyStr: privateKeyValue });
      await invoke("backup_controller_begin");
      this.onSuccess();
      this.resetContinueButton();
    } catch (error) {
      console.error("Internal Error:", error);
      alert(`Error: ${error}`);
      this.resetContinueButton();
      throw error;
    }
  }

  async checkDevMode() {
    try {
      const data = await invoke("fetch_state");
      const devIndicator = document.getElementById("startup-dev-indicator");

      if (data.developer_mode) {
        this.developerModeEnabled = true;
      }

      if (data.developer_mode && devIndicator) {
        devIndicator.classList.remove("hidden");
        console.log("Developer mode is enabled");
      }
    } catch (error) {
      console.error("Error checking dev mode:", error);
    }
  }

  async autoLoadDeveloperState() {
    if (!this.developerModeEnabled) {
      return;
    }

    try {
      await this.initializeAndStart("developer-mode");
    } catch (error) {
      console.error("Internal Error:", error);
    }
  }
}
