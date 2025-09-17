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
        await invoke('init_state_for_pubky', { pubkyStr: pubkyValue })

        // Start background task when transitioning to main form
        await invoke('start_background_task')

        this.onSuccess()
      } catch (error) {
        console.error('Internal Error:', error)
        alert(`Error: ${error}`)
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