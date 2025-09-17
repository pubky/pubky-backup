import { invoke } from '@tauri-apps/api/core'

export class MainForm {
  constructor() {
    this.pubky = null
    this.homeserver = null
    this.developerMode = false
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

    document.getElementById('back-btn').addEventListener('click', async () => {
      try {
        await invoke('worker_thread_close')
      } catch (error) {
        console.error('Internal Error:', error)
      }

      document.querySelectorAll('.screen').forEach(screen => {
        screen.classList.add('hidden')
        })
      document.getElementById('startup-screen').classList.remove('hidden')
    })
  }

  async loadStateOnInit() {
    try {
      const stateMsg = await invoke('fetch_state')
      const data = JSON.parse(stateMsg)
      this.pubky = data.pubky
      this.homeserver = data.homeserver
      this.developerMode = data.developer_mode
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
      pubkyDisplay.textContent = this.displayPubky(this.pubky)
      homeserverDisplay.textContent = this.displayPubky(this.homeserver)
      backupHeader.classList.remove('hidden')
    } else {
      console.log(`Failed to find State data`)
    }
    
    const devIndicator = document.getElementById('dev-indicator')
    if (this.developerMode && devIndicator) {
      devIndicator.classList.remove('hidden')
      console.log('Developer mode is enabled')
    } else if (devIndicator) {
      devIndicator.classList.add('hidden')
    }
  }
}