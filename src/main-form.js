import { invoke } from '@tauri-apps/api/core'

export class MainForm {
  constructor() {
    this.name = ''
    this.greetMsg = ''
  }

  init() {
    this.bindEvents()
  }

  bindEvents() {
    // Greet form functionality
    const greetForm = document.getElementById('greet-form')
    const greetInput = document.getElementById('greet-input')

    greetInput.addEventListener('input', (e) => {
      this.name = e.target.value
    })

    greetForm.addEventListener('submit', async (e) => {
      e.preventDefault()
      if (this.name.trim() === '') {
        return
      }

      try {
        const newMsg = await invoke('greet', { name: this.name })
        this.greetMsg = newMsg
        this.updateGreetMsg()
      } catch (error) {
        console.error('Error calling greet:', error)
      }
    })

    // Fetch form functionality
    const fetchForm = document.getElementById('fetch-form')

    fetchForm.addEventListener('submit', async (e) => {
      e.preventDefault()
      try {
        const newMsg = await invoke('fetch_data')
        this.greetMsg = newMsg
        this.updateGreetMsg()
      } catch (error) {
        console.error('Error calling fetch_data:', error)
      }
    })

    // Fetch from state form functionality
    const fetchStateForm = document.getElementById('fetch-state-form')

    fetchStateForm.addEventListener('submit', async (e) => {
      e.preventDefault()
      try {
        const newMsg = await invoke('fetch_from_state')
        this.greetMsg = newMsg
        this.updateGreetMsg()
      } catch (error) {
        console.error('Error calling fetch_from_state:', error)
      }
    })
  }

  updateGreetMsg() {
    document.getElementById('greet-msg').textContent = this.greetMsg
  }
}