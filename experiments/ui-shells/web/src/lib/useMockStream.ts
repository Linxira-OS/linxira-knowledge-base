import { useEffect, useState } from 'react'

interface StreamOptions {
  charsPerTick?: number
  intervalMs?: number
  startDelayMs?: number
}

/** Mock token stream: reveals `text` progressively so the summary card behaves
 *  like a streaming model response without any network dependency. */
export function useMockStream(text: string, options: StreamOptions = {}) {
  const { charsPerTick = 3, intervalMs = 22, startDelayMs = 140 } = options
  const [shown, setShown] = useState('')
  const [done, setDone] = useState(false)

  useEffect(() => {
    setShown('')
    setDone(false)
    let index = 0
    let interval: number | undefined
    const delay = window.setTimeout(() => {
      interval = window.setInterval(() => {
        index = Math.min(text.length, index + charsPerTick)
        setShown(text.slice(0, index))
        if (index >= text.length) {
          window.clearInterval(interval)
          setDone(true)
        }
      }, intervalMs)
    }, startDelayMs)
    return () => {
      window.clearTimeout(delay)
      if (interval) window.clearInterval(interval)
    }
  }, [text, charsPerTick, intervalMs, startDelayMs])

  return { shown, done }
}
