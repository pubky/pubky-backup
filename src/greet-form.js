import { invoke } from '@tauri-apps/api/core'

export class GreetForm {
  constructor(onSuccess) {
    this.onSuccess = onSuccess
  }

  resetContinueButton() {
    const continueBtn = document.getElementById('continue-btn')
    if (continueBtn) {
      continueBtn.disabled = false
      // Remove spinner if it exists
      const spinner = continueBtn.querySelector('.spinner')
      if (spinner) {
        spinner.remove()
      }
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

      // Show loading state - add spinner to button
      continueBtn.disabled = true
      const spinner = document.createElement('span')
      spinner.className = 'spinner'
      spinner.textContent = '◐'
      continueBtn.append(' ')
      continueBtn.append(spinner)

      try {
        await invoke('init_app_state', { pubkyStr: pubkyValue })

        // Start background task when transitioning to main form
        await invoke('backup_controller_begin')

        // Remove spinner before transitioning
        this.resetContinueButton()

        this.onSuccess()
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
      const stateMsg = await invoke('fetch_state')
      const data = JSON.parse(stateMsg)
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