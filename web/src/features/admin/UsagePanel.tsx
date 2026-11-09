import { useState, type FormEvent } from 'react'
import { keepPreviousData, useQuery } from '@tanstack/react-query'
import { aggregateUsage, listUsage, type UsageQuery } from '../../api/admin'
import { Badge, Button, Card, EmptyState, KeyValue, Table, TableWrap, TBody, TD, TH, THead, TR, statusTone } from '../../components/ui'
import { useI18n } from '../../lib/i18n'
import { AdminQueryGate, OffsetPager, SelectInput, TextInput, adminKeys } from './shared'

const GROUP_BY = ['user', 'space', 'alias', 'period', 'source'] as const
type GroupBy = (typeof GROUP_BY)[number]
const GROUP_BY_OPTIONS = GROUP_BY.map((value) => ({ value, label: value }))

interface UsageFilters {
  space: string
  user: string
  alias: string
  from: string
  to: string
}

const EMPTY_FILTERS: UsageFilters = { space: '', user: '', alias: '', from: '', to: '' }

function usageParams(filters: UsageFilters): UsageQuery {
  return {
    space: filters.space.trim() || undefined,
    user: filters.user.trim() || undefined,
    alias: filters.alias.trim() || undefined,
    from: filters.from.trim() || undefined,
    to: filters.to.trim() || undefined,
  }
}

