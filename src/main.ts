import { createApp } from 'vue'

import App from './App.vue'
import { app } from './lib/ipc'
import './styles/main.css'

/**
 * Sifora Launcher — renderer entry point.
 *
 * The renderer has no capabilities of its own: it cannot read the filesystem,
 * make network requests, spawn processes or reach the Tauri ACL. Everything
 * privileged is a Rust command reached through `@/lib/ipc`.
 */

/**
 * Forward renderer diagnostics into the Rust ring buffer.
 *
 * The release build ships without devtools, so an exception here would
 * otherwise be invisible to the user and to us. Errors and unhandled
 * rejections are always reported; `console.debug` is not, to keep the log
 * readable.
 */
function installLogBridge(): void {
  const send = (level: string, args: unknown[]): void => {
    const text = args
      .map((value) => {
        if (typeof value === 'string') return value
        if (value instanceof Error) return `${value.name}: ${value.message}`
        try {
          return JSON.stringify(value)
        } catch {
          return String(value)
        }
      })
      .join(' ')
    void app.rendererLog(level, text).catch(() => undefined)
  }

  for (const level of ['error', 'warn', 'info'] as const) {
    const original = console[level].bind(console)
    console[level] = (...args: unknown[]) => {
      original(...args)
      send(level, args)
    }
  }

  window.addEventListener('error', (event) => {
    send('error', [event.message, event.filename, String(event.lineno)])
  })
  window.addEventListener('unhandledrejection', (event) => {
    send('error', ['unhandledrejection', event.reason])
  })
}

installLogBridge()
app.tailLog(20).then((records) => {
  console.info(`[sifora] loaded with ${records.length} recent log entries`)
}).catch(() => undefined)

createApp(App).mount('#app')