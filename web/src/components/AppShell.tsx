import type { ReactNode } from 'react'
import { useLocation } from 'react-router-dom'
import { QaPanel } from './QaPanel'
import { Sidebar } from './Sidebar'
import { TopBar } from './TopBar'
import { Banner } from './ui'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import { isEmbedMode } from '../lib/embed'

/**
 * Application chrome.
 *
 * Embed mode (`?embed=1`) renders a single content column with no navigation,
 * no QA panel and no host-API dependency, so the build can be framed anywhere.
 * The layout is intentionally CSS-only (flex + Tailwind) with no measure calls.
 */
export function AppShell({ children }: { children: ReactNode }) {
  const { t } = useI18n()
  const location = useLocation()
  const session = useSession()
  const embed = isEmbedMode()

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
      <div className="flex min-h-0 flex-1">
        <Sidebar />
        <main className="thin-scroll min-w-0 flex-1 overflow-y-auto bg-surface">
          {showExpired && (
            <div className="px-6 pt-4">
              <Banner tone="warn" title={t('account.sessionExpired')} />
            </div>
          )}
          {children}
        </main>
        <QaPanel />
      </div>
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
