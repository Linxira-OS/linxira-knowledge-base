import { useState, type FormEvent } from 'react'
import { useMutation, useQuery } from '@tanstack/react-query'
import { createQuotaOverride, createTokenQuota, deleteTokenQuota, listTokenQuotas } from '../../api/admin'
import type { TokenQuota } from '../../api/types'
import { Badge, Banner, Button, Card, EmptyState, ErrorState, Table, TableWrap, TBody, TD, TH, THead, TR } from '../../components/ui'
import { useI18n } from '../../lib/i18n'
import { useSession } from '../../lib/session'
import { AdminQueryGate, SelectInput, TextInput, adminKeys, parseOptionalNumber, useAdminInvalidate } from './shared'
import { ConfirmDialog } from './kit'

const QUOTA_SCOPES = ['tenant', 'space', 'user', 'role'] as const
const QUOTA_SCOPE_OPTIONS = QUOTA_SCOPES.map((value) => ({ value, label: value }))
const PERIODS = ['day', 'month', 'total'] as const
const PERIOD_OPTIONS = PERIODS.map((value) => ({ value, label: value }))
const ENFORCEMENTS = ['hard', 'soft'] as const
const ENFORCEMENT_OPTIONS = ENFORCEMENTS.map((value) => ({ value, label: value }))
const OVERRIDE_SCOPES = ['tenant', 'group', 'space', 'user'] as const
const OVERRIDE_SCOPE_OPTIONS = OVERRIDE_SCOPES.map((value) => ({ value, label: value }))
type OverrideScope = (typeof OVERRIDE_SCOPES)[number]

