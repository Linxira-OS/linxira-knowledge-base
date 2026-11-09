declare global {
  interface Window {
    __ELECTRON__?: boolean
    __TAURI_INTERNALS__?: unknown
    __TAURI__?: unknown
  }
}

export type ShellName = 'tauri' | 'electron' | 'pwa'

export function detectShell(): ShellName {
  if (window.__ELECTRON__) return 'electron'
  if (window.__TAURI_INTERNALS__ || window.__TAURI__) return 'tauri'
  return 'pwa'
}

const MARK_ENDPOINT = 'http://127.0.0.1:8791/mark'

/** Reports first contentful paint to the local benchmark collector.
 *  The collector is only running during measurement; a failed fetch is a no-op. */
export function reportFirstPaint(): void {
  const send = () => {
    try {
      const fcp = performance.getEntriesByName('first-contentful-paint')[0]?.startTime ?? performance.now()
      const wall = performance.timeOrigin + fcp
      const nav = performance.getEntriesByType('navigation')[0] as PerformanceNavigationTiming | undefined
      const transferred =
        (nav?.transferSize ?? 0) +
        performance.getEntriesByType('resource').reduce((sum, entry) => sum + ((entry as PerformanceResourceTiming).transferSize ?? 0), 0)
      const query = `shell=${detectShell()}&fcp=${fcp.toFixed(1)}&t=${wall.toFixed(1)}&bytes=${transferred}`
      void fetch(`${MARK_ENDPOINT}?${query}`, { mode: 'no-cors', keepalive: true }).catch(() => {})
    } catch {
      /* measurement only */
    }
  }
  requestAnimationFrame(() => requestAnimationFrame(() => window.setTimeout(send, 0)))
}
