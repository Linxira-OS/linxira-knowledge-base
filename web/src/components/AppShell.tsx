import { useEffect, useRef, useState, type ReactNode } from 'react'
import { useLocation } from 'react-router-dom'
import { QaPanel } from './QaPanel'
import { ResizeHandle } from './ResizeHandle'
import { Sidebar } from './Sidebar'
import { TopBar } from './TopBar'
import { Banner, cx } from './ui'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import { isEmbedMode } from '../lib/embed'
import { useMediaQuery } from '../lib/hooks'
import {
  COLLAPSE_RIGHT_QUERY,
  HIDE_LEFT_QUERY,
  LEFT_DEFAULT,
  LEFT_MAX,
  LEFT_MIN,
  MAIN_MIN,
  RIGHT_DEFAULT,
  RIGHT_MAX,
  RIGHT_MIN,
  loadLayout,
  saveLayout,
  type LayoutState,
} from '../lib/layout'

/**
 * Application chrome.
 *
 * Three columns (sidebar · content · QA panel) with pointer/keyboard resize
 * separators; widths and collapse state persist in localStorage. The right
 * column auto-collapses below 1100px and the left below 1024px. Embed mode
 * (`?embed=1`) keeps a single content column, no separators and no QA panel.
 */
export function AppShell({ children }: { children: ReactNode }) {
  const { t } = useI18n()
  const location = useLocation()
  const session = useSession()
  const embed = isEmbedMode()
  const compactRight = useMediaQuery(COLLAPSE_RIGHT_QUERY)
  const hideLeft = useMediaQuery(HIDE_LEFT_QUERY)
  const [layout, setLayout] = useState<LayoutState>(loadLayout)
  const [rowWidth, setRowWidth] = useState(0)
  const rowRef = useRef<HTMLDivElement | null>(null)

  const leftVisible = !hideLeft && !layout.leftCollapsed
  const rightVisible = !compactRight && !layout.rightCollapsed

  // Measure the middle row so resize limits know how much room the main column has.
  useEffect(() => {
    const element = rowRef.current
    if (!element || embed) return
    const observer = new ResizeObserver((entries) => {
      setRowWidth(entries[0]?.contentRect.width ?? 0)
    })
    observer.observe(element)
    return () => observer.disconnect()
  }, [embed])

  useEffect(() => {
    saveLayout(layout)
  }, [layout])

  // Keep the main column ≥ MAIN_MIN when the viewport narrows: shrink the
  // auxiliary columns instead of squeezing the content.
  useEffect(() => {
    if (rowWidth <= 0) return
    setLayout((prev) => {
      const leftUsed = prev.leftCollapsed || hideLeft ? 0 : prev.left
      const rightUsed = prev.rightCollapsed || compactRight ? 0 : prev.right
      let overflow = leftUsed + rightUsed + MAIN_MIN - rowWidth
      if (overflow <= 0) return prev
      let right = prev.right
      let left = prev.left
      if (rightUsed > 0) right = Math.max(RIGHT_MIN, right - overflow)
      overflow = (leftUsed > 0 ? left : 0) + (rightUsed > 0 ? right : 0) + MAIN_MIN - rowWidth
      if (overflow > 0 && leftUsed > 0) left = Math.max(LEFT_MIN, left - overflow)
      return right === prev.right && left === prev.left ? prev : { ...prev, left, right }
    })
  }, [rowWidth, hideLeft, compactRight])

  if (embed) {
    return (
      <div className="flex h-full min-h-0 flex-col overflow-hidden bg-canvas text-fg">
        <EmbedHeader />
        <main className="thin-scroll min-w-0 flex-1 overflow-y-auto bg-surface">{children}</main>
      </div>
    )
  }

  const showExpired = session.expired && session.status !== 'authenticated' && location.pathname !== '/login'

  return (
    <div className="flex h-full flex-col overflow-hidden bg-canvas text-fg">
      <TopBar />
      <div ref={rowRef} className="flex min-h-0 flex-1">
        {leftVisible ? (
          <>
            <Sidebar
              width={layout.left}
              onCollapse={() => setLayout((prev) => ({ ...prev, leftCollapsed: true }))}
            />
            <ResizeHandle
              side="left"
              value={layout.left}
              min={LEFT_MIN}
              max={LEFT_MAX}
              oppositeWidth={rightVisible ? layout.right : 0}
              containerRef={rowRef}
              onChange={(left) => setLayout((prev) => ({ ...prev, left }))}
              onReset={() => setLayout((prev) => ({ ...prev, left: LEFT_DEFAULT }))}
              label={t('layout.resizeLeft')}
            />
          </>
        ) : (
          !hideLeft && (
            <CollapseRail
              side="left"
              label={t('layout.expandLeft')}
              onExpand={() => setLayout((prev) => ({ ...prev, leftCollapsed: false }))}
            />
          )
        )}
        <main className="thin-scroll min-w-0 flex-1 overflow-y-auto bg-surface">
          {showExpired && (
            <div className="px-6 pt-4">
              <Banner tone="warn" title={t('account.sessionExpired')} />
            </div>
          )}
          {children}
        </main>
        {rightVisible ? (
          <>
            <ResizeHandle
              side="right"
              value={layout.right}
              min={RIGHT_MIN}
              max={RIGHT_MAX}
              oppositeWidth={leftVisible ? layout.left : 0}
              containerRef={rowRef}
              onChange={(right) => setLayout((prev) => ({ ...prev, right }))}
              onReset={() => setLayout((prev) => ({ ...prev, right: RIGHT_DEFAULT }))}
              label={t('layout.resizeRight')}
            />
            <QaPanel
              width={layout.right}
              onCollapse={() => setLayout((prev) => ({ ...prev, rightCollapsed: true }))}
            />
          </>
        ) : (
          !compactRight && (
            <CollapseRail
              side="right"
              label={t('layout.expandRight')}
              onExpand={() => setLayout((prev) => ({ ...prev, rightCollapsed: false }))}
            />
          )
        )}
      </div>
    </div>
  )
}

function CollapseRail({ side, label, onExpand }: { side: 'left' | 'right'; label: string; onExpand: () => void }) {
  return (
    <div
      className={cx(
        'flex w-2.5 shrink-0 flex-col items-center bg-panel',
        side === 'left' ? 'border-r border-line' : 'border-l border-line',
      )}
    >
      <button
        type="button"
        onClick={onExpand}
        aria-label={label}
        title={label}
        className="mt-3 rounded p-0.5 text-fg-3 transition-colors hover:bg-raise hover:text-fg"
      >
        <svg viewBox="0 0 16 16" className="h-3 w-3" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true">
          {side === 'left' ? (
            <path d="M6 4l4 4-4 4" strokeLinecap="round" strokeLinejoin="round" />
          ) : (
            <path d="M10 4L6 8l4 4" strokeLinecap="round" strokeLinejoin="round" />
          )}
        </svg>
      </button>
    </div>
  )
}

function EmbedHeader() {
  const { t } = useI18n()
  return (
    <header className="flex h-9 shrink-0 items-center justify-between border-b border-line bg-surface px-3">
      <span className="text-[11.5px] font-medium text-fg-2">{t('embed.title')}</span>
      <span className="text-[10.5px] text-fg-3">{t('embed.hint')}</span>
    </header>
  )
}