export function QuotasPanel() {
  const { t, format } = useI18n()
  const session = useSession()
  const features = session.features?.features
  const invalidate = useAdminInvalidate()
  const [notice, setNotice] = useState<string | null>(null)
  const [confirmQuota, setConfirmQuota] = useState<TokenQuota | null>(null)
  // Quotas are a team/enterprise tier capability. When the flag is absent
  // (older backend) keep read-only counters as a safe degradation; when the
  // backend explicitly disables it, skip the request and hide every write.
  const quotasEnabled = features?.quotas === true
  const tierRestricted = features?.quotas !== true
  const canQuery = features?.quotas !== false
  const query = useQuery({ queryKey: adminKeys.quotas, queryFn: listTokenQuotas, enabled: canQuery })
  const remove = useMutation({
    mutationFn: deleteTokenQuota,
    onSuccess: () => {
      setNotice(t('admin.quotas.deleted'))
      setConfirmQuota(null)
      invalidate(adminKeys.quotas)
    },
  })

  const quotas = query.data?.quotas ?? []
  const counters = query.data?.counters ?? []

  return (
    <div className="space-y-4">
      {tierRestricted && (
        <Banner tone="warn" title={t('admin.quotas.tierDisabled')}>
          {t('admin.quotas.tierDisabledHint')}
        </Banner>
      )}
      {notice && <Banner tone="accent">{notice}</Banner>}
      {remove.isError && <ErrorState error={remove.error} compact />}
      {quotasEnabled && (
        <ConfirmDialog
          open={confirmQuota !== null}
          title={t('admin.quotas.delete')}
          description={t('admin.quotas.deleteConfirm', { quota: confirmQuota?.quota_id ?? '' })}
          confirmLabel={t('action.delete')}
          pending={remove.isPending}
          onConfirm={() => {
            if (confirmQuota !== null) remove.mutate(confirmQuota.quota_id)
          }}
          onCancel={() => setConfirmQuota(null)}
        />
      )}
      {canQuery ? (
        <AdminQueryGate pending={query.isPending} error={query.error} onRetry={() => void query.refetch()}>
        <div className="space-y-4">
          <Card title={t('admin.quotas.title')}>
            {quotas.length === 0 ? (
              <EmptyState title={t('admin.quotas.empty')} />
            ) : (
              <TableWrap>
                <Table>
                  <THead>
                    <TR>
                      <TH>{t('admin.quotas.scope')}</TH>
                      <TH>{t('admin.quotas.period')}</TH>
                      <TH>{t('admin.quotas.limitTokens')}</TH>
                      <TH>{t('admin.quotas.limitRequests')}</TH>
                      <TH>{t('admin.quotas.costLimit')}</TH>
                      <TH>{t('admin.quotas.enforcement')}</TH>
                      <TH>{t('admin.quotas.action')}</TH>
                      <TH>{t('admin.quotas.enabled')}</TH>
                      <TH>{t('label.updatedAt')}</TH>
                      {quotasEnabled && <TH>{t('label.actions')}</TH>}
                    </TR>
                  </THead>
                  <TBody>
                    {quotas.map((quota) => (
                      <TR key={quota.quota_id}>
                        <TD className="font-mono text-[11.5px]">
                          {quota.scope_type}/{quota.scope_id}
                        </TD>
                        <TD>{quota.period}</TD>
                        <TD>{quota.limit_tokens === null ? t('label.unlimited') : format.number(quota.limit_tokens)}</TD>
                        <TD>
                          {quota.limit_requests === null ? t('label.unlimited') : format.number(quota.limit_requests)}
                        </TD>
                        <TD>
                          {quota.limit_cost_micros === null
                            ? t('label.unlimited')
                            : format.currency(quota.limit_cost_micros, quota.currency)}
                        </TD>
                        <TD>
                          <Badge tone={quota.enforcement === 'hard' ? 'danger' : 'warn'}>{quota.enforcement}</Badge>
                        </TD>
                        <TD>{quota.action}</TD>
                        <TD>
                          <Badge tone={quota.enabled ? 'accent' : 'neutral'}>
                            {quota.enabled ? t('label.yes') : t('label.no')}
                          </Badge>
                        </TD>
                        <TD className="whitespace-nowrap text-[11.5px] text-fg-3">{format.dateTime(quota.updated_at)}</TD>
                        {quotasEnabled && (
                          <TD>
                            <Button
                              size="sm"
                              variant="danger"
                              disabled={remove.isPending && remove.variables === quota.quota_id}
                              onClick={() => {
                                setNotice(null)
                                setConfirmQuota(quota)
                              }}
                            >
                              {t('action.delete')}
                            </Button>
                          </TD>
                        )}
                      </TR>
                    ))}
                  </TBody>
                </Table>
              </TableWrap>
            )}
          </Card>
          <Card title={t('admin.quotas.counters')}>
            {counters.length === 0 ? (
              <EmptyState title={t('state.empty')} />
            ) : (
              <TableWrap>
                <Table>
                  <THead>
                    <TR>
                      <TH>{t('admin.quotas.scope')}</TH>
                      <TH>{t('admin.quotas.periodStart')}</TH>
                      <TH>{t('admin.quotas.usedTokens')}</TH>
                      <TH>{t('admin.quotas.reservedTokens')}</TH>
                      <TH>{t('admin.quotas.usedRequests')}</TH>
                      <TH>{t('admin.quotas.reservedRequests')}</TH>
                      <TH>{t('admin.quotas.usedCost')}</TH>
                      <TH>{t('admin.quotas.reservedCost')}</TH>
                      <TH>{t('label.updatedAt')}</TH>
                    </TR>
                  </THead>
                  <TBody>
                    {counters.map((counter) => (
                      <TR key={`${counter.scope_type}/${counter.scope_id}/${counter.period_start}`}>
                        <TD className="font-mono text-[11.5px]">
                          {counter.scope_type}/{counter.scope_id}
                        </TD>
                        <TD className="whitespace-nowrap text-[11.5px] text-fg-3">{format.dateTime(counter.period_start)}</TD>
                        <TD>{format.number(counter.used_tokens)}</TD>
                        <TD>{format.number(counter.reserved_tokens)}</TD>
                        <TD>{format.number(counter.used_requests)}</TD>
                        <TD>{format.number(counter.reserved_requests)}</TD>
                        <TD>{format.currency(counter.used_cost_micros)}</TD>
                        <TD>{format.currency(counter.reserved_cost_micros)}</TD>
                        <TD className="whitespace-nowrap text-[11.5px] text-fg-3">{format.dateTime(counter.updated_at)}</TD>
                      </TR>
                    ))}
                  </TBody>
                </Table>
              </TableWrap>
            )}
          </Card>
        </div>
        </AdminQueryGate>
      ) : (
        <Card title={t('admin.quotas.counters')}>
          <p className="text-[11.5px] leading-5 text-fg-3">{t('admin.quotas.tierDisabledCounters')}</p>
        </Card>
      )}
      {quotasEnabled && <CreateQuotaCard />}
      {quotasEnabled && <OverrideCard />}
    </div>
  )
}

