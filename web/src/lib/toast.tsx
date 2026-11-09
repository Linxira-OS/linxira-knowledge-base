import { useEffect, useState } from 'react'

export type ToastTone = 'accent' | 'warn' | 'danger'

interface ToastMessage {
  id: number
  tone: ToastTone
  text: string
}

let toastId = 0
let messages: ToastMessage[] = []
const listeners = new Set<() => void>()

function emit(): void {
  for (const listener of listeners) listener()
}

function dismiss(id: number): void {
  messages = messages.filter((message) => message.id !== id)
  emit()
}

/** Shows a transient app-wide message; survives route changes. */
export function pushToast(text: string, tone: ToastTone = 'accent'): void {
  const message: ToastMessage = { id: ++toastId, tone, text }
  messages = [...messages.slice(-3), message]
  emit()
  window.setTimeout(() => dismiss(message.id), 4200)
}

const TONE_CLASSES: Record<ToastTone, string> = {
  accent: 'border-accent-line bg-accent-weak text-accent',
  warn: 'border-warn/40 bg-warn-weak text-warn',
  danger: 'border-danger/40 bg-danger-weak text-danger',
}

/** Mounted once by the app shell (outside the admin console as well). */
export function ToastHost() {
  const [items, setItems] = useState<ToastMessage[]>(messages)

  useEffect(() => {
    const update = () => setItems(messages)
    listeners.add(update)
    update()
    return () => {
      listeners.delete(update)
    }
  }, [])

  if (items.length === 0) return null
  return (
    <div className="pointer-events-none fixed bottom-4 right-4 z-[60] flex w-[300px] flex-col gap-2">
      {items.map((item) => (
        <div
          key={item.id}
          role="status"
          className={`rounded-lg border px-3 py-2 text-[12px] shadow-lg ${TONE_CLASSES[item.tone]}`}
        >
          {item.text}
        </div>
      ))}
    </div>
  )
}
