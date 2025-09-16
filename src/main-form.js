import { invoke } from '@tauri-apps/api/core'

export class MainForm {
  constructor() {
    this.pubky = null
    this.homeserver = null
  }

  init() {
    this.bindEvents()
    this.loadStateOnInit()
  }

  bindEvents() {
    // Copy button functionality
    document.getElementById('copy-pubky').addEventListener('click', () => {
      navigator.clipboard.writeText(this.pubky).then(() => {
        console.log('Pubky copied to clipboard')
      }).catch(err => {
        console.error('Failed to copy pubky:', err)
      })
    })
    document.getElementById('copy-homeserver').addEventListener('click', () => {
      navigator.clipboard.writeText(this.homeserver).then(() => {
        console.log('Homeserver copied to clipboard')
      }).catch(err => {
        console.error('Failed to copy homeserver:', err)
      })
    })
  }

  async loadStateOnInit() {
    try {
      const stateMsg = await invoke('fetch_state')
      const data = JSON.parse(stateMsg)
      this.pubky = data.pubky
      this.homeserver = data.homeserver
      this.setHeader()
    } catch (error) {
      console.error('Error loading initial state:', error)
      document.getElementById('backup-header').classList.add('hidden')
    }
  }

  displayPubky(str, length = 8) {
    return str.length > length ? str.substring(0, length) + '...' : str
  }

  setHeader() {
    const backupHeader = document.getElementById('backup-header')
    const pubkyDisplay = document.getElementById('pubky-display')
    const homeserverDisplay = document.getElementById('homeserver-display')

    if (this.pubky && this.homeserver) {
      // Display truncated values
      pubkyDisplay.textContent = this.displayPubky(this.pubky)
      homeserverDisplay.textContent = this.displayPubky(this.homeserver)
      backupHeader.classList.remove('hidden')
    } else {
      backupHeader.classList.add('hidden')
    }
  }
}