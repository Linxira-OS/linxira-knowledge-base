import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { getSpaceUsage, setSpaceQuota, type SpaceQuotaBody } from '../../api/spaces'
import type { SpaceQuota } from '../../api/types'
import { Badge, Button, Card, ErrorState, Field, KeyValue, LoadingState } from '../../components/ui'
import { useI18n } from '../../lib/i18n'
import { spaceScopeLabelKey } from './badges'
import { invalidateSpaceQueries, spaceKeys } from './keys'

interface SpaceQuotaTabProps {
  spaceId: string
  canManage: boolean
  active: boolean
}

/** Usage counters plus the effective quota chain (tenant → owner → space). */
export function SpaceQuotaTab({ spaceId, canManage, active }: SpaceQuotaTabProps) {
  const { t, format } = useI18n()

  const usageQuery = useQuery({
    queryKey: spaceKeys.usage(spaceId),
    queryFn: () => getSpaceUsage(spaceId),
    retry: false,
  })

  if (usageQuery.isPending) return <LoadingState />
  if (usageQuery.isError) {
    return <ErrorState error={usageQuery.error} onRetry={() => void usageQuery.refetch()} />
  }

  const { usage, quotas } = usageQuery.data
  const spaceQuota = quotas.find((quota) => quota.scope_type === 'space') ?? null

  const scopeLabel = (scopeType: string): string => {
    const key = spaceScopeLabelKey(scopeType)
    return key ? t(key) : scopeType
  }
  const enforcementLabel = (enforcement: string): string => {
    if (enforcement === 'hard') return t('spaces.quota.hard')
    if (enforcement === 'soft') return t('spaces.quota.soft')
    return enforcement
  }
  const countLimit = (value: number | null): string =>
    value === null ? t('label.unlimited') : format.number(value)

  return (
    <div className="space-y-3">
      <Card title={t('spaces.usage.title')}>
        <KeyValue
          items={[
            [t('spaces.usage.source'), format.bytes(usage.source_bytes)],
            [t('spaces.usage.index'), format.bytes(usage.index_bytes)],
            [t('spaces.usage.history'), format.bytes(usage.history_bytes)],
            [t('spaces.usage.total'), format.bytes(usage.total_bytes)],
            [t('spaces.usage.reserved'), format.bytes(usage.reserved_bytes)],
            [t('spaces.usage.documents'), format.number(usage.documents)],
            [t('spaces.usage.chunks'), format.number(usage.chunks)],
            [t('spaces.usage.members'), format.number(usage.members)],
            [t('spaces.usage.updatedAt'), format.dateTime(usage.updated_at)],
          ]}
        />
      </Card>

      <Card title={t('spaces.quota.chain')}>
        <div className="space-y-2">
          {quotas.map((quota) => (
            <div
              key={`${quota.scope_type}:${quota.scope_id}`}
              className="rounded-lg border border-line bg-panel px-3 py-2"
            >
              <div className="flex flex-wrap items-center gap-2">
                <Badge tone={quota.scope_type === 'space' ? 'accent' : 'neutral'}>{scopeLabel(quota.scope_type)}</Badge>
                <code className="font-mono text-[11px] text-fg-3">{quota.scope_id}</code>
                {quota.scope_type !== 'space' && (
                  <span className="text-[10.5px] text-fg-3">
                    {t('spaces.quota.inherited', { scope: scopeLabel(quota.scope_type) })}
                  </span>
                )}
                <span className="text-[10.5px] text-fg-3">
                  {t('spaces.quota.enforcement')}: {enforcementLabel(quota.enforcement)}
                </span>
              </div>
              <dl className="mt-1.5 grid gap-x-4 gap-y-1 text-[11.5px] text-fg-2 sm:grid-cols-2">
                <div className="flex justify-between gap-3">
                  <dt className="text-fg-3">{t('spaces.quota.storageBytes')}</dt>
                  <dd>{quota.storage_bytes === null ? t('label.unlimited') : format.bytes(quota.storage_bytes)}</dd>
                </div>
                <div className="flex justify-between gap-3">
                  <dt className="text-fg-3">{t('spaces.quota.documents')}</dt>
                  <dd>{countLimit(quota.documents)}</dd>
                </div>
                <div className="flex justify-between gap-3">
                  <dt className="text-fg-3">{t('spaces.quota.chunks')}</dt>
                  <dd>{countLimit(quota.chunks)}</dd>
                </div>
                <div className="flex justify-between gap-3">
                  <dt className="text-fg-3">{t('spaces.quota.members')}</dt>
                  <dd>{countLimit(quota.members)}</dd>
                </div>
                <div className="flex justify-between gap-3">
                  <dt className="text-fg-3">{t('spaces.quota.spaces')}</dt>
                  <dd>{countLimit(quota.spaces)}</dd>
                </div>
              </dl>
            </div>
          ))}
        </div>
        {!spaceQuota && <p className="mt-2 text-[11.5px] text-fg-3">{t('spaces.quota.none')}</p>}
      </Card>

      {canManage && (
        <Card title={t('spaces.quota.title')}>
          <SpaceQuotaForm spaceId={spaceId} quota={spaceQuota} disabled={!active} />
        </Card>
      )}
    </div>
  )
}

