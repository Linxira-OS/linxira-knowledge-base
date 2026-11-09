import type { ButtonHTMLAttributes, HTMLAttributes, ReactNode } from 'react'
import { ApiError } from '../api/client'
import { useApiErrorText } from '../lib/errors'
import { useI18n } from '../lib/i18n'

export function cx(...values: (string | false | null | undefined)[]): string {
  return values.filter(Boolean).join(' ')
}

/* -------------------------------------------------------------------- button */

type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger'

const BUTTON_VARIANTS: Record<ButtonVariant, string> = {
  primary: 'border border-accent-line bg-accent text-on-accent hover:opacity-90',
  secondary: 'border border-line bg-surface text-fg hover:border-line-strong hover:bg-panel',
  ghost: 'border border-transparent bg-transparent text-fg-2 hover:bg-panel hover:text-fg',
  danger: 'border border-line bg-surface text-danger hover:border-danger',
}

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant
  size?: 'sm' | 'md'
}

export function Button({ variant = 'secondary', size = 'md', className, type = 'button', ...rest }: ButtonProps) {
  return (
    <button
      type={type}
      className={cx(
        'inline-flex items-center justify-center gap-1.5 rounded-lg font-medium transition-colors disabled:cursor-not-allowed',
        size === 'sm' ? 'px-2.5 py-1 text-[11.5px]' : 'px-3 py-[6px] text-[12.5px]',
        BUTTON_VARIANTS[variant],
        className,
      )}
      {...rest}
    />
  )
}

/* ---------------------------------------------------------------------- card */

export function Card({
  title,
  description,
  actions,
  children,
  className,
  bodyClassName,
}: {
  title?: ReactNode
  description?: ReactNode
  actions?: ReactNode
  children?: ReactNode
  className?: string
  bodyClassName?: string
}) {
  return (
    <section className={cx('rounded-xl border border-line bg-surface', className)}>
      {(title || actions) && (
        <header className="flex items-start justify-between gap-3 border-b border-line px-4 py-3">
          <div className="min-w-0">
            {title && <h2 className="text-[13.5px] font-semibold text-fg">{title}</h2>}
            {description && <p className="mt-0.5 text-[11.5px] text-fg-3">{description}</p>}
          </div>
          {actions && <div className="flex shrink-0 items-center gap-1.5">{actions}</div>}
        </header>
      )}
      <div className={cx('px-4 py-3', bodyClassName)}>{children}</div>
    </section>
  )
}

/* --------------------------------------------------------------------- badge */

export type BadgeTone = 'neutral' | 'accent' | 'warn' | 'danger'

const BADGE_TONES: Record<BadgeTone, string> = {
  neutral: 'border-line bg-panel text-fg-3',
  accent: 'border-accent-line bg-accent-weak text-accent',
  warn: 'border-line bg-warn-weak text-warn',
  danger: 'border-line bg-danger-weak text-danger',
}

export function Badge({ tone = 'neutral', children, className }: { tone?: BadgeTone; children: ReactNode; className?: string }) {
  return (
    <span
      className={cx(
        'inline-flex items-center gap-1 rounded-md border px-1.5 py-[2px] text-[10.5px] leading-none',
        BADGE_TONES[tone],
        className,
      )}
    >
      {children}
    </span>
  )
}

export function StatusDot({ tone = 'neutral' }: { tone?: BadgeTone }) {
  const color =
    tone === 'accent' ? 'bg-accent' : tone === 'warn' ? 'bg-warn' : tone === 'danger' ? 'bg-danger' : 'bg-fg-3'
  return <span className={cx('inline-block h-1.5 w-1.5 shrink-0 rounded-full', color)} />
}

/** Maps a backend status literal onto a badge tone (never colour-only). */
export function statusTone(status: string): BadgeTone {
  switch (status) {
    case 'active':
    case 'ready':
    case 'healthy':
    case 'closed':
    case 'ok':
    case 'paid':
    case 'completed':
      return 'accent'
    case 'pending':
    case 'queued':
    case 'running':
    case 'archived':
    case 'soft':
    case 'half_open':
    case 'partial':
      return 'warn'
    case 'failed':
    case 'deleted':
    case 'error':
    case 'open':
      return 'danger'
    default:
      return 'neutral'
  }
}

/* --------------------------------------------------------------------- field */

export function Field({
  label,
  hint,
  error,
  htmlFor,
  children,
}: {
  label: ReactNode
  hint?: ReactNode
  error?: ReactNode
  htmlFor?: string
  children: ReactNode
}) {
  return (
    <label className="block" htmlFor={htmlFor}>
      <span className="mb-1 block text-[11.5px] font-medium text-fg-2">{label}</span>
      {children}
      {hint && <span className="mt-1 block text-[10.5px] leading-4 text-fg-3">{hint}</span>}
      {error && <span className="mt-1 block text-[10.5px] leading-4 text-danger">{error}</span>}
    </label>
  )
}

/* -------------------------------------------------------------------- states */

export function LoadingState({ label }: { label?: ReactNode }) {
  const { t } = useI18n()
  return (
    <div className="flex items-center gap-2 px-1 py-6 text-[12.5px] text-fg-3">
      <span className="h-3 w-3 animate-spin rounded-full border border-line-strong border-t-accent" />
      {label ?? t('state.loading')}
    </div>
  )
}

export function EmptyState({ title, description, actions }: { title?: ReactNode; description?: ReactNode; actions?: ReactNode }) {
  const { t } = useI18n()
  return (
    <div className="rounded-xl border border-dashed border-line px-4 py-8 text-center">
      <p className="text-[13px] font-medium text-fg-2">{title ?? t('state.empty')}</p>
      {description && <p className="mx-auto mt-1 max-w-md text-[11.5px] leading-5 text-fg-3">{description}</p>}
      {actions && <div className="mt-3 flex items-center justify-center gap-1.5">{actions}</div>}
    </div>
  )
}

/**
 * Backend capability that does not exist (or is gated off). Used instead of
 * fabricating data, per ADR 0004 (explicit degradation).
 */
export function NotProvided({ title, body, code }: { title?: ReactNode; body?: ReactNode; code?: string }) {
  const { t } = useI18n()
  return (
    <div className="rounded-xl border border-line bg-warn-weak px-4 py-3">
      <div className="flex items-center gap-2">
        <Badge tone="warn">{t('notProvided.title')}</Badge>
        {code && <code className="text-[10.5px] text-fg-3">{code}</code>}
      </div>
      <p className="mt-1.5 text-[12px] font-medium text-fg">{title ?? t('notProvided.title')}</p>
      <p className="mt-0.5 text-[11.5px] leading-5 text-fg-2">{body ?? t('notProvided.body')}</p>
    </div>
  )
}

export function ErrorState({ error, onRetry, compact }: { error: unknown; onRetry?: () => void; compact?: boolean }) {
  const text = useApiErrorText()
  const { t } = useI18n()
  const apiError = error instanceof ApiError ? error : null
  return (
    <div className={cx('rounded-xl border border-line bg-danger-weak', compact ? 'px-3 py-2' : 'px-4 py-3')}>
      <div className="flex items-center gap-2">
        <Badge tone="danger">{apiError ? apiError.code : t('state.error')}</Badge>
        {apiError && apiError.status > 0 && <span className="text-[10.5px] text-fg-3">HTTP {apiError.status}</span>}
      </div>
      <p className="mt-1.5 text-[12.5px] leading-5 text-fg">{text(error)}</p>
      {apiError?.retryAfterSeconds !== null && apiError?.retryAfterSeconds !== undefined && (
        <p className="mt-0.5 text-[11px] text-fg-3">{t('label.retryAfter', { seconds: apiError.retryAfterSeconds })}</p>
      )}
      {apiError?.requestId && (
        <p className="mt-0.5 font-mono text-[10.5px] text-fg-3">
          {t('label.requestId')}: {apiError.requestId}
        </p>
      )}
      {onRetry && (
        <div className="mt-2">
          <Button size="sm" onClick={onRetry}>
            {t('action.retry')}
          </Button>
        </div>
      )}
    </div>
  )
}

export function Banner({
  tone = 'neutral',
  title,
  children,
  actions,
}: {
  tone?: BadgeTone
  title?: ReactNode
  children?: ReactNode
  actions?: ReactNode
}) {
  const tones: Record<BadgeTone, string> = {
    neutral: 'border-line bg-panel',
    accent: 'border-accent-line bg-accent-weak',
    warn: 'border-line bg-warn-weak',
    danger: 'border-line bg-danger-weak',
  }
  return (
    <div className={cx('rounded-xl border px-3.5 py-2.5', tones[tone])}>
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          {title && <p className="text-[12.5px] font-medium text-fg">{title}</p>}
          {children && <div className={cx('text-[11.5px] leading-5 text-fg-2', title ? 'mt-0.5' : undefined)}>{children}</div>}
        </div>
        {actions && <div className="flex shrink-0 items-center gap-1.5">{actions}</div>}
      </div>
    </div>
  )
}

/* --------------------------------------------------------------------- table */

export function TableWrap({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div className={cx('thin-scroll overflow-x-auto rounded-xl border border-line bg-surface', className)}>
      {children}
    </div>
  )
}

export function Table({ children, className }: { children: ReactNode; className?: string }) {
  return <table className={cx('w-full border-collapse text-[12.5px]', className)}>{children}</table>
}

export function THead({ children }: { children: ReactNode }) {
  return <thead className="bg-panel text-[11px] uppercase tracking-[0.08em] text-fg-3">{children}</thead>
}

export function TBody({ children }: { children: ReactNode }) {
  return <tbody className="divide-y divide-line">{children}</tbody>
}

