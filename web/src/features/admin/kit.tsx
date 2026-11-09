import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { Button } from '../../components/ui'
import { useI18n } from '../../lib/i18n'

/* ------------------------------------------------------------------ skeleton */

/** Placeholder rows shown by AdminQueryGate while a panel loads. */
export function AdminSkeleton({ rows = 4 }: { rows?: number }) {
  return (
    <div className="space-y-2" aria-hidden="true">
      {Array.from({ length: rows }, (_, index) => (
        <div key={index} className="lkb-skeleton h-7" style={{ opacity: 1 - index * 0.12 }} />
      ))}
    </div>
  )
}

/* -------------------------------------------------------------------- toasts */

type ToastTone = 'accent' | 'danger' | 'warn'

interface ToastMessage {
  id: number
  tone: ToastTone
  text: string
}

let toastId = 0
let messages: ToastMessage[] = []
const listeners = new Set<() => void>()

function emitToasts() {
  for (const listener of listeners) listener()
}

function dismissToast(id: number) {
  messages = messages.filter((message) => message.id !== id)
  emitToasts()
}

export function pushAdminToast(text: string, tone: ToastTone = 'accent'): void {
  const message: ToastMessage = { id: ++toastId, tone, text }
  messages = [...messages.slice(-3), message]
  emitToasts()
  window.setTimeout(() => dismissToast(message.id), 4200)
}

/** Success/error feedback for admin mutations (mounted once in AdminPage). */
export function AdminToastHost() {
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
          className={
            item.tone === 'danger'
              ? 'rounded-lg border border-danger/40 bg-danger-weak px-3 py-2 text-[12px] text-danger shadow-lg'
              : item.tone === 'warn'
                ? 'rounded-lg border border-warn/40 bg-warn-weak px-3 py-2 text-[12px] text-warn shadow-lg'
                : 'rounded-lg border border-accent-line bg-accent-weak px-3 py-2 text-[12px] text-accent shadow-lg'
          }
        >
          {item.text}
        </div>
      ))}
    </div>
  )
}

export function useAdminToast() {
  return useMemo(
    () => ({
      success: (text: string) => pushAdminToast(text, 'accent'),
      warn: (text: string) => pushAdminToast(text, 'warn'),
      error: (text: string) => pushAdminToast(text, 'danger'),
    }),
    [],
  )
}

/* -------------------------------------------------------------- confirm dialog */

/** Modal confirmation for destructive admin actions (delete/disable/transfer). */
export function ConfirmDialog({
  open,
  title,
  description,
  confirmLabel,
  pending = false,
  onConfirm,
  onCancel,
}: {
  open: boolean
  title: ReactNode
  description?: ReactNode
  confirmLabel: ReactNode
  pending?: boolean
  onConfirm: () => void
  onCancel: () => void
}) {
  const { t } = useI18n()
  if (!open) return null
  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onCancel()
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={typeof title === 'string' ? title : undefined}
        className="w-full max-w-[400px] rounded-xl border border-line bg-surface p-4 shadow-xl"
      >
        <h2 className="text-[13.5px] font-semibold text-fg">{title}</h2>
        {description && <p className="mt-1 text-[12px] leading-5 text-fg-2">{description}</p>}
        <div className="mt-4 flex justify-end gap-2">
          <Button size="sm" variant="ghost" disabled={pending} onClick={onCancel}>
            {t('action.cancel')}
          </Button>
          <Button size="sm" variant="danger" disabled={pending} onClick={onConfirm}>
            {confirmLabel}
          </Button>
        </div>
      </div>
    </div>
  )
}

/* ---------------------------------------------------------------- side drawer */

/** Right-hand detail drawer (audit events, row details). */
export function DetailDrawer({
  open,
  title,
  onClose,
  children,
}: {
  open: boolean
  title: ReactNode
  onClose: () => void
  children: ReactNode
}) {
  const { t } = useI18n()
  if (!open) return null
  return (
    <div
      className="fixed inset-0 z-50 flex justify-end bg-black/30"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose()
      }}
    >
      <aside
        role="dialog"
        aria-modal="true"
        aria-label={typeof title === 'string' ? title : undefined}
        className="thin-scroll flex h-full w-full max-w-[460px] flex-col overflow-y-auto border-l border-line bg-surface p-4 shadow-xl"
      >
        <header className="mb-3 flex items-start justify-between gap-3">
          <h2 className="min-w-0 text-[13.5px] font-semibold text-fg">{title}</h2>
          <Button size="sm" variant="ghost" onClick={onClose}>
            {t('action.close')}
          </Button>
        </header>
        <div className="min-w-0">{children}</div>
      </aside>
    </div>
  )
}

/* ------------------------------------------------------------ form dialog */

/** Form modal with cancel/submit footer; closes on backdrop click. */
export function FormDialog({
  open,
  title,
  pending = false,
  submitLabel,
  submitDisabled = false,
  onSubmit,
  onClose,
  children,
}: {
  open: boolean
  title: ReactNode
  pending?: boolean
  submitLabel: ReactNode
  submitDisabled?: boolean
  onSubmit: () => void
  onClose: () => void
  children: ReactNode
}) {
  const { t } = useI18n()
  if (!open) return null
  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose()
      }}
    >
      <form
        role="dialog"
        aria-modal="true"
        aria-label={typeof title === 'string' ? title : undefined}
        className="thin-scroll max-h-[90vh] w-full max-w-[440px] overflow-y-auto rounded-xl border border-line bg-surface p-4 shadow-xl"
        onSubmit={(event) => {
          event.preventDefault()
          if (!pending && !submitDisabled) onSubmit()
        }}
      >
        <h2 className="text-[13.5px] font-semibold text-fg">{title}</h2>
        <div className="mt-3 space-y-3">{children}</div>
        <div className="mt-4 flex justify-end gap-2">
          <Button size="sm" variant="ghost" type="button" disabled={pending} onClick={onClose}>
            {t('action.cancel')}
          </Button>
          <Button size="sm" variant="primary" type="submit" disabled={pending || submitDisabled}>
            {submitLabel}
          </Button>
        </div>
      </form>
    </div>
  )
}
