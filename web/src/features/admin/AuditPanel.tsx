import { Fragment, useState, type FormEvent } from 'react'
import { useQuery } from '@tanstack/react-query'
import { listAudit } from '../../api/admin'
import { Badge, Button, Card, EmptyState, JsonBlock, Table, TableWrap, TBody, TD, TH, THead, TR, statusTone, type BadgeTone } from '../../components/ui'
import { useI18n } from '../../lib/i18n'
import { AdminQueryGate, TextInput, adminKeys } from './shared'

interface AuditFilters {
  actor: string
  action: string
  space: string
  from: string
  to: string
  limit: string
}

const EMPTY_FILTERS: AuditFilters = { actor: '', action: '', space: '', from: '', to: '', limit: '50' }

/** Audit results are `allow` / `denied`; anything else falls back to statusTone. */
function resultTone(result: string): BadgeTone {
  if (result === 'allow') return 'accent'
  if (result === 'denied') return 'danger'
  return statusTone(result)
}

export function AuditPanel() {
  const { t, format } = useI18n()
  const [draft, setDraft] = useState<AuditFilters>(EMPTY_FILTERS)
  const [filters, setFilters] = useState<AuditFilters>(EMPTY_FILTERS)
  const [expanded, setExpanded] = useState<string | null>(null)

  const query = useQuery({
    queryKey: [...adminKeys.audit, filters],
    queryFn: () => {
      const limit = Number(filters.limit)
      return listAudit({
        actor: filters.actor.trim() || undefined,
        action: filters.action.trim() || undefined,
        space: filters.space.trim() || undefined,
        from: filters.from.trim() || undefined,
        to: filters.to.trim() || undefined,
        limit: filters.limit.trim() !== '' && Number.isFinite(limit) ? limit : undefined,
      })
    },
  })

  const apply = (event: FormEvent) => {
    event.preventDefault()
    setFilters(draft)
    setExpanded(null)
  }

  const events = query.data?.events ?? []

  return (
    <Card title={t('admin.audit.title')}>
      <div className="space-y-3">
        <form className="grid gap-3 sm:grid-cols-3" onSubmit={apply}>
          <TextInput
            label={t('admin.audit.filterActor')}
            value={draft.actor}
            onChange={(value) => setDraft({ ...draft, actor: value })}
          />
          <TextInput
            label={t('admin.audit.filterAction')}
            value={draft.action}
            onChange={(value) => setDraft({ ...draft, action: value })}
          />
          <TextInput
            label={t('admin.audit.filterSpace')}
            value={draft.space}
            onChange={(value) => setDraft({ ...draft, space: value })}
          />
          <TextInput
            label={t('admin.audit.filterFrom')}
            value={draft.from}
            onChange={(value) => setDraft({ ...draft, from: value })}
          />
          <TextInput
            label={t('admin.audit.filterTo')}
            value={draft.to}
            onChange={(value) => setDraft({ ...draft, to: value })}
          />
          <TextInput
            type="number"
            label={t('admin.audit.limit')}
            value={draft.limit}
            onChange={(value) => setDraft({ ...draft, limit: value })}
          />
          <div className="flex items-end">
            <Button type="submit">{t('action.apply')}</Button>
          </div>
        </form>
        <AdminQueryGate
          pending={query.isPending}
          error={query.error}
          onRetry={() => void query.refetch()}
          featureDisabledKey="admin.audit.disabled"
        >
          {events.length === 0 ? (
            <EmptyState title={t('admin.audit.empty')} />
          ) : (
            <TableWrap>
              <Table>
                <THead>
                  <TR>
                    <TH>{t('admin.audit.at')}</TH>
                    <TH>{t('admin.audit.actor')}</TH>
                    <TH>{t('admin.audit.action')}</TH>
                    <TH>{t('admin.audit.target')}</TH>
                    <TH>{t('admin.audit.result')}</TH>
                    <TH>{t('admin.audit.status')}</TH>
                    <TH>{t('admin.audit.requestId')}</TH>
                    <TH>{t('admin.audit.space')}</TH>
                    <TH>{t('admin.audit.detail')}</TH>
                  </TR>
                </THead>
                <TBody>
                  {events.map((event) => (
                    <Fragment key={event.event_id}>
                      <TR>
                        <TD className="whitespace-nowrap text-[11.5px] text-fg-3">{format.dateTime(event.created_at)}</TD>
                        <TD>
                          <span className="font-mono text-[11.5px]">{event.actor_id}</span>
                          <span className="ml-1 text-[10.5px] text-fg-3">{event.actor_type}</span>
                        </TD>
                        <TD className="font-mono text-[11.5px]">{event.action}</TD>
                        <TD className="font-mono text-[11px]">
                          {event.target_type}/{event.target_id}
                        </TD>
                        <TD>
                          <Badge tone={resultTone(event.result)}>{event.result}</Badge>
                        </TD>
                        <TD>{format.number(event.status_code)}</TD>
                        <TD className="break-all font-mono text-[10.5px]">{event.request_id ?? t('label.none')}</TD>
                        <TD className="font-mono text-[11px]">{event.space_id ?? t('label.none')}</TD>
                        <TD>
                          <Button
                            size="sm"
                            variant="ghost"
                            onClick={() => setExpanded(expanded === event.event_id ? null : event.event_id)}
                          >
                            {t('label.details')}
                          </Button>
                        </TD>
                      </TR>
                      {expanded === event.event_id && (
                        <TR>
                          <TD colSpan={9} className="bg-panel">
                            <JsonBlock value={event.detail ?? ''} label={t('admin.audit.detail')} />
                          </TD>
                        </TR>
                      )}
                    </Fragment>
                  ))}
                </TBody>
              </Table>
            </TableWrap>
          )}
        </AdminQueryGate>
      </div>
    </Card>
  )
}
