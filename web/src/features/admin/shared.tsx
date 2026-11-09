import { useCallback, useState, type ReactNode } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { ApiError } from '../../api/client'
import { Banner, Button, ErrorState, Field } from '../../components/ui'
import { useI18n } from '../../lib/i18n'
import { AdminSkeleton } from './kit'

/** Page size for the offset-paginated admin tables. */
export const ADMIN_PAGE_SIZE = 25

/** Query keys owned by the admin console; also the invalidation targets. */
export const adminKeys = {
  users: ['admin', 'users'] as const,
  roles: ['admin', 'roles'] as const,
  groups: ['admin', 'groups'] as const,
  spaces: ['admin', 'spaces'] as const,
  providers: ['admin', 'providers'] as const,
  providerModels: ['admin', 'provider-models'] as const,
  aliases: ['admin', 'aliases'] as const,
  routes: ['admin', 'routes'] as const,
  quotas: ['admin', 'quotas'] as const,
  usage: ['admin', 'usage'] as const,
  audit: ['admin', 'audit'] as const,
  overview: ['admin', 'overview'] as const,
  billing: ['admin', 'billing'] as const,
  connectors: ['admin', 'connectors'] as const,
}

/** Invalidates exactly the admin query keys given (prefix match). */
export function useAdminInvalidate(): (...keys: ReadonlyArray<readonly unknown[]>) => void {
  const queryClient = useQueryClient()
  return useCallback(
    (...keys: ReadonlyArray<readonly unknown[]>) => {
      for (const key of keys) void queryClient.invalidateQueries({ queryKey: key })
    },
    [queryClient],
  )
}

/**
 * Renders loading / error states for an admin query. A 403 becomes the
 * `admin.forbidden` banner (never fake data); `feature_disabled` can map to a
 * capability-specific banner via `featureDisabledKey`.
 */
export function AdminQueryGate({
  pending,
  error,
  onRetry,
  featureDisabledKey,
  children,
}: {
  pending: boolean
  error: unknown
  onRetry: () => void
  featureDisabledKey?: 'admin.audit.disabled' | 'admin.billing.disabled' | 'admin.connectors.disabled'
  children: ReactNode
}) {
  const { t } = useI18n()
  if (pending) return <AdminSkeleton />
  if (error) {
    if (error instanceof ApiError && error.status === 403) {
      const disabled = error.code === 'feature_disabled'
      return (
        <Banner
          tone="warn"
          title={disabled && featureDisabledKey ? t(featureDisabledKey) : t('admin.forbidden')}
        />
      )
    }
    return <ErrorState error={error} onRetry={onRetry} />
  }
  return <>{children}</>
}

/** Prev/next offset pager with a caller-supplied summary label. */
export function OffsetPager({
  offset,
  limit,
  total,
  onChange,
  loading,
  label,
}: {
  offset: number
  limit: number
  total: number
  onChange: (offset: number) => void
  loading?: boolean
  label?: ReactNode
}) {
  const { t } = useI18n()
  const hasPrev = offset > 0
  const hasNext = offset + limit < total
  if (!hasPrev && !hasNext && !label) return null
  return (
    <div className="flex items-center justify-between gap-3 px-1 py-2">
      <span className="text-[11px] text-fg-3">{label}</span>
      <div className="flex items-center gap-1.5">
        <Button size="sm" disabled={!hasPrev || loading} onClick={() => onChange(Math.max(0, offset - limit))}>
          {t('paging.prev')}
        </Button>
        <Button size="sm" disabled={!hasNext || loading} onClick={() => onChange(offset + limit)}>
          {t('paging.next')}
        </Button>
      </div>
    </div>
  )
}

export function TextInput({
  label,
  value,
  onChange,
  type = 'text',
  hint,
  error,
}: {
  label: ReactNode
  value: string
  onChange: (value: string) => void
  type?: 'text' | 'number' | 'password'
  hint?: ReactNode
  error?: ReactNode
}) {
  return (
    <Field label={label} hint={hint} error={error}>
      <input type={type} value={value} onChange={(event) => onChange(event.target.value)} />
    </Field>
  )
}

export function SelectInput({
  label,
  value,
  onChange,
  options,
  hint,
}: {
  label: ReactNode
  value: string
  onChange: (value: string) => void
  options: ReadonlyArray<{ value: string; label: ReactNode }>
  hint?: ReactNode
}) {
  return (
    <Field label={label} hint={hint}>
      <select value={value} onChange={(event) => onChange(event.target.value)}>
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </Field>
  )
}

/** Plain-text JSON field (`*_json` wire strings), validated on submit. */
export function JsonInput({
  label,
  value,
  onChange,
  hint,
  error,
}: {
  label: ReactNode
  value: string
  onChange: (value: string) => void
  hint?: ReactNode
  error?: ReactNode
}) {
  return (
    <Field label={label} hint={hint} error={error}>
      <textarea
        rows={3}
        spellCheck={false}
        className="font-mono text-[11.5px]"
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
    </Field>
  )
}

/**
 * Draft state for a form's JSON string fields: values are raw text, `validate`
 * marks the ones that are not parseable so callers can render an inline
 * `error.bad_request`-style message.
 */
export function useJsonDraft(fields: readonly string[], initial?: Record<string, string>) {
  const { t } = useI18n()
  const [values, setValues] = useState<Record<string, string>>(() => {
    const record: Record<string, string> = {}
    for (const field of fields) record[field] = initial?.[field] ?? ''
    return record
  })
  const [invalid, setInvalid] = useState<readonly string[]>([])

  const set = (field: string, value: string) => {
    setValues((prev) => ({ ...prev, [field]: value }))
    setInvalid((prev) => (prev.includes(field) ? prev.filter((item) => item !== field) : prev))
  }

  const reset = () => {
    setValues((prev) => {
      const next: Record<string, string> = {}
      for (const field of Object.keys(prev)) next[field] = ''
      return next
    })
    setInvalid([])
  }

  const validate = (): boolean => {
    const bad = Object.keys(values).filter((field) => {
      const trimmed = values[field].trim()
      if (trimmed === '') return false
      try {
        JSON.parse(trimmed)
        return false
      } catch {
        return true
      }
    })
    setInvalid(bad)
    return bad.length === 0
  }

  const error = (field: string): string | undefined =>
    invalid.includes(field) ? t('error.bad_request') : undefined

  return { values, set, reset, validate, error }
}

/** `''` → undefined (omit), non-numeric → null (invalid), else the number. */
export function parseOptionalNumber(text: string): number | undefined | null {
  const trimmed = text.trim()
  if (trimmed === '') return undefined
  const value = Number(trimmed)
  return Number.isFinite(value) ? value : null
}
