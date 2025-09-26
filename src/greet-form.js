import { invoke } from '@tauri-apps/api/core'

export class GreetForm {
  constructor(onSuccess) {
    this.onSuccess = onSuccess
  }

  resetContinueButton() {
    const continueBtn = document.getElementById('continue-btn')
    const spinner = document.getElementById('continue-spinner')
    if (continueBtn) {
      continueBtn.disabled = false
    }
    if (spinner) {
      spinner.classList.add('hidden')
    }
  }

  async init() {
    this.bindEvents()
    await this.checkDevMode()
  }

  bindEvents() {
    const continueBtn = document.getElementById('continue-btn')
    const pubkyInput = document.getElementById('pubky-input')

    // Clear placeholder on focus or input
    pubkyInput.addEventListener('focus', () => {
      pubkyInput.placeholder = ''
    })

    pubkyInput.addEventListener('input', () => {
      pubkyInput.placeholder = ''
    })

    continueBtn.addEventListener('click', async (e) => {
      e.preventDefault()
      const pubkyValue = pubkyInput.value.trim()

      // Show loading state - show spinner
      continueBtn.disabled = true
      const spinner = document.getElementById('continue-spinner')
      if (spinner) {
        spinner.classList.remove('hidden')
      }

      try {
        await invoke('init_app_state', { pubkyStr: pubkyValue })

        // Start backup task when transitioning to main form
        await invoke('backup_controller_begin')

        this.onSuccess()

        // Remove spinner after transitioning
        this.resetContinueButton()
      } catch (error) {
        console.error('Internal Error:', error)
        alert(`Error: ${error}`)

        // Restore button state on error
        this.resetContinueButton()
      }
    })
  }

  async checkDevMode() {
    try {
      const data = await invoke('fetch_state')
      const devIndicator = document.getElementById('startup-dev-indicator')

      if (data.developer_mode && devIndicator) {
        devIndicator.classList.remove('hidden')
        console.log('Developer mode is enabled')
      }
    } catch (error) {
      console.error('Error checking dev mode:', error)
    }
  }
}