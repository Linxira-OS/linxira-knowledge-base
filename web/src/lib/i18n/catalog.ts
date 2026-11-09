import type { Locale, Messages } from './types'
import { commonMessages } from './messages/common'
import { errorMessages } from './messages/errors'
import { queryMessages } from './messages/query'
import { wikiMessages } from './messages/wiki'
import { notesMessages } from './messages/notes'
import { spaceMessages } from './messages/spaces'
import { adminMessages } from './messages/admin'
import { accountMessages } from './messages/account'

const catalogs: Messages[] = [
  commonMessages,
  errorMessages,
  queryMessages,
  wikiMessages,
  notesMessages,
  spaceMessages,
  adminMessages,
  accountMessages,
]

function mergeCatalogs(locale: Locale): Record<string, string> {
  const merged: Record<string, string> = {}
  for (const catalog of catalogs) Object.assign(merged, catalog[locale])
  return merged
}

export const catalog: Record<Locale, Record<string, string>> = {
  'zh-CN': mergeCatalogs('zh-CN'),
  en: mergeCatalogs('en'),
}

export type MessageKey = keyof typeof catalog.en

/** Interpolates `{name}` placeholders. */
export function interpolate(template: string, params?: Record<string, string | number>): string {
  if (!params) return template
  return template.replace(/\{(\w+)\}/g, (match, key: string) => {
    const value = params[key]
    return value === undefined ? match : String(value)
  })
}

export function translate(locale: Locale, key: MessageKey, params?: Record<string, string | number>): string {
  const value = catalog[locale][key] ?? catalog.en[key]
  if (value === undefined) {
    if (import.meta.env.DEV) console.warn(`[i18n] missing key: ${key}`)
    return key
  }
  return interpolate(value, params)
}

/** Runtime membership check for keys assembled from data (e.g. `error.<code>`). */
export function hasMessage(locale: Locale, key: string): key is MessageKey {
  return Object.prototype.hasOwnProperty.call(catalog[locale], key)
}

/** Locale-aware formatters backed by Intl (no hand-rolled formats). */
export interface Formatters {
  number: (value: number | null | undefined) => string
  decimal: (value: number | null | undefined) => string
  percent: (value: number | null | undefined) => string
  dateTime: (value: string | number | null | undefined) => string
  date: (value: string | number | null | undefined) => string
  relative: (value: string | number | null | undefined) => string
  /** Binary size (B/KiB/MiB…) for storage counters. */
  bytes: (value: number | null | undefined) => string
  /** `micros` is 1/1 000 000 of `currency` (backend stores micros). */
  currency: (micros: number | null | undefined, currency?: string) => string
}

export function createFormatters(locale: Locale): Formatters {
  const number = new Intl.NumberFormat(locale)
  const decimal = new Intl.NumberFormat(locale, { maximumFractionDigits: 2 })
  const dateTime = new Intl.DateTimeFormat(locale, {
    dateStyle: 'medium',
    timeStyle: 'short',
  })
  const date = new Intl.DateTimeFormat(locale, { dateStyle: 'medium' })
  const relative = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' })
  const percent = new Intl.NumberFormat(locale, { style: 'percent', maximumFractionDigits: 1 })

  const parse = (value: string | number | null | undefined): Date | null => {
    if (value === null || value === undefined) return null
    const date = typeof value === 'number' ? new Date(value) : new Date(value)
    return Number.isNaN(date.getTime()) ? null : date
  }

  return {
    number: (value: number | null | undefined) => (value === null || value === undefined ? '—' : number.format(value)),
    decimal: (value: number | null | undefined) => (value === null || value === undefined ? '—' : decimal.format(value)),
    percent: (value: number | null | undefined) => (value === null || value === undefined ? '—' : percent.format(value)),
    dateTime: (value: string | number | null | undefined) => {
      const parsed = parse(value)
      return parsed ? dateTime.format(parsed) : '—'
    },
    date: (value: string | number | null | undefined) => {
      const parsed = parse(value)
      return parsed ? date.format(parsed) : '—'
    },
    relative: (value: string | number | null | undefined) => {
      const parsed = parse(value)
      if (!parsed) return '—'
      const diffMs = parsed.getTime() - Date.now()
      const minutes = Math.round(diffMs / 60000)
      if (Math.abs(minutes) < 60) return relative.format(minutes, 'minute')
      const hours = Math.round(minutes / 60)
      if (Math.abs(hours) < 24) return relative.format(hours, 'hour')
      const days = Math.round(hours / 24)
      return relative.format(days, 'day')
    },
    bytes: (value: number | null | undefined) => {
      if (value === null || value === undefined) return '—'
      const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB']
      let scaled = value
      let index = 0
      while (Math.abs(scaled) >= 1024 && index < units.length - 1) {
        scaled /= 1024
        index += 1
      }
      return `${index === 0 ? number.format(scaled) : decimal.format(scaled)} ${units[index]}`
    },
    currency: (micros: number | null | undefined, currency = 'USD') => {
      if (micros === null || micros === undefined) return '—'
      const formatter = new Intl.NumberFormat(locale, {
        style: 'currency',
        currency,
        currencyDisplay: 'narrowSymbol',
      })
      return formatter.format(micros / 1_000_000)
    },
  }
}