interface LimitField {
  value?: number
  invalid?: boolean
}

function parseLimit(raw: string): LimitField {
  const trimmed = raw.trim()
  if (trimmed === '') return {}
  if (!/^\d+$/.test(trimmed)) return { invalid: true }
  const parsed = Number(trimmed)
  return Number.isSafeInteger(parsed) ? { value: parsed } : { invalid: true }
}

function SpaceQuotaForm({ spaceId, quota, disabled }: { spaceId: string; quota: SpaceQuota | null; disabled: boolean }) {
  const { t } = useI18n()
  const queryClient = useQueryClient()

  const [storageBytes, setStorageBytes] = useState(quota?.storage_bytes?.toString() ?? '')
  const [documents, setDocuments] = useState(quota?.documents?.toString() ?? '')
  const [chunks, setChunks] = useState(quota?.chunks?.toString() ?? '')
  const [members, setMembers] = useState(quota?.members?.toString() ?? '')
  const [enforcement, setEnforcement] = useState<'hard' | 'soft'>(quota?.enforcement === 'soft' ? 'soft' : 'hard')
  const [reason, setReason] = useState('')
  const [saved, setSaved] = useState(false)

  const limits = {
    storage_bytes: parseLimit(storageBytes),
    documents: parseLimit(documents),
    chunks: parseLimit(chunks),
    members: parseLimit(members),
  }
  const invalid = Object.values(limits).some((entry) => entry.invalid === true)

  const mutation = useMutation({
    mutationFn: () => {
      const body: SpaceQuotaBody = { enforcement }
      if (limits.storage_bytes.value !== undefined) body.storage_bytes = limits.storage_bytes.value
      if (limits.documents.value !== undefined) body.documents = limits.documents.value
      if (limits.chunks.value !== undefined) body.chunks = limits.chunks.value
      if (limits.members.value !== undefined) body.members = limits.members.value
      if (reason.trim() !== '') body.reason = reason.trim()
      return setSpaceQuota(spaceId, body)
    },
    onSuccess: () => {
      setSaved(true)
      invalidateSpaceQueries(queryClient, spaceId)
    },
  })

  const fields: { key: keyof typeof limits; label: string; value: string; setValue: (next: string) => void }[] = [
    { key: 'storage_bytes', label: t('spaces.quota.storageBytes'), value: storageBytes, setValue: setStorageBytes },
    { key: 'documents', label: t('spaces.quota.documents'), value: documents, setValue: setDocuments },
    { key: 'chunks', label: t('spaces.quota.chunks'), value: chunks, setValue: setChunks },
    { key: 'members', label: t('spaces.quota.members'), value: members, setValue: setMembers },
  ]

  return (
    <form
      className="grid gap-3 sm:grid-cols-2"
      onSubmit={(event) => {
        event.preventDefault()
        if (disabled || invalid || mutation.isPending) return
        setSaved(false)
        mutation.mutate()
      }}
    >
      <p className="text-[11px] text-fg-3 sm:col-span-2">{t('spaces.quota.hint')}</p>
      {fields.map((field) => (
        <Field
          key={field.key}
          label={field.label}
          error={limits[field.key].invalid === true ? t('spaces.quota.invalidNumber') : undefined}
        >
          <input
            type="number"
            min={0}
            step={1}
            inputMode="numeric"
            value={field.value}
            onChange={(event) => {
              field.setValue(event.target.value)
              setSaved(false)
            }}
            disabled={disabled || mutation.isPending}
            className="font-mono"
          />
        </Field>
      ))}
      <Field label={t('spaces.quota.enforcement')}>
        <select
          value={enforcement}
          onChange={(event) => {
            setEnforcement(event.target.value === 'soft' ? 'soft' : 'hard')
            setSaved(false)
          }}
          disabled={disabled || mutation.isPending}
        >
          <option value="hard">{t('spaces.quota.hard')}</option>
          <option value="soft">{t('spaces.quota.soft')}</option>
        </select>
      </Field>
      <Field label={t('spaces.quota.reason')}>
        <input
          value={reason}
          onChange={(event) => {
            setReason(event.target.value)
            setSaved(false)
          }}
          disabled={disabled || mutation.isPending}
        />
      </Field>
      <div className="flex items-center gap-2 sm:col-span-2">
        <Button variant="primary" type="submit" disabled={disabled || invalid || mutation.isPending}>
          {t('action.save')}
        </Button>
        {saved && <span className="text-[11.5px] text-accent">{t('spaces.quota.saved')}</span>}
      </div>
      {mutation.error && (
        <div className="sm:col-span-2">
          <ErrorState error={mutation.error} compact />
        </div>
      )}
    </form>
  )
}
