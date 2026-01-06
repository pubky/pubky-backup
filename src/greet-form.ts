import { handleBackendError } from "@/utils/error-handler";
import {
  initAppState,
  backupControllerBegin,
  getPreviousPubkyKeys,
  getLastPubky,
} from "@/types/tauri-commands";
import { getElementById, getElementByIdStrict } from "@/types/dom-helpers";

type OnSuccessCallback = () => void;

export class GreetForm {
  private readonly onSuccess: OnSuccessCallback;

  constructor(onSuccess: OnSuccessCallback) {
    this.onSuccess = onSuccess;
  }

  private resetContinueButton(): void {
    const continueBtn = getElementById<HTMLButtonElement>("continue-btn");
    if (continueBtn === null) return;

    continueBtn.disabled = false;
    continueBtn.classList.remove("activated");

    // Restore opacity based on input value
    const pubkyInput = getElementById<HTMLInputElement>("pubky-input");
    if (pubkyInput !== null && pubkyInput.value.trim().length > 0) {
      continueBtn.style.opacity = "1";
    } else {
      continueBtn.style.opacity = "0.3";
    }
  }

  async init(): Promise<void> {
    this.bindEvents();
    await this.autoLoadLastPubky();
    await this.loadPreviousKeys();
  }

  private bindEvents(): void {
    const continueBtn = getElementByIdStrict<HTMLButtonElement>("continue-btn");
    const pubkyInput = getElementByIdStrict<HTMLInputElement>("pubky-input");
    const inputWrapper = pubkyInput.closest(".input-wrapper");
    if (inputWrapper === null) {
      console.error("Input wrapper element not found");
    }

    // Update button and input state based on input value
    const updateButtonState = (): void => {
      const hasValue = pubkyInput.value.trim().length > 0;

      if (hasValue) {
        // Filled state
        continueBtn.style.opacity = "1";
        continueBtn.disabled = false;
        if (inputWrapper !== null) {
          inputWrapper.classList.add("filled");
        }
      } else {
        // Empty state
        continueBtn.style.opacity = "0.3";
        continueBtn.disabled = true;
        if (inputWrapper !== null) {
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

    continueBtn.addEventListener("click", async (e: MouseEvent) => {
      e.preventDefault();
      const pubkyValue = pubkyInput.value.trim();
      await this.initializeAndStart(pubkyValue);
    });

    // Set initial button state
    updateButtonState();
  }

  // Load main-form, beginning backup process
  private async initializeAndStart(pubkyValue: string): Promise<void> {
    const continueBtn = getElementByIdStrict<HTMLButtonElement>("continue-btn");
    continueBtn.disabled = true;
    continueBtn.classList.add("activated");

    try {
      await initAppState(pubkyValue);
      await backupControllerBegin();
      this.onSuccess();
      this.resetContinueButton();
    } catch (error: unknown) {
      console.error("Error:", error);
      handleBackendError(error);
      this.resetContinueButton();
      throw error;
    }
  }

  // Fetch list of keys previously used and provide as suggestions
  private async loadPreviousKeys(): Promise<void> {
    const pubkyInput = getElementByIdStrict<HTMLInputElement>("pubky-input");

    try {
      const previousKeys = await getPreviousPubkyKeys();
      const datalist =
        getElementByIdStrict<HTMLDataListElement>("previous-keys");

      datalist.innerHTML = "";
      if (previousKeys.length > 0) {
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
    } catch (error: unknown) {
      console.error("Error loading previous keys:", error);
      // If we can't load previous keys just use the default placeholder
      if (pubkyInput.value === "") {
        pubkyInput.placeholder = "g1b6wp8bhhxt...";
      }
    }
  }

  // If a last_pubky exists then automatically move on to main-from
  private async autoLoadLastPubky(): Promise<void> {
    try {
      const lastPubky = await getLastPubky();
      if (lastPubky !== null) {
        console.log("Auto-loading last used pubky:", lastPubky);
        const pubkyInput =
          getElementByIdStrict<HTMLInputElement>("pubky-input");
        pubkyInput.value = lastPubky;

        try {
          await this.initializeAndStart(lastPubky);
        } catch (error: unknown) {
          console.error("Internal Error:", error);
        }
      }
    } catch (error: unknown) {
      console.error("Error checking for last pubky:", error);
    }
  }
}
