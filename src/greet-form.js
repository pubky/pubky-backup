import { invoke } from "@tauri-apps/api/core";

export class GreetForm {
  constructor(onSuccess) {
    this.onSuccess = onSuccess;
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
    await this.autoLoadLastPubky();
    await this.loadPreviousKeys();
  }

  bindEvents() {
    const continueBtn = document.getElementById("continue-btn");
    const pubkyInput = document.getElementById("pubky-input");
    const inputWrapper = pubkyInput.closest(".input-wrapper");

    // Update button and input state based on input value
    const updateButtonState = () => {
      const hasValue = pubkyInput.value.trim().length > 0;

      if (hasValue) {
        // Filled state
        continueBtn.style.opacity = "1";
        continueBtn.disabled = false;
        if (inputWrapper) {
          inputWrapper.classList.add("filled");
        }
      } else {
        // Empty state
        continueBtn.style.opacity = "0.3";
        continueBtn.disabled = true;
        if (inputWrapper) {
          inputWrapper.classList.remove("filled");
        }
      }
    };

    // Clear placeholder on focus or input
    pubkyInput.addEventListener("focus", () => {
      pubkyInput.placeholder = "";
    });

    pubkyInput.addEventListener("input", () => {
      pubkyInput.placeholder = "";
      updateButtonState();
    });

    continueBtn.addEventListener("click", async (e) => {
      e.preventDefault();
      const pubkyValue = pubkyInput.value.trim();
      await this.initializeAndStart(pubkyValue);
    });

    // Set initial button state
    updateButtonState();
  }

  // Load main-form, beginning backup process
  async initializeAndStart(pubkyValue) {
    const continueBtn = document.getElementById("continue-btn");
    const spinner = document.getElementById("continue-spinner");

    // Show loading spinner
    continueBtn.disabled = true;
    if (spinner) {
      spinner.classList.remove("hidden");
    }

    try {
      await invoke("init_app_state", { pubkyStr: pubkyValue });
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

      if (data.developer_mode && devIndicator) {
        devIndicator.classList.remove("hidden");
        console.log("Developer mode is enabled");
      }
    } catch (error) {
      console.error("Error checking dev mode:", error);
    }
  }

  // Fetch list of keys previously used and provide as suggestions
  async loadPreviousKeys() {
    const pubkyInput = document.getElementById("pubky-input");

    try {
      const previousKeys = await invoke("get_previous_pubky_keys");
      const datalist = document.getElementById("previous-keys");

      datalist.innerHTML = "";
      if (previousKeys && previousKeys.length > 0) {
        previousKeys.forEach((key) => {
          const option = document.createElement("option");
          option.value = key;
          datalist.appendChild(option);
        });

        console.log(`Loaded ${previousKeys.length} previous keys`);
      } else {
        // No previous keys found, display example key
        pubkyInput.placeholder = "g1b6wp8bhhxt...";
      }
    } catch (error) {
      console.error("Error loading previous keys:", error);
      // If we can't load previous keys just use the default placeholder
      if (pubkyInput.value === "") {
        pubkyInput.placeholder = "g1b6wp8bhhxt...";
      }
    }
  }

  // If a last_pubky exists then automatically move on to main-from
  async autoLoadLastPubky() {
    try {
      const lastPubky = await invoke("get_last_pubky");
      if (lastPubky) {
        console.log("Auto-loading last used pubky:", lastPubky);
        const pubkyInput = document.getElementById("pubky-input");
        pubkyInput.value = lastPubky;

        try {
          await this.initializeAndStart(lastPubky);
        } catch (error) {
          console.error("Internal Error:", error);
        }
      }
    } catch (error) {
      console.error("Error checking for last pubky:", error);
    }
  }
}