function CreateQuotaCard() {
  const { t } = useI18n()
  const invalidate = useAdminInvalidate()
  const [notice, setNotice] = useState<string | null>(null)
  const [formError, setFormError] = useState<string | null>(null)
  const [quotaId, setQuotaId] = useState('')
  const [scopeType, setScopeType] = useState<string>(QUOTA_SCOPES[0])
  const [scopeId, setScopeId] = useState('')
  const [period, setPeriod] = useState<string>(PERIODS[1])
  const [limitTokens, setLimitTokens] = useState('')
  const [limitRequests, setLimitRequests] = useState('')
  const [limitCost, setLimitCost] = useState('')
  const [currency, setCurrency] = useState('')
  const [enforcement, setEnforcement] = useState<string>(ENFORCEMENTS[0])
  const [action, setAction] = useState('')
  const [enabled, setEnabled] = useState(true)

  const create = useMutation({
    mutationFn: createTokenQuota,
    onSuccess: () => {
      setNotice(t('admin.quotas.created'))
      setQuotaId('')
      setScopeId('')
      setLimitTokens('')
      setLimitRequests('')
      setLimitCost('')
      setCurrency('')
      setAction('')
      invalidate(adminKeys.quotas)
    },
  })

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const tokens = parseOptionalNumber(limitTokens)
    const requests = parseOptionalNumber(limitRequests)
    const cost = parseOptionalNumber(limitCost)
    if (tokens === null || requests === null || cost === null) {
      setFormError(t('error.bad_request'))
      return
    }
    setFormError(null)
    create.mutate({
      quota_id: quotaId.trim() || undefined,
      scope_type: scopeType,
      scope_id: scopeId.trim(),
      period,
      limit_tokens: tokens,
      limit_requests: requests,
      limit_cost_micros: cost,
      currency: currency.trim() || undefined,
      enforcement,
      action: action.trim() || undefined,
      enabled,
    })
  }

  const hasLimit = limitTokens.trim() !== '' || limitRequests.trim() !== '' || limitCost.trim() !== ''

  return (
    <Card title={t('admin.quotas.create')}>
      <form className="grid gap-3 sm:grid-cols-3" onSubmit={submit}>
        {notice && <p className="text-[11.5px] text-accent sm:col-span-3">{notice}</p>}
        <TextInput label={t('admin.quotas.scopeId')} value={scopeId} onChange={setScopeId} />
        <SelectInput label={t('admin.quotas.scopeType')} value={scopeType} onChange={setScopeType} options={QUOTA_SCOPE_OPTIONS} />
        <TextInput label={t('admin.quotas.quotaId')} value={quotaId} onChange={setQuotaId} />
        <SelectInput label={t('admin.quotas.period')} value={period} onChange={setPeriod} options={PERIOD_OPTIONS} />
        <TextInput type="number" label={t('admin.quotas.limitTokens')} value={limitTokens} onChange={setLimitTokens} />
        <TextInput type="number" label={t('admin.quotas.limitRequests')} value={limitRequests} onChange={setLimitRequests} />
        <TextInput type="number" label={t('admin.quotas.limitCost')} value={limitCost} onChange={setLimitCost} />
        <TextInput label={t('admin.quotas.currency')} value={currency} onChange={setCurrency} />
        <SelectInput label={t('admin.quotas.enforcement')} value={enforcement} onChange={setEnforcement} options={ENFORCEMENT_OPTIONS} />
        <TextInput label={t('admin.quotas.action')} value={action} onChange={setAction} />
        <label className="flex items-center gap-2 text-[12.5px] text-fg-2">
          <input type="checkbox" className="h-3.5 w-3.5" checked={enabled} onChange={(event) => setEnabled(event.target.checked)} />
          {t('admin.quotas.enabled')}
        </label>
        {formError && <p className="text-[11.5px] text-danger sm:col-span-3">{formError}</p>}
        {create.isError && (
          <div className="sm:col-span-3">
            <ErrorState error={create.error} compact />
          </div>
        )}
        <div className="sm:col-span-3">
          <Button type="submit" disabled={scopeId.trim() === '' || !hasLimit || create.isPending}>
            {t('action.create')}
          </Button>
        </div>
      </form>
    </Card>
  )
}

