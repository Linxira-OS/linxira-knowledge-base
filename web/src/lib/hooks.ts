import { useCallback, useEffect, useRef, useState } from 'react'

/** Debounces a fast-changing value (search-as-you-type). */
export function useDebouncedValue<T>(value: T, delayMs = 350): T {
  const [debounced, setDebounced] = useState(value)
  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(value), delayMs)
    return () => window.clearTimeout(timer)
  }, [value, delayMs])
  return debounced
}

/** Keeps an element scrolled to the bottom while `pinned` is true. */
export function useStickToBottom<T extends HTMLElement>(dep: unknown) {
  const ref = useRef<T | null>(null)
  const pinned = useRef(true)
  useEffect(() => {
    const element = ref.current
    if (!element || !pinned.current) return
    element.scrollTop = element.scrollHeight
  }, [dep])
  const onScroll = useCallback(() => {
    const element = ref.current
    if (!element) return
    pinned.current = element.scrollHeight - element.scrollTop - element.clientHeight < 48
  }, [])
  return { ref, onScroll }
}
