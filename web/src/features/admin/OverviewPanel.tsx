import { useQuery } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import { getAdminOverview } from '../../api/admin'
import {
  Badge,
  Card,
  EmptyState,
  KeyValue,
  Table,
  TableWrap,
  TBody,
  TD,
  TH,
  THead,
  TR,
} from '../../components/ui'
import { useI18n } from '../../lib/i18n'
import { useSession } from '../../lib/session'
import { AdminQueryGate, adminKeys } from './shared'

/**
 * Admin overview backed by `GET /api/v1/admin/overview`: resource counts,
 * last-24h counters (requests/errors/cost/cache hits) and the ten most recent
 * audit events. Loading, error and empty states are handled by the gate.
 */
export function OverviewPanel() {
  const { t, format } = useI18n()
  const session = useSession()
  const features = session.features?.features

  const query = useQuery({ queryKey: adminKeys.overview, queryFn: getAdminOverview })
  const counts = query.data?.counts
  const last24h = query.data?.last_24h
  const recent = query.data?.recent_audit ?? []

  return (
    <div className="space-y-4">
      <AdminQueryGate pending={query.isPending} error={query.error} onRetry={() => void query.refetch()}>
        <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
          <Kpi label={t('admin.tab.users')} value={format.number(counts?.users)} to="/admin/users" />
          <Kpi label={t('admin.tab.spaces')} value={format.number(counts?.spaces)} to="/admin/spaces" />
          <Kpi label={t('admin.overview.documents')} value={format.number(counts?.documents)} to="/admin/spaces" />
          <Kpi label={t('admin.overview.chunks')} value={format.number(counts?.chunks)} to="/admin/spaces" />
        </div>
      </AdminQueryGate>

      <Card title={t('admin.overview.last24h')} description={t('admin.overview.note')}>
        <AdminQueryGate pending={query.isPending} error={query.error} onRetry={() => void query.refetch()}>
          <KeyValue
            items={[
              [t('admin.overview.requests24h'), format.number(last24h?.requests)],
              [t('admin.overview.errors24h'), format.number(last24h?.errors)],
              [t('admin.overview.cost24h'), format.currency(last24h?.cost_micros)],
              [t('admin.overview.cacheHitRate'), format.percent(last24h?.cache_hit_rate)],
            ]}
          />
        </AdminQueryGate>
      </Card>

      <Card
        title={t('admin.overview.recentAudit')}
        actions={
          <Link to="/admin/audit" className="text-[11.5px] text-accent">
            {t('admin.overview.viewAll')}
          </Link>
        }
      >
        <AdminQueryGate pending={query.isPending} error={query.error} onRetry={() => void query.refetch()}>
          {recent.length === 0 ? (
            <EmptyState title={t('admin.audit.empty')} />
          ) : (
            <TableWrap className="border-0">
              <Table>
                <THead>
                  <TR>
                    <TH>{t('admin.audit.at')}</TH>
                    <TH>{t('admin.audit.actor')}</TH>
                    <TH>{t('admin.audit.action')}</TH>
                    <TH>{t('admin.audit.result')}</TH>
                  </TR>
                </THead>
                <TBody>
                  {recent.map((event) => (
                    <TR key={event.event_id}>
                      <TD className="whitespace-nowrap text-[11.5px] text-fg-3">{format.dateTime(event.created_at)}</TD>
                      <TD className="font-mono text-[11.5px]">{event.actor_id}</TD>
                      <TD className="font-mono text-[11.5px]">{event.action}</TD>
                      <TD>
                        <Badge tone={event.result === 'allow' ? 'accent' : 'danger'}>{event.result}</Badge>
                      </TD>
                    </TR>
                  ))}
                </TBody>
              </Table>
            </TableWrap>
          )}
        </AdminQueryGate>
      </Card>

      <Card title={t('admin.overview.quickLinks')}>
        <div className="flex flex-wrap gap-2">
          <QuickLink to="/admin/users" label={t('admin.tab.users')} />
          <QuickLink to="/admin/roles" label={t('admin.tab.roles')} />
          <QuickLink to="/admin/groups" label={t('admin.tab.groups')} />
          <QuickLink to="/admin/spaces" label={t('admin.tab.spaces')} />
          <QuickLink to="/admin/models" label={t('admin.tab.models')} />
          <QuickLink to="/admin/usage" label={t('admin.tab.usage')} />
          <QuickLink to="/admin/quotas" label={t('admin.tab.quotas')} />
          {features?.billing && <QuickLink to="/admin/billing" label={t('admin.tab.billing')} />}
          {features?.connectors && <QuickLink to="/admin/connectors" label={t('admin.tab.connectors')} />}
          <QuickLink to="/admin/audit" label={t('admin.tab.audit')} />
        </div>
      </Card>
    </div>
  )
}

function Kpi({ label, value, to }: { label: string; value: string; to: string }) {
  return (
    <Link
      to={to}
      className="rounded-xl border border-line bg-surface px-4 py-3 transition-colors hover:border-accent-line"
    >
      <div className="text-[10.5px] uppercase tracking-[0.1em] text-fg-3">{label}</div>
      <div className="mt-1 text-[22px] font-semibold leading-7 text-fg">{value}</div>
    </Link>
  )
}

function QuickLink({ to, label }: { to: string; label: string }) {
  return (
    <Link
      to={to}
      className="rounded-lg border border-line px-2.5 py-1 text-[11.5px] text-fg-2 transition-colors hover:border-accent-line hover:text-fg"
    >
      {label}
    </Link>
  )
}
