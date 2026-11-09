/** Locales shipped in this release. Ids match the server catalog (src/i18n.rs). */
export const LOCALES = ['zh-CN', 'en'] as const

export type Locale = (typeof LOCALES)[number]

/** A flat, externalised message catalog: locale → dotted key → text. */
export type Messages = Record<Locale, Record<string, string>>

export function isLocale(value: string | null | undefined): value is Locale {
  return value === 'zh-CN' || value === 'en'
}

/** Picks the best supported locale for a navigator.language style tag. */
export function matchLocale(tag: string | null | undefined): Locale | null {
  if (!tag) return null
  const lower = tag.toLowerCase()
  if (lower.startsWith('zh')) return 'zh-CN'
  if (lower.startsWith('en')) return 'en'
  return null
}
