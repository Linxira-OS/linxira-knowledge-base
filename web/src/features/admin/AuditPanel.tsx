import { useEffect, useMemo, useState, type FormEvent } from 'react'
import { keepPreviousData, useQuery } from '@tanstack/react-query'
import { Link, useSearchParams } from 'react-router-dom'
import { getAuditEvent, listAudit } from '../../api/admin'
import type { AuditEvent } from '../../api/types'
import {
  Badge,
  Button,
  Card,
  EmptyState,
  ErrorState,
  JsonBlock,
  KeyValue,
  Table,
  TableWrap,
  TBody,
  TD,
  TH,
  THead,
  TR,
  statusTone,
  type BadgeTone,
} from '../../components/ui'
import { useI18n } from '../../lib/i18n'
import { AdminQueryGate, TextInput, adminKeys } from './shared'
import { DetailDrawer } from './kit'

interface AuditFilters {
  actor: string
  action: string
  space: string
  from: string
  to: string
  limit: string
}

const EMPTY_FILTERS: AuditFilters = { actor: '', action: '', space: '', from: '', to: '', limit: '50' }

function filtersFromParams(params: URLSearchParams): AuditFilters {
  return {
    actor: params.get('actor') ?? '',
    action: params.get('action') ?? '',
    space: params.get('space') ?? '',
    from: params.get('from') ?? '',
    to: params.get('to') ?? '',
    limit: params.get('limit') ?? EMPTY_FILTERS.limit,
  }
}

function paramsFromFilters(filters: AuditFilters): URLSearchParams {
  const params = new URLSearchParams()
  for (const key of ['actor', 'action', 'space', 'from', 'to'] as const) {
    if (filters[key].trim() !== '') params.set(key, filters[key].trim())
  }
  if (filters.limit.trim() !== '' && filters.limit.trim() !== EMPTY_FILTERS.limit) params.set('limit', filters.limit.trim())
  return params
}

function limitValue(text: string): number | undefined {
  const trimmed = text.trim()
  if (trimmed === '') return undefined
  const value = Number(trimmed)
  return Number.isFinite(value) && value > 0 ? Math.min(value, 500) : undefined
}

/** Audit results are `allow` / `denied`; anything else falls back to statusTone. */
function resultTone(result: string): BadgeTone {
  if (result === 'allow') return 'accent'
  if (result === 'denied') return 'danger'
  return statusTone(result)
}

export function AuditPanel() {
  const { t, format } = useI18n()
  const [searchParams, setSearchParams] = useSearchParams()
  const filters = useMemo(() => filtersFromParams(searchParams), [searchParams])
  const [draft, setDraft] = useState<AuditFilters>(filters)
  const [detail, setDetail] = useState<AuditEvent | null>(null)

  // Keep the form in sync when the URL filters change (cross-links/back-forward).
  useEffect(() => {
    setDraft(filtersFromParams(searchParams))
  }, [searchParams])

  const query = useQuery({
    queryKey: [...adminKeys.audit, filters],
    queryFn: () =>
      listAudit({
        actor: filters.actor.trim() || undefined,
        action: filters.action.trim() || undefined,
        space: filters.space.trim() || undefined,
        from: filters.from.trim() || undefined,
        to: filters.to.trim() || undefined,
        limit: limitValue(filters.limit),
      }),
    placeholderData: keepPreviousData,
  })

  // Fetch the full event when a row is opened; the list row acts as placeholder.
  const detailQuery = useQuery({
    queryKey: [...adminKeys.audit, 'detail', detail?.event_id ?? ''],
    queryFn: async () => (await getAuditEvent(detail?.event_id ?? '')).event,
    enabled: detail !== null,
    placeholderData: detail ?? undefined,
  })
  const event = detailQuery.data ?? detail

  const apply = (event: FormEvent) => {
    event.preventDefault()
    setSearchParams(paramsFromFilters(draft))
  }

  const reset = () => {
    setDraft(EMPTY_FILTERS)
    setSearchParams(new URLSearchParams())
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
          <div className="flex items-end gap-2 sm:col-span-2">
            <Button type="submit">{t('action.apply')}</Button>
            <Button type="button" variant="ghost" onClick={reset}>
              {t('action.reset')}
            </Button>
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
                    <TR key={event.event_id}>
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
                        <Button size="sm" variant="ghost" onClick={() => setDetail(event)}>
                          {t('label.details')}
                        </Button>
                      </TD>
                    </TR>
                  ))}
                </TBody>
              </Table>
            </TableWrap>
          )}
        </AdminQueryGate>
      </div>
      <DetailDrawer
        open={detail !== null}
        title={t('admin.audit.drawerTitle')}
        onClose={() => setDetail(null)}
      >
        {event !== null && (
          <div className="space-y-4">
            {detailQuery.isError && (
              <ErrorState error={detailQuery.error} compact onRetry={() => void detailQuery.refetch()} />
            )}
            <KeyValue
              items={[
                [t('admin.audit.eventId'), <span className="font-mono text-[11px]">{event.event_id}</span>],
                [t('admin.audit.at'), format.dateTime(event.created_at)],
                [t('admin.audit.actor'), `${event.actor_id} (${event.actor_type})`],
                [t('admin.audit.action'), <span className="font-mono text-[11px]">{event.action}</span>],
                [t('admin.audit.target'), `${event.target_type}/${event.target_id}`],
                [
                  t('admin.audit.result'),
                  <Badge tone={resultTone(event.result)}>{event.result}</Badge>,
                ],
                [t('admin.audit.status'), format.number(event.status_code)],
                [t('admin.audit.requestId'), <span className="font-mono text-[11px]">{event.request_id ?? t('label.none')}</span>],
                [
                  t('admin.audit.space'),
                  event.space_id !== null ? (
                    <Link className="text-accent" to={`/spaces/${encodeURIComponent(event.space_id)}`}>
                      {event.space_id}
                    </Link>
                  ) : (
                    t('label.none')
                  ),
                ],
                [t('admin.audit.ip'), event.ip ?? t('label.none')],
                [t('admin.audit.userAgent'), <span className="break-all text-[11px]">{event.user_agent ?? t('label.none')}</span>],
              ]}
            />
            <JsonBlock value={event.detail ?? ''} label={t('admin.audit.detail')} />
          </div>
        )}
      </DetailDrawer>
    </Card>
  )
}
