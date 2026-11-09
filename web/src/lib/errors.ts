import { useCallback } from 'react'
import { ApiError } from '../api/client'
import { hasMessage } from './i18n/catalog'
import { useI18n } from './i18n'

/**
 * Turns any thrown value into a localised, user-facing line.
 *
 * Order: our own catalog for the stable backend `code` (mirrors
 * `error.<code>` on the server) → the server's localized `message`
 * (Accept-Language already applied) → a generic line keyed by status.
 */
export function useApiErrorText() {
  const { t, locale } = useI18n()
  return useCallback(
    (error: unknown): string => {
      if (error instanceof ApiError) {
        const key = `error.${error.code}`
        if (hasMessage(locale, key)) return t(key)
        if (error.code.startsWith('http.') && hasMessage(locale, error.code)) return t(error.code)
        if (error.serverMessage) return error.serverMessage
        return t('http.unknown', { status: error.status })
      }
      if (error instanceof Error && error.message) return error.message
      return t('http.unknown', { status: 0 })
    },
    [t, locale],
  )
}

/** Non-hook variant for places without a React context (logging, guards). */
export function describeApiError(error: unknown): string {
  if (error instanceof ApiError) return `${error.method} ${error.path} → ${error.status} ${error.code}`
  if (error instanceof Error) return error.message
  return String(error)
}
