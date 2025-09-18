import { invoke } from '@tauri-apps/api/core'

export class MainForm {
  constructor() {
    this.pubky = null
    this.homeserver = null
    this.developerMode = false
    this.isSyncing = false
    this.nextSyncTime = 0
    this.statusInterval = null
    this.countdownInterval = null
  }

  init() {
    this.bindEvents()
    this.loadStateOnInit()
    this.startStatusPolling()
  }

  bindEvents() {
    // Copy pubky and homeserver buttons
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

    // Back button
    document.getElementById('back-btn').addEventListener('click', async () => {
      try {
        await invoke('worker_thread_close')
        this.stopStatusPolling()
        this.stopCountdown()
      } catch (error) {
        console.error('Internal Error:', error)
      }
      document.querySelectorAll('.screen').forEach(screen => {
        screen.classList.add('hidden')
        })
      document.getElementById('startup-screen').classList.remove('hidden')
    })

    // Force sync button
    document.getElementById('force-sync-btn').addEventListener('click', async () => {
      try {
        await invoke('force_sync_now')
        console.log('Force sync triggered')
      } catch (error) {
        console.error('Force sync failed:', error)
      }
    })
  }

  async loadStateOnInit() {
    try {
      const stateMsg = await invoke('fetch_state')
      const data = JSON.parse(stateMsg)
      this.pubky = data.pubky
      this.homeserver = data.homeserver
      this.developerMode = data.developer_mode
      this.isSyncing = data.is_syncing
      this.nextSyncTime = data.next_sync_time
      this.setHeader()
      this.updateSyncStatus()
      this.startCountdown()
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

  startStatusPolling() {
    // Poll for status updates every second
    this.statusInterval = setInterval(async () => {
      await this.fetchAndUpdateStatus()
    }, 1000)
  }

  stopStatusPolling() {
    if (this.statusInterval) {
      clearInterval(this.statusInterval)
      this.statusInterval = null
    }
  }

  async fetchAndUpdateStatus() {
    try {
      const stateMsg = await invoke('fetch_state')
      const data = JSON.parse(stateMsg)
      const newIsSyncing = data.is_syncing
      const newNextSyncTime = data.next_sync_time

      // Status
      if (newIsSyncing !== this.isSyncing) {
        this.isSyncing = newIsSyncing
        this.updateSyncStatus()
      }

      // Next sync time
      if (newNextSyncTime !== this.nextSyncTime) {
        this.nextSyncTime = newNextSyncTime
      }
    } catch (error) {
      console.error('Error fetching status:', error)
    }
  }

  updateSyncStatus() {
    const statusText = document.getElementById('status-text')
    const statusSpinner = document.getElementById('status-spinner')
    const statusTick = document.getElementById('status-tick')
    const syncStatus = document.getElementById('sync-status')

    if (this.isSyncing) {
      statusText.textContent = 'Syncing...'
      statusSpinner.classList.remove('hidden')
      statusTick.classList.add('hidden')
      syncStatus.classList.add('syncing')
      syncStatus.classList.remove('synced')
    } else {
      statusText.textContent = 'Synced'
      statusSpinner.classList.add('hidden')
      statusTick.classList.remove('hidden')
      syncStatus.classList.add('synced')
      syncStatus.classList.remove('syncing')
    }
  }

  startCountdown() {
    // Update countdown every second
    this.countdownInterval = setInterval(() => {
      this.updateCountdown()
    }, 1000)
  }

  stopCountdown() {
    if (this.countdownInterval) {
      clearInterval(this.countdownInterval)
      this.countdownInterval = null
    }
  }

  updateCountdown() {
    const countdownElement = document.getElementById('countdown-timer')
    const now = Math.floor(Date.now() / 1000)

    if (this.nextSyncTime > now) {
      const remaining = this.nextSyncTime - now
      const minutes = Math.floor(remaining / 60)
      const seconds = remaining % 60

      if (minutes > 0) {
        countdownElement.textContent = `${minutes}m ${seconds}s`
      } else {
        countdownElement.textContent = `${seconds}s`
      }
    } else {
      countdownElement.textContent = '0s'
    }
  }
}