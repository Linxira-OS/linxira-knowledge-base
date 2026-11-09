import { useEffect, useMemo, useRef, useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { useQueryState } from '../features/query/QueryProvider'
import { encodePathSegment } from '../api/client'
import { ensureMyNotesSpace } from '../api/notes'
import { hitReaderPath } from '../api/wiki'
import { Badge, Button, cx } from './ui'
import { BrandMark } from './BrandMark'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import { useTheme } from '../lib/theme'
import { pushToast } from '../lib/toast'

function Logo() {
  const { t } = useI18n()
  return (
    <Link to="/search" className="flex shrink-0 items-center gap-2.5">
      <BrandMark className="h-6 w-6 shrink-0 text-fg" />
      <span className="hidden leading-tight sm:block">
        <span className="block text-[13px] font-semibold tracking-[-0.01em] text-fg">{t('app.name')}</span>
        <span className="block text-[10px] text-fg-3">{t('app.tagline')}</span>
      </span>
    </Link>
  )
}

function SearchBox() {
  const { t } = useI18n()
  const navigate = useNavigate()
  const { input, setInput, results, searching, ask, currentSpace } = useQueryState()
  const [focused, setFocused] = useState(false)
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        inputRef.current?.focus()
        inputRef.current?.select()
      }
      if (event.key === 'Escape') inputRef.current?.blur()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const hits = useMemo(() => results?.hits.slice(0, 5) ?? [], [results])
  const open = focused && input.trim().length > 0

  const submitSearch = () => {
    const value = input.trim()
    if (!value) return
    setFocused(false)
    navigate(`/search?q=${encodeURIComponent(value)}`)
  }

  return (
    <div className="relative min-w-0 flex-1 sm:max-w-lg">
      <div className="flex items-center gap-2 rounded-lg border border-line bg-panel px-2.5 py-[6px] transition-colors focus-within:border-accent-line focus-within:bg-surface">
        <svg viewBox="0 0 20 20" className="h-[15px] w-[15px] shrink-0 text-fg-3" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true">
          <circle cx="9" cy="9" r="5.5" />
          <path d="m13.5 13.5 3 3" strokeLinecap="round" />
        </svg>
        <input
          ref={inputRef}
          value={input}
          onChange={(event) => setInput(event.target.value)}
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault()
              submitSearch()
            }
          }}
          placeholder={t('query.placeholder')}
          aria-label={t('nav.search')}
          className="min-w-0 flex-1 border-0 bg-transparent p-0 text-[13px] text-fg outline-none placeholder:text-fg-3"
        />
        {searching && <span className="h-3 w-3 shrink-0 animate-spin rounded-full border border-line-strong border-t-accent" />}
        <kbd className="hidden shrink-0 rounded-md border border-line bg-surface px-1.5 py-0.5 font-mono text-[10px] text-fg-3 sm:block">
          Ctrl K
        </kbd>
      </div>

      {open && (
        <div
          className="absolute inset-x-0 top-[calc(100%+6px)] z-30 overflow-hidden rounded-xl border border-line bg-surface"
          onMouseDown={(event) => event.preventDefault()}
        >
          <div className="flex items-center justify-between border-b border-line px-3 py-2 text-[10px] uppercase tracking-[0.12em] text-fg-3">
            <span>{t('query.results')}</span>
            <span>
              {currentSpace ? currentSpace.space_id : ''} · {t('query.resultsCount', { count: hits.length })}
            </span>
          </div>
          {hits.length === 0 ? (
            <div className="px-3 py-4 text-[12px] text-fg-2">
              {searching ? t('query.searching') : t('query.resultsEmpty')}
            </div>
          ) : (
            hits.map((hit) => {
              const path = hitReaderPath(hit)
              const inner = (
                <>
                  <span className="flex items-center gap-2 text-[12.5px] font-medium text-fg">
                    <span className="truncate">{hit.title}</span>
                  </span>
                  <span className="line-clamp-1 text-[11px] text-fg-3">{hit.quote}</span>
                </>
              )
              return path ? (
                <Link
                  key={hit.chunk_id}
                  to={path}
                  onClick={() => setFocused(false)}
                  className="flex w-full flex-col items-start gap-0.5 px-3 py-2 text-left transition-colors hover:bg-panel"
                >
                  {inner}
                </Link>
              ) : (
                <button
                  key={hit.chunk_id}
                  type="button"
                  onClick={() => {
                    setFocused(false)
                    navigate(`/search?q=${encodeURIComponent(input.trim())}`)
                  }}
                  className="flex w-full flex-col items-start gap-0.5 px-3 py-2 text-left transition-colors hover:bg-panel"
                >
                  {inner}
                </button>
              )
            })
          )}
          <div className="flex items-center justify-between gap-2 border-t border-line px-3 py-2">
            <span className="text-[10px] text-fg-3">{t('query.enterHint')}</span>
            <Button
              size="sm"
              variant="primary"
              onClick={() => {
                const value = input.trim()
                setFocused(false)
                if (value) {
                  navigate(`/search?q=${encodeURIComponent(value)}`)
                  void ask(value)
                }
              }}
              disabled={input.trim().length === 0}
            >
              {t('query.askAi')}
            </Button>
          </div>
        </div>
      )}
    </div>
  )
}

function SpacePicker() {
  const { t } = useI18n()
  const { readableSpaces, spaceId, setSpaceId, contentLang, setContentLang, spacesError, spacesLoading } = useQueryState()
  return (
    <div className="hidden items-center gap-1.5 md:flex">
      <select
        aria-label={t('space.select')}
        value={spaceId ?? ''}
        onChange={(event) => setSpaceId(event.target.value)}
        disabled={spacesLoading || readableSpaces.length === 0}
        className="w-auto max-w-[160px] py-[3px] text-[11.5px]"
      >
        {readableSpaces.map((space) => (
          <option key={space.space_id} value={space.space_id}>
            {space.name} ({space.space_id})
          </option>
        ))}
        {readableSpaces.length === 0 && <option value="">{spacesError ? t('state.error') : t('spaces.empty')}</option>}
      </select>
      <select
        aria-label={t('label.language')}
        value={contentLang ?? ''}
        onChange={(event) => setContentLang(event.target.value === '' ? null : event.target.value)}
        className="w-auto py-[3px] text-[11.5px]"
      >
        <option value="">{t('query.langAll')}</option>
        <option value="zh">{t('query.langZh')}</option>
        <option value="en">{t('query.langEn')}</option>
      </select>
    </div>
  )
}

function LocaleToggle() {
  const { locale, setLocale, t } = useI18n()
  const next = locale === 'zh-CN' ? 'en' : 'zh-CN'
  return (
    <button
      type="button"
      onClick={() => setLocale(next)}
      title={next === 'en' ? t('locale.switchTo') : t('locale.switchToZh')}
      className="grid h-8 shrink-0 place-items-center rounded-lg border border-line px-2 text-[11px] text-fg-2 transition-colors hover:bg-panel hover:text-fg"
    >
      {locale === 'zh-CN' ? 'EN' : '中文'}
    </button>
  )
}

function ThemeToggle() {
  const { theme, toggle } = useTheme()
  const { t } = useI18n()
  return (
    <button
      type="button"
      onClick={toggle}
      aria-label={t('label.themeToggle')}
      title={theme === 'dark' ? t('label.themeLight') : t('label.themeDark')}
      className="grid h-8 w-8 shrink-0 place-items-center rounded-lg border border-line text-fg-2 transition-colors hover:bg-panel hover:text-fg"
    >
      {theme === 'dark' ? (
        <svg viewBox="0 0 24 24" className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" aria-hidden="true">
          <circle cx="12" cy="12" r="4" />
          <path d="M12 3v2m0 14v2M3 12h2m14 0h2M5.6 5.6l1.4 1.4m10 10 1.4 1.4m0-12.8-1.4 1.4m-10 10-1.4 1.4" />
        </svg>
      ) : (
        <svg viewBox="0 0 24 24" className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="M20 14.5A8.5 8.5 0 0 1 9.5 4a7 7 0 1 0 10.5 10.5Z" />
        </svg>
      )}
    </button>
  )
}

function AccountMenu() {
  const { t } = useI18n()
  const navigate = useNavigate()
  const { status, principalLabel, logout, features } = useSession()
  const [busy, setBusy] = useState(false)

  if (status !== 'authenticated') {
    return (
      <Link
        to="/login"
        className="hidden rounded-lg border border-line px-2 py-1 text-[11.5px] text-fg-2 transition-colors hover:text-fg sm:block"
      >
        {t('account.login.title')}
      </Link>
    )
  }

  return (
    <div className="hidden items-center gap-1.5 sm:flex">
      <Badge tone="neutral">{features?.mode ?? ''}</Badge>
      <span className="max-w-[120px] truncate text-[11.5px] text-fg-2">{principalLabel ?? t('account.anonymous')}</span>
      <Button
        size="sm"
        variant="ghost"
        disabled={busy}
        onClick={() => {
          setBusy(true)
          void logout()
            .then(() => navigate('/login'))
            .finally(() => setBusy(false))
        }}
      >
        {t('account.logout')}
      </Button>
    </div>
  )
}

/**
 * Creates a note in the signed-in user's private notes space. The space is
 * ensured lazily (idempotent); if the endpoint is not deployed yet, fall back
 * to the currently selected space and surface a banner/toast.
 */
function NewNoteButton() {
  const { t } = useI18n()
  const navigate = useNavigate()
  const { currentSpace, readableSpaces } = useQueryState()
  const [busy, setBusy] = useState(false)

  const createNote = async () => {
    setBusy(true)
    try {
      const { space } = await ensureMyNotesSpace()
      navigate(`/create/${encodePathSegment(space.space_id)}`)
    } catch {
      const fallbackSpace = currentSpace ?? readableSpaces[0] ?? null
      if (fallbackSpace) {
        pushToast(t('notes.fallbackBanner'), 'warn')
        navigate(`/create/${encodePathSegment(fallbackSpace.space_id)}`)
      } else {
        pushToast(t('notes.fallbackNoSpace'), 'danger')
      }
    } finally {
      setBusy(false)
    }
  }

  return (
    <Button
      size="sm"
      variant="primary"
      className="whitespace-nowrap"
      disabled={busy}
      onClick={() => void createNote()}
    >
      <svg viewBox="0 0 16 16" className="h-3 w-3" fill="none" stroke="currentColor" strokeWidth="1.7" aria-hidden="true">
        <path d="M8 3.5v9M3.5 8h9" strokeLinecap="round" />
      </svg>
      {t('topbar.newNote')}
    </Button>
  )
}

export function TopBar() {
  const { t } = useI18n()
  const { currentSpace } = useQueryState()
  return (
    <header className="flex h-12 shrink-0 items-center gap-3 border-b border-line bg-surface px-3 sm:gap-4 sm:px-4">
      <Logo />
      <SearchBox />
      <div className={cx('flex items-center gap-1.5', 'ml-auto')}>
        <span className="hidden items-center gap-1.5 text-[11px] text-fg-3 xl:flex">
          <span className="h-1.5 w-1.5 rounded-full bg-accent" />
          {currentSpace ? currentSpace.name : t('space.selectHint')}
        </span>
        <NewNoteButton />
        <SpacePicker />
        <LocaleToggle />
        <ThemeToggle />
        <AccountMenu />
      </div>
    </header>
  )
}