function OverrideCard() {
  const { t } = useI18n()
  const invalidate = useAdminInvalidate()
  const [notice, setNotice] = useState<string | null>(null)
  const [scopeType, setScopeType] = useState<OverrideScope>(OVERRIDE_SCOPES[0])
  const [scopeId, setScopeId] = useState('')
  const [dimension, setDimension] = useState('')
  const [limitValue, setLimitValue] = useState('')
  const [reason, setReason] = useState('')
  const [expiresAt, setExpiresAt] = useState('')
  const [formError, setFormError] = useState<string | null>(null)

  const override = useMutation({
    mutationFn: createQuotaOverride,
    onSuccess: (data) => {
      setNotice(t('admin.quotas.overrideCreated', { id: data.override_id }))
      setScopeId('')
      setDimension('')
      setLimitValue('')
      setReason('')
      setExpiresAt('')
      invalidate(adminKeys.quotas)
    },
  })

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const value = parseOptionalNumber(limitValue)
    if (value === null || value === undefined) {
      setFormError(t('error.bad_request'))
      return
    }
    setFormError(null)
    override.mutate({
      scope_type: scopeType,
      scope_id: scopeId.trim(),
      dimension: dimension.trim(),
      limit_value: value,
      reason: reason.trim(),
      expires_at: expiresAt.trim() || null,
    })
  }

  const ready = scopeId.trim() !== '' && dimension.trim() !== '' && limitValue.trim() !== '' && reason.trim() !== ''

  return (
    <Card title={t('admin.quotas.override')}>
      <form className="grid gap-3 sm:grid-cols-3" onSubmit={submit}>
        <SelectInput
          label={t('admin.quotas.scopeType')}
          value={scopeType}
          onChange={(value) => setScopeType(value as OverrideScope)}
          options={OVERRIDE_SCOPE_OPTIONS}
        />
        <TextInput label={t('admin.quotas.scopeId')} value={scopeId} onChange={setScopeId} />
        <TextInput label={t('admin.quotas.overrideDimension')} value={dimension} onChange={setDimension} hint={t('admin.quotas.dimensions')} />
        <TextInput type="number" label={t('admin.quotas.overrideLimit')} value={limitValue} onChange={setLimitValue} />
        <TextInput label={t('admin.quotas.overrideExpires')} value={expiresAt} onChange={setExpiresAt} />
        <TextInput label={t('admin.quotas.overrideReason')} value={reason} onChange={setReason} />
        {notice && <p className="text-[11.5px] text-accent sm:col-span-3">{notice}</p>}
        {formError && <p className="text-[11.5px] text-danger sm:col-span-3">{formError}</p>}
        {override.isError && (
          <div className="sm:col-span-3">
            <ErrorState error={override.error} compact />
          </div>
        )}
        <div className="sm:col-span-3">
          <Button type="submit" disabled={!ready || override.isPending}>
            {t('action.create')}
          </Button>
        </div>
      </form>
    </Card>
  )
}
