import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from 'react'
import { setApiLocale } from '../../api/client'
import { createFormatters, translate, type Formatters, type MessageKey } from './catalog'
import { LOCALES, isLocale, matchLocale, type Locale } from './types'

const STORAGE_KEY = 'lkb-locale'

interface I18nValue {
  locale: Locale
  setLocale: (locale: Locale) => void
  /** `{name}` placeholders are interpolated; keys are exhaustive at compile time. */
  t: (key: MessageKey, params?: Record<string, string | number>) => string
  format: Formatters
  supported: readonly Locale[]
}

const I18nContext = createContext<I18nValue | null>(null)

function initialLocale(): Locale {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    if (isLocale(stored)) return stored
  } catch {
    /* private mode: fall through to the system preference */
  }
  for (const tag of navigator.languages ?? [navigator.language]) {
    const matched = matchLocale(tag)
    if (matched) return matched
  }
  return 'en'
}

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>(initialLocale)

  useEffect(() => {
    document.documentElement.lang = locale
    setApiLocale(locale)
    try {
      localStorage.setItem(STORAGE_KEY, locale)
    } catch {
      /* the in-memory choice still applies */
    }
  }, [locale])

  const setLocale = useCallback((next: Locale) => setLocaleState(next), [])
  const format = useMemo(() => createFormatters(locale), [locale])
  const t = useCallback(
    (key: MessageKey, params?: Record<string, string | number>) => translate(locale, key, params),
    [locale],
  )

  const value = useMemo<I18nValue>(
    () => ({ locale, setLocale, t, format, supported: LOCALES }),
    [locale, setLocale, t, format],
  )

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>
}

export function useI18n(): I18nValue {
  const value = useContext(I18nContext)
  if (!value) throw new Error('useI18n must be used inside <I18nProvider>')
  return value
}

export type { Locale, MessageKey }
