import { invoke } from '@tauri-apps/api/core'

export class GreetForm {
  constructor(onSuccess) {
    this.onSuccess = onSuccess
  }

  async init() {
    this.bindEvents()
    await this.checkDevMode()
  }

  bindEvents() {
    const continueBtn = document.getElementById('continue-btn')
    const pubkyInput = document.getElementById('pubky-input')

    continueBtn.addEventListener('click', async (e) => {
      e.preventDefault()
      const pubkyValue = pubkyInput.value.trim()

      try {
        const result = await invoke('store_pubky', { pubkyStr: pubkyValue })
        console.log(result)
        this.onSuccess()
      } catch (error) {
        console.error('Error storing pubky:', error)
        alert(`Error: ${error}`)
      }
    })
  }

  async checkDevMode() {
    try {
      const isDevMode = await invoke('is_dev_mode')
      const devIndicator = document.getElementById('startup-dev-indicator')

      if (isDevMode && devIndicator) {
        devIndicator.classList.remove('hidden')
        console.log('Developer mode is enabled')
      }
    } catch (error) {
      console.error('Error checking dev mode:', error)
    }
  }
}