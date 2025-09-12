import { invoke } from '@tauri-apps/api/core'

export class GreetForm {
  constructor(onSuccess) {
    this.onSuccess = onSuccess
  }

  init() {
    this.bindEvents()
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
}