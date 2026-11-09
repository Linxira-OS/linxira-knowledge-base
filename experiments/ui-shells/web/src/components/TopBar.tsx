import { useEffect, useMemo, useRef, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { searchPages, spaceOf } from '../data/wiki'
import { detectShell } from '../lib/telemetry'
import { useTheme } from '../lib/theme'

const shellLabels = {
  pwa: 'Web / PWA',
  tauri: 'Tauri 2',
  electron: 'Electron',
} as const

function ThemeToggle() {
  const { theme, toggle } = useTheme()
  return (
    <button
      type="button"
      onClick={toggle}
      aria-label={theme === 'dark' ? '切换到浅色模式' : '切换到深色模式'}
      title={theme === 'dark' ? '浅色模式' : '深色模式'}
      className="grid h-8 w-8 shrink-0 place-items-center rounded-lg border border-line text-fg-2 transition-colors hover:bg-panel hover:text-fg"
    >
      {theme === 'dark' ? (
        <svg viewBox="0 0 24 24" className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round">
          <circle cx="12" cy="12" r="4" />
          <path d="M12 3v2m0 14v2M3 12h2m14 0h2M5.6 5.6l1.4 1.4m10 10 1.4 1.4m0-12.8-1.4 1.4m-10 10-1.4 1.4" />
        </svg>
      ) : (
        <svg viewBox="0 0 24 24" className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
          <path d="M20 14.5A8.5 8.5 0 0 1 9.5 4a7 7 0 1 0 10.5 10.5Z" />
        </svg>
      )}
    </button>
  )
}

export function TopBar({ onAsk }: { onAsk: (question: string) => void }) {
  const [query, setQuery] = useState('')
  const [focused, setFocused] = useState(false)
  const inputRef = useRef<HTMLInputElement>(null)
  const navigate = useNavigate()
  const shell = detectShell()
  const hits = useMemo(() => searchPages(query, 5), [query])
  const open = focused && query.trim().length > 0

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

  const submit = () => {
    const q = query.trim()
    if (!q) return
    navigate(`/search?q=${encodeURIComponent(q)}`)
    setFocused(false)
    inputRef.current?.blur()
  }

  const go = (pageId: string) => {
    navigate(`/p/${pageId}`)
    setQuery('')
    setFocused(false)
  }

  return (
    <header className="flex h-12 shrink-0 items-center gap-3 border-b border-line bg-surface px-3 sm:gap-5 sm:px-4">
      <div className="flex items-center gap-2.5">
        <div className="grid h-7 w-7 place-items-center rounded-lg border border-accent-line bg-accent-weak text-[13px] font-semibold text-accent">
          L
        </div>
        <div className="hidden leading-tight sm:block">
          <div className="text-[13px] font-semibold tracking-[-0.01em] text-fg">Layered KB</div>
          <div className="text-[10px] text-fg-3">分层 AI 知识库</div>
        </div>
      </div>

      <div className="relative min-w-0 flex-1 sm:max-w-lg">
        <div className="flex items-center gap-2 rounded-lg border border-line bg-panel px-2.5 py-[6px] transition-colors focus-within:border-accent-line focus-within:bg-surface">
          <svg viewBox="0 0 20 20" className="h-[15px] w-[15px] shrink-0 text-fg-3" fill="none" stroke="currentColor" strokeWidth="1.6">
            <circle cx="9" cy="9" r="5.5" />
            <path d="m13.5 13.5 3 3" strokeLinecap="round" />
          </svg>
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onFocus={() => setFocused(true)}
            onBlur={() => setFocused(false)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') submit()
            }}
            placeholder="问 AI，或搜索页面与段落…"
            aria-label="AI 搜索"
            className="min-w-0 flex-1 bg-transparent text-[13px] text-fg outline-none placeholder:text-fg-3"
          />
          <kbd className="hidden shrink-0 rounded-md border border-line bg-surface px-1.5 py-0.5 font-mono text-[10px] text-fg-3 sm:block">
            Ctrl K
          </kbd>
        </div>

        {open && (
          <div
            className="absolute inset-x-0 top-[calc(100%+6px)] z-30 overflow-hidden rounded-xl border border-line bg-surface"
            onMouseDown={(e) => e.preventDefault()}
          >
            <div className="flex items-center justify-between border-b border-line px-3 py-2 text-[10px] uppercase tracking-[0.12em] text-fg-3">
              <span>检索命中</span>
              <span>{hits.length} 条</span>
            </div>
            {hits.length === 0 && (
              <div className="px-3 py-4 text-[12.5px] text-fg-2">没有匹配的页面，按 Enter 让 AI 回答。</div>
            )}
            {hits.map((hit) => (
              <button
                key={hit.page.id}
                type="button"
                onClick={() => go(hit.page.id)}
                className="flex w-full flex-col items-start gap-0.5 px-3 py-2 text-left transition-colors hover:bg-panel"
              >
                <span className="flex items-center gap-2 text-[13px] font-medium text-fg">
                  {hit.page.title}
                  <span className="text-[10.5px] font-normal text-fg-3">{spaceOf(hit.page.id)?.name}</span>
                </span>
                <span className="line-clamp-1 text-[11.5px] text-fg-3">{hit.snippet}</span>
              </button>
            ))}
            <button
              type="button"
              onClick={() => {
                const q = query.trim()
                setQuery('')
                setFocused(false)
                if (q) onAsk(q)
              }}
              className="flex w-full items-center gap-2 border-t border-line px-3 py-2.5 text-[12.5px] font-medium text-accent transition-colors hover:bg-accent-weak"
            >
              <span className="grid h-4 w-4 place-items-center rounded-md border border-accent-line text-[8.5px] font-semibold">AI</span>
              让 AI 基于全部页面回答这个问题
            </button>
          </div>
        )}
      </div>

      <div className="flex items-center gap-2">
        <span className="hidden rounded-md border border-line px-2 py-[3px] text-[10.5px] text-fg-3 md:block">
          {shellLabels[shell]}
        </span>
        <span className="hidden items-center gap-1.5 text-[11px] text-fg-3 sm:flex">
          <span className="h-1.5 w-1.5 rounded-full bg-accent" />
          离线演示
        </span>
        <ThemeToggle />
      </div>
    </header>
  )
}
