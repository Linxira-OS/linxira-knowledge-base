import { NavLink } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { getHealth } from '../api/system'
import { PageTree } from '../features/wiki/PageTree'
import { useQueryState } from '../features/query/QueryProvider'
import { Badge, cx } from './ui'
import { useI18n } from '../lib/i18n'

function NavItem({ to, label, end }: { to: string; label: string; end?: boolean }) {
  return (
    <NavLink
      to={to}
      end={end}
      className={({ isActive }) =>
        cx(
          'block truncate rounded-lg px-2.5 py-[7px] text-[13px] transition-colors',
          isActive ? 'bg-accent-weak font-medium text-accent' : 'text-fg-2 hover:bg-raise hover:text-fg',
        )
      }
    >
      {label}
    </NavLink>
  )
}

export function Sidebar() {
  const { t } = useI18n()
  const { currentSpace, readableSpaces } = useQueryState()
  const health = useQuery({ queryKey: ['health'], queryFn: getHealth, staleTime: 60_000, retry: false })

  return (
    <aside className="hidden w-60 shrink-0 flex-col border-r border-line bg-panel lg:flex">
      <div className="px-2 pb-1 pt-3">
        <div className="px-2 pb-1 text-[10.5px] font-medium uppercase tracking-[0.14em] text-fg-3">
          {t('nav.search')}
        </div>
        <nav className="space-y-0.5">
          <NavItem to="/search" label={t('nav.search')} end />
          <NavItem to="/conversations" label={t('nav.conversations')} />
          <NavItem to="/spaces" label={t('nav.spaceList')} />
          <NavItem to="/admin" label={t('nav.admin')} />
        </nav>
      </div>

      <div className="mx-3 my-2 border-t border-line" />

      <div className="thin-scroll flex-1 overflow-y-auto px-2 pb-4">
        <PageTree />
      </div>

      <div className="border-t border-line px-4 py-3">
        <div className="flex items-center gap-2 text-[11px] text-fg-3">
          <span className="h-1.5 w-1.5 rounded-full bg-accent" />
          {currentSpace ? currentSpace.space_id : t('space.selectHint')}
        </div>
        <div className="mt-1 flex items-center gap-1.5 text-[10.5px] text-fg-3">
          {health.data?.backend && <Badge>{health.data.backend}</Badge>}
          {health.data?.fts !== undefined && <Badge>{health.data.fts ? 'fts' : 'no-fts'}</Badge>}
          {readableSpaces.length > 0 && <span>{readableSpaces.length}</span>}
        </div>
      </div>
    </aside>
  )
}