export function UsagePanel() {
  const { t, format } = useI18n()
  const [draft, setDraft] = useState<UsageFilters>(EMPTY_FILTERS)
  const [filters, setFilters] = useState<UsageFilters>(EMPTY_FILTERS)
  const [groupBy, setGroupBy] = useState<GroupBy>('user')
  const [offset, setOffset] = useState(0)

  const eventsQuery = useQuery({
    queryKey: [...adminKeys.usage, 'events', filters],
    queryFn: () => listUsage(usageParams(filters)),
    placeholderData: keepPreviousData,
  })
  const aggregateQuery = useQuery({
    queryKey: [...adminKeys.usage, 'aggregate', groupBy, offset, filters],
    queryFn: () => aggregateUsage({ ...usageParams(filters), group_by: groupBy, offset }),
    placeholderData: keepPreviousData,
  })

  const apply = (event: FormEvent) => {
    event.preventDefault()
    setFilters(draft)
    setOffset(0)
  }

  const eventsData = eventsQuery.data
  const aggregateData = aggregateQuery.data

  return (
    <div className="space-y-4">
      <Card title={t('admin.usage.filters')}>
        <form className="grid gap-3 sm:grid-cols-3" onSubmit={apply}>
          <TextInput
            label={t('admin.usage.filterSpace')}
            value={draft.space}
            onChange={(value) => setDraft({ ...draft, space: value })}
          />
          <TextInput
            label={t('admin.usage.filterUser')}
            value={draft.user}
            onChange={(value) => setDraft({ ...draft, user: value })}
          />
          <TextInput
            label={t('admin.usage.filterAlias')}
            value={draft.alias}
            onChange={(value) => setDraft({ ...draft, alias: value })}
          />
          <TextInput
            label={t('admin.usage.filterFrom')}
            value={draft.from}
            onChange={(value) => setDraft({ ...draft, from: value })}
          />
          <TextInput
            label={t('admin.usage.filterTo')}
            value={draft.to}
            onChange={(value) => setDraft({ ...draft, to: value })}
          />
          <div className="flex items-end">
            <Button type="submit">{t('action.apply')}</Button>
          </div>
        </form>
      </Card>

      <AdminQueryGate pending={eventsQuery.isPending} error={eventsQuery.error} onRetry={() => void eventsQuery.refetch()}>
        {eventsData && (
          <div className="space-y-4">
            <Card title={t('admin.usage.totals')}>
              <KeyValue
                items={[
                  [t('admin.usage.requests'), format.number(eventsData.totals.requests)],
                  [t('admin.usage.errors'), format.number(eventsData.totals.errors)],
                  [t('admin.usage.input'), format.number(eventsData.totals.input_tokens)],
                  [t('admin.usage.output'), format.number(eventsData.totals.output_tokens)],
                  [t('admin.usage.cached'), format.number(eventsData.totals.cached_tokens)],
                  [t('admin.usage.cost'), format.currency(eventsData.totals.cost_micros)],
                  [t('admin.usage.providerCost'), format.currency(eventsData.totals.provider_cost_micros)],
                ]}
              />
            </Card>
            <Card title={t('admin.usage.title')}>
              {eventsData.events.length === 0 ? (
                <EmptyState title={t('admin.usage.empty')} />
              ) : (
                <TableWrap>
                  <Table>
                    <THead>
                      <TR>
                        <TH>{t('admin.usage.at')}</TH>
                        <TH>{t('admin.usage.source')}</TH>
                        <TH>{t('admin.usage.alias')}</TH>
                        <TH>{t('admin.usage.space')}</TH>
                        <TH>{t('admin.usage.user')}</TH>
                        <TH>{t('admin.usage.status')}</TH>
                        <TH>{t('admin.usage.input')}</TH>
                        <TH>{t('admin.usage.output')}</TH>
                        <TH>{t('admin.usage.cached')}</TH>
                        <TH>{t('admin.usage.cost')}</TH>
                        <TH>{t('admin.usage.latency')}</TH>
                        <TH>{t('admin.usage.requestId')}</TH>
                      </TR>
                    </THead>
                    <TBody>
                      {eventsData.events.map((event) => (
                        <TR key={event.usage_event_id}>
                          <TD className="whitespace-nowrap text-[11.5px] text-fg-3">{format.dateTime(event.at)}</TD>
                          <TD>{event.source}</TD>
                          <TD>{event.alias}</TD>
                          <TD className="font-mono text-[11px]">{event.space_id ?? t('label.none')}</TD>
                          <TD className="font-mono text-[11px]">{event.user_id}</TD>
                          <TD>
                            <Badge tone={statusTone(event.status)}>{event.status}</Badge>
                          </TD>
                          <TD>{format.number(event.input_tokens)}</TD>
                          <TD>{format.number(event.output_tokens)}</TD>
                          <TD>{format.number(event.cached_tokens)}</TD>
                          <TD className="whitespace-nowrap">
                            {format.currency(event.cost_micros, event.currency)}
                          </TD>
                          <TD>{format.number(event.latency_ms)}</TD>
                          <TD className="break-all font-mono text-[10.5px]">{event.request_id}</TD>
                        </TR>
                      ))}
                    </TBody>
                  </Table>
                </TableWrap>
              )}
            </Card>
          </div>
        )}
      </AdminQueryGate>

      <Card title={t('admin.usage.aggregate')}>
        <div className="space-y-3">
          <div className="max-w-[220px]">
            <SelectInput
              label={t('admin.usage.groupBy')}
              value={groupBy}
              onChange={(value) => {
                setGroupBy(value as GroupBy)
                setOffset(0)
              }}
              options={GROUP_BY_OPTIONS}
            />
          </div>
          <AdminQueryGate
            pending={aggregateQuery.isPending}
            error={aggregateQuery.error}
            onRetry={() => void aggregateQuery.refetch()}
          >
            {aggregateData && (
              <div className="space-y-3">
                <KeyValue
                  items={[
                    [t('admin.usage.requests'), format.number(aggregateData.totals.requests)],
                    [t('admin.usage.errors'), format.number(aggregateData.totals.errors)],
                    [t('admin.usage.input'), format.number(aggregateData.totals.input_tokens)],
                    [t('admin.usage.output'), format.number(aggregateData.totals.output_tokens)],
                    [t('admin.usage.cost'), format.currency(aggregateData.totals.cost_micros)],
                    [t('admin.usage.providerCost'), format.currency(aggregateData.totals.provider_cost_micros)],
                  ]}
                />
                {aggregateData.groups.length === 0 ? (
                  <EmptyState title={t('state.empty')} />
                ) : (
                  <TableWrap>
                    <Table>
                      <THead>
                        <TR>
                          <TH>{t('admin.usage.key')}</TH>
                          <TH>{t('admin.usage.requests')}</TH>
                          <TH>{t('admin.usage.input')}</TH>
                          <TH>{t('admin.usage.output')}</TH>
                          <TH>{t('admin.usage.cost')}</TH>
                        </TR>
                      </THead>
                      <TBody>
                        {aggregateData.groups.map((group) => (
                          <TR key={group.key || '—'}>
                            <TD className="break-all font-mono text-[11.5px]">{group.key || t('label.none')}</TD>
                            <TD>{format.number(group.requests)}</TD>
                            <TD>{format.number(group.input_tokens)}</TD>
                            <TD>{format.number(group.output_tokens)}</TD>
                            <TD>{format.currency(group.cost_micros)}</TD>
                          </TR>
                        ))}
                      </TBody>
                    </Table>
                  </TableWrap>
                )}
                <OffsetPager
                  offset={offset}
                  limit={aggregateData.limit}
                  total={aggregateData.total}
                  loading={aggregateQuery.isFetching}
                  onChange={setOffset}
                  label={t('admin.pager.showing', {
                    from: format.number(aggregateData.total === 0 ? 0 : aggregateData.offset + 1),
                    to: format.number(Math.min(aggregateData.offset + aggregateData.limit, aggregateData.total)),
                    total: format.number(aggregateData.total),
                  })}
                />
              </div>
            )}
          </AdminQueryGate>
        </div>
      </Card>
    </div>
  )
}