export function TR({
  children,
  className,
  ...rest
}: { children: ReactNode; className?: string } & HTMLAttributes<HTMLTableRowElement>) {
  return (
    <tr className={cx('align-top', className)} {...rest}>
      {children}
    </tr>
  )
}

export function TH({ children, className, title }: { children: ReactNode; className?: string; title?: string }) {
  return (
    <th title={title} className={cx('px-3 py-2 text-left font-medium', className)}>
      {children}
    </th>
  )
}

export function TD({
  children,
  className,
  colSpan,
  title,
}: {
  children: ReactNode
  className?: string
  colSpan?: number
  title?: string
}) {
  return (
    <td colSpan={colSpan} title={title} className={cx('px-3 py-2 text-fg-2', className)}>
      {children}
    </td>
  )
}

/* ------------------------------------------------------------------ key/value */

export function KeyValue({ items, className }: { items: [ReactNode, ReactNode][]; className?: string }) {
  return (
    <dl className={cx('grid gap-x-6 gap-y-2 sm:grid-cols-2', className)}>
      {items.map(([label, value], index) => (
        <div key={index} className="min-w-0">
          <dt className="text-[10.5px] uppercase tracking-[0.1em] text-fg-3">{label}</dt>
          <dd className="mt-0.5 break-words text-[12.5px] text-fg">{value}</dd>
        </div>
      ))}
    </dl>
  )
}

/* --------------------------------------------------------------------- json */

export function JsonBlock({ value, label }: { value: unknown; label?: string }) {
  const { t, format } = useI18n()
  const text = typeof value === 'string' ? value : JSON.stringify(value, null, 2)
  return (
    <div className="min-w-0">
      {label && <div className="mb-1 text-[10.5px] uppercase tracking-[0.1em] text-fg-3">{label}</div>}
      <pre className="thin-scroll max-h-64 overflow-auto rounded-lg border border-line bg-code-bg px-3 py-2 font-mono text-[11px] leading-5 text-code-fg">
        {text || t('label.none')}
      </pre>
      <div className="mt-0.5 text-[10px] text-fg-3">{format.number(text.length)} chars</div>
    </div>
  )
}

/* --------------------------------------------------------------------- tabs */

export interface TabItem<T extends string> {
  id: T
  label: ReactNode
  badge?: ReactNode
}

export function Tabs<T extends string>({
  items,
  active,
  onChange,
}: {
  items: TabItem<T>[]
  active: T
  onChange: (id: T) => void
}) {
  return (
    <div className="thin-scroll flex items-center gap-1 overflow-x-auto overflow-y-hidden border-b border-line pb-px">
      {items.map((item) => (
        <button
          key={item.id}
          type="button"
          onClick={() => onChange(item.id)}
          aria-current={item.id === active ? 'page' : undefined}
          className={cx(
            '-mb-px flex items-center gap-1.5 whitespace-nowrap border-b-2 px-3 py-2 text-[12.5px] transition-colors',
            item.id === active
              ? 'border-accent font-medium text-fg'
              : 'border-transparent text-fg-3 hover:text-fg',
          )}
        >
          {item.label}
          {item.badge}
        </button>
      ))}
    </div>
  )
}

/* ----------------------------------------------------------------- pagination */

export function Pager({
  hasMore,
  loading,
  onNext,
  loaded,
}: {
  hasMore: boolean
  loading?: boolean
  onNext: () => void
  loaded?: number
}) {
  const { t } = useI18n()
  if (!hasMore && loaded === undefined) return null
  return (
    <div className="flex items-center justify-between gap-3 px-1 py-2">
      <span className="text-[11px] text-fg-3">{loaded !== undefined ? t('paging.loaded', { count: loaded }) : ''}</span>
      {hasMore && (
        <Button size="sm" onClick={onNext} disabled={loading}>
          {t('paging.more')}
        </Button>
      )}
    </div>
  )
}

/* ---------------------------------------------------------------- degradation */

export function DegradedList({ items }: { items: { code: string; impact: string; fallback: string }[] }) {
  const { t } = useI18n()
  if (items.length === 0) return null
  return (
    <div className="rounded-xl border border-line bg-warn-weak px-3 py-2">
      <div className="text-[10.5px] font-medium uppercase tracking-[0.1em] text-warn">{t('degraded.title')}</div>
      <ul className="mt-1 space-y-1">
        {items.map((item) => (
          <li key={item.code} className="text-[11.5px] leading-5 text-fg-2">
            <code className="text-fg-3">{item.code}</code>
            {item.impact && <> · {t('degraded.impact')}: {item.impact}</>}
            {item.fallback && <> · {t('degraded.fallback')}: {item.fallback}</>}
          </li>
        ))}
      </ul>
    </div>
  )
}

/* ------------------------------------------------------------------- section */

export function SectionTitle({ children, actions }: { children: ReactNode; actions?: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-3">
      <h1 className="text-[22px] font-semibold tracking-[-0.02em] text-fg">{children}</h1>
      {actions && <div className="flex items-center gap-1.5">{actions}</div>}
    </div>
  )
}
