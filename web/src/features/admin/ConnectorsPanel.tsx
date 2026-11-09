import { useEffect, useState, type FormEvent } from 'react'
import { useMutation, useQuery } from '@tanstack/react-query'
import {
  CONNECTOR_KINDS,
  createConnector,
  decodeJobStats,
  deleteConnector,
  isStubConnectorKind,
  listConnectorJobs,
  listConnectors,
  listExternalRefs,
  patchConnector,
  syncConnector,
  testConnector,
  type Connector,
  type ConnectorJob,
  type ConnectorKind,
  type ConnectorTestResult,
} from '../../api/connectors'
import { listSpaces } from '../../api/spaces'
import {
  Badge,
  Banner,
  Button,
  Card,
  EmptyState,
  ErrorState,
  Table,
  TableWrap,
  TBody,
  TD,
  TH,
  THead,
  TR,
  statusTone,
} from '../../components/ui'
import { hasMessage } from '../../lib/i18n/catalog'
import { useI18n, type MessageKey } from '../../lib/i18n'
import { isJsonText } from '../../lib/json'
import { useSession } from '../../lib/session'
import { useUnsavedWarning } from '../../lib/hooks'
import { AdminQueryGate, JsonInput, SelectInput, TextInput, adminKeys, useAdminInvalidate } from './shared'
import { ConfirmDialog, useAdminToast } from './kit'

const JOB_STATUS_KEYS: Record<string, MessageKey> = {
  queued: 'admin.connectors.jobStatus.queued',
  running: 'admin.connectors.jobStatus.running',
  completed: 'admin.connectors.jobStatus.completed',
  partial: 'admin.connectors.jobStatus.partial',
  failed: 'admin.connectors.jobStatus.failed',
}

/**
 * Connector console. The feature gate is client-side first: while
 * `features.connectors` is off this renders a neutral notice and never calls
 * the connector endpoints (the backend would answer 403 `feature_disabled`).
 */
export function ConnectorsPanel() {
  const { t } = useI18n()
  const session = useSession()

  if (session.features === null) {
    return (
      <Card title={t('admin.connectors.title')} description={t('admin.connectors.subtitle')}>
        <Banner tone="neutral" title={t('admin.connectors.unknownTitle')}>
          {t('admin.connectors.unknownBody')}
        </Banner>
      </Card>
    )
  }

  if (!session.features.features.connectors) {
    return (
      <Card title={t('admin.connectors.title')} description={t('admin.connectors.subtitle')}>
        <Banner tone="neutral" title={t('admin.connectors.disabledTitle')}>
          {t('admin.connectors.disabledBody')}
        </Banner>
      </Card>
    )
  }

  return <ConnectorsConsole />
}

function ConnectorsConsole() {
  const { t, format, locale } = useI18n()
  const invalidate = useAdminInvalidate()
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null)
  const [editing, setEditing] = useState<Connector | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [lastTest, setLastTest] = useState<{ name: string; result: ConnectorTestResult } | null>(null)

  const query = useQuery({ queryKey: [...adminKeys.connectors, 'list'], queryFn: listConnectors })
  const connectors = query.data?.connectors ?? []

  // Keep the jobs panel useful without an extra click: select the first connector.
  useEffect(() => {
    if (selectedId === null && connectors.length > 0) setSelectedId(connectors[0].connector_id)
  }, [connectors, selectedId])

  const remove = useMutation({
    mutationFn: deleteConnector,
    onSuccess: (result) => {
      if (result.removed) setNotice(t('admin.connectors.deleted'))
      setConfirmDeleteId(null)
      invalidate(adminKeys.connectors)
    },
  })

  const test = useMutation({
    mutationFn: async (connector: Connector) => ({
      connector,
      result: await testConnector(connector.connector_id),
    }),
    onSuccess: ({ connector, result }) => setLastTest({ name: connector.name, result }),
  })

  const sync = useMutation({
    mutationFn: (connectorId: string) => syncConnector(connectorId),
    onSuccess: (result, connectorId) => {
      setSelectedId(connectorId)
      setNotice(t('admin.connectors.syncQueued', { job: result.job_id }))
      invalidate(adminKeys.connectors)
    },
  })

  const testErrorLabel = (result: ConnectorTestResult): string => {
    const code = result.code ?? ''
    const localized = code !== '' && hasMessage(locale, `error.${code}`) ? t(`error.${code}`) : code
    return result.message ? `${localized} — ${result.message}` : localized
  }

  return (
    <div className="space-y-4">
      <Card title={t('admin.connectors.section.list')}>
        <AdminQueryGate
          pending={query.isPending}
          error={query.error}
          onRetry={() => void query.refetch()}
          featureDisabledKey="admin.connectors.disabled"
        >
          <div className="space-y-3">
            {notice !== null && <Banner tone="accent">{notice}</Banner>}
            {connectors.length === 0 ? (
              <EmptyState title={t('admin.connectors.empty')} />
            ) : (
              <TableWrap>
                <Table>
                  <THead>
                    <TR>
                      <TH>{t('admin.connectors.field.name')}</TH>
                      <TH>{t('admin.connectors.field.kind')}</TH>
                      <TH>{t('admin.connectors.field.space')}</TH>
                      <TH>{t('admin.connectors.field.status')}</TH>
                      <TH>{t('admin.connectors.field.lastSync')}</TH>
                      <TH>{t('admin.connectors.field.secretEnv')}</TH>
                      <TH>{t('label.actions')}</TH>
                    </TR>
                  </THead>
                  <TBody>
                    {connectors.map((connector) => {
                      const isSelected = connector.connector_id === selectedId
                      return (
                        <TR key={connector.connector_id} className={isSelected ? 'bg-accent-weak' : undefined}>
                          <TD>
                            <button
                              type="button"
                              className="text-left"
                              onClick={() => setSelectedId(connector.connector_id)}
                            >
                              <span className="font-medium text-fg">{connector.name}</span>
                              <span className="ml-1.5 font-mono text-[10.5px] text-fg-3">
                                {connector.connector_id}
                              </span>
                            </button>
                          </TD>
                          <TD>
                            {connector.kind}
                            {isStubConnectorKind(connector.kind) && (
                              <Badge tone="warn" className="ml-1.5">
                                {t('admin.connectors.stub')}
                              </Badge>
                            )}
                          </TD>
                          <TD>{connector.space_id}</TD>
                          <TD>
                            <Badge tone={statusTone(connector.status)}>{connector.status}</Badge>
                          </TD>
                          <TD>{format.dateTime(connector.last_sync_at)}</TD>
                          <TD>
                            {connector.secret_env ? (
                              <span className="font-mono text-[10.5px]">
                                {connector.secret_env}
                                {connector.secret_configured ? ' ✓' : ' —'}
                              </span>
                            ) : (
                              '—'
                            )}
                          </TD>
                          <TD>
                            <div className="flex flex-wrap gap-1.5">
                              <Button
                                size="sm"
                                variant="ghost"
                                disabled={test.isPending}
                                onClick={() => test.mutate(connector)}
                              >
                                {t('admin.connectors.action.test')}
                              </Button>
                              <Button
                                size="sm"
                                variant="ghost"
                                disabled={sync.isPending}
                                onClick={() => sync.mutate(connector.connector_id)}
                              >
                                {t('admin.connectors.action.sync')}
                              </Button>
                              <Button size="sm" variant="ghost" onClick={() => setEditing(connector)}>
                                {t('action.edit')}
                              </Button>
                              <Button size="sm" variant="ghost" onClick={() => setConfirmDeleteId(connector.connector_id)}>
                                {t('action.delete')}
                              </Button>
                            </div>
                          </TD>
                        </TR>
                      )
                    })}
                  </TBody>
                </Table>
              </TableWrap>
            )}

            {lastTest !== null && (
              <Banner
                tone={lastTest.result.ok ? 'accent' : 'warn'}
                title={lastTest.result.ok ? t('admin.connectors.testOk') : t('admin.connectors.testFailed')}
              >
                {lastTest.name}{' '}
                {lastTest.result.ok
                  ? JSON.stringify(lastTest.result.details ?? lastTest.result.status)
                  : testErrorLabel(lastTest.result)}
              </Banner>
            )}
            {test.error != null && <ErrorState error={test.error} compact />}
            {sync.error != null && <ErrorState error={sync.error} compact />}
            {remove.error != null && <ErrorState error={remove.error} compact />}
            <ConfirmDialog
              open={confirmDeleteId !== null}
              title={t('admin.connectors.deleteConfirmTitle')}
              description={t('admin.connectors.deleteConfirm')}
              confirmLabel={t('action.delete')}
              pending={remove.isPending}
              onConfirm={() => {
                if (confirmDeleteId !== null) remove.mutate(confirmDeleteId)
              }}
              onCancel={() => setConfirmDeleteId(null)}
            />
          </div>
        </AdminQueryGate>
      </Card>
      <JobsCard connectorId={selectedId} connectors={connectors} />
      <CreateConnectorCard />
      <ExternalRefsCard />
      {editing !== null && (
        <ConnectorEditDialog key={editing.connector_id} connector={editing} onClose={() => setEditing(null)} />
      )}
    </div>
  )
}

function CreateConnectorCard() {
  const { t } = useI18n()
  const invalidate = useAdminInvalidate()
  const spacesQuery = useQuery({ queryKey: ['spaces'], queryFn: listSpaces })
  const [kind, setKind] = useState<ConnectorKind>('jsonl')
  const [spaceId, setSpaceId] = useState('')
  const [name, setName] = useState('')
  const [config, setConfig] = useState('')
  const [secretEnv, setSecretEnv] = useState('')
  const [created, setCreated] = useState<string | null>(null)

  const spaces = spacesQuery.data?.spaces ?? []
  const selectedSpaceId = spaceId !== '' ? spaceId : (spaces[0]?.space_id ?? '')
  const configValid = config.trim() === '' || isJsonText(config)
  useUnsavedWarning(name.trim() !== '' || config.trim() !== '' || secretEnv.trim() !== '')

  const create = useMutation({
    mutationFn: () =>
      createConnector({
        space_id: selectedSpaceId,
        kind,
        name: name.trim(),
        ...(config.trim() !== '' ? { config_json: config.trim() } : {}),
        ...(secretEnv.trim() !== '' ? { secret_env: secretEnv.trim() } : {}),
      }),
    onSuccess: (result) => {
      setCreated(result.connector.name)
      setName('')
      setConfig('')
      setSecretEnv('')
      invalidate(adminKeys.connectors)
    },
  })

  return (
    <Card title={t('admin.connectors.section.create')}>
      <div className="space-y-3">
        <div className="grid gap-3 sm:grid-cols-3">
          <SelectInput
            label={t('admin.connectors.field.kind')}
            value={kind}
            onChange={(value) => setKind(value as ConnectorKind)}
            options={CONNECTOR_KINDS.map((candidate) => ({
              value: candidate,
              label: isStubConnectorKind(candidate) ? `${candidate} (stub)` : candidate,
            }))}
          />
          <SelectInput
            label={t('admin.connectors.field.space')}
            value={selectedSpaceId}
            onChange={setSpaceId}
            options={spaces.map((space) => ({ value: space.space_id, label: `${space.name} (${space.space_id})` }))}
          />
          <TextInput label={t('admin.connectors.field.name')} value={name} onChange={setName} />
          <TextInput
            label={t('admin.connectors.field.secretEnv')}
            value={secretEnv}
            onChange={setSecretEnv}
            hint={t('admin.connectors.secretEnvHint')}
          />
        </div>
        <JsonInput
          label={t('admin.connectors.field.config')}
          value={config}
          onChange={setConfig}
          hint={t('admin.connectors.configHint')}
          error={!configValid ? t('error.bad_request') : undefined}
        />
        {isStubConnectorKind(kind) && <Banner tone="warn">{t('admin.connectors.stubHint')}</Banner>}
        <div className="flex flex-wrap items-center gap-3">
          <Button
            variant="primary"
            disabled={create.isPending || name.trim() === '' || selectedSpaceId === '' || !configValid}
            onClick={() => create.mutate()}
          >
            {t('admin.connectors.action.create')}
          </Button>
          {created !== null && <Badge tone="accent">{t('admin.connectors.created', { name: created })}</Badge>}
        </div>
        {spacesQuery.error != null && <ErrorState error={spacesQuery.error} compact />}
        {create.error != null && <ErrorState error={create.error} />}
      </div>
    </Card>
  )
}

function JobsCard({ connectorId, connectors }: { connectorId: string | null; connectors: Connector[] }) {
  const { t, format } = useI18n()
  const selected = connectors.find((connector) => connector.connector_id === connectorId) ?? null
  const query = useQuery({
    queryKey: [...adminKeys.connectors, 'jobs', connectorId],
    queryFn: () => listConnectorJobs(connectorId as string, 20),
    enabled: connectorId !== null,
    // Sync is async (202 + job_id): poll while the newest job is still queued/running.
    refetchInterval: (queryState) => {
      const jobs: ConnectorJob[] = queryState.state.data?.jobs ?? []
      const active = jobs.some((job) => job.status === 'queued' || job.status === 'running')
      return active ? 2000 : false
    },
  })
  const jobs = query.data?.jobs ?? []

  return (
    <Card title={t('admin.connectors.section.jobs')} description={t('admin.connectors.jobsHint')}>
      {connectorId === null ? (
        <p className="text-[11.5px] text-fg-3">{t('admin.connectors.selectHint')}</p>
      ) : (
        <AdminQueryGate
          pending={query.isPending}
          error={query.error}
          onRetry={() => void query.refetch()}
          featureDisabledKey="admin.connectors.disabled"
        >
          <div className="space-y-3">
            {selected !== null && (
              <div className="text-[11.5px] text-fg-3">
                {selected.name} · <span className="font-mono">{selected.connector_id}</span>
              </div>
            )}
            {jobs.length === 0 ? (
              <EmptyState title={t('admin.connectors.emptyJobs')} />
            ) : (
              <TableWrap>
                <Table>
                  <THead>
                    <TR>
                      <TH>{t('admin.connectors.field.jobId')}</TH>
                      <TH>{t('admin.connectors.field.status')}</TH>
                      <TH>{t('admin.connectors.field.startedAt')}</TH>
                      <TH>{t('admin.connectors.field.finishedAt')}</TH>
                      <TH>{t('admin.connectors.field.stats')}</TH>
                      <TH>{t('admin.connectors.field.error')}</TH>
                    </TR>
                  </THead>
                  <TBody>
                    {jobs.map((job) => {
                      const stats = decodeJobStats(job.stats_json)
                      return (
                        <TR key={job.job_id}>
                          <TD className="font-mono text-[10.5px]">{job.job_id}</TD>
                          <TD>
                            <Badge tone={statusTone(job.status)}>
                              {JOB_STATUS_KEYS[job.status] ? t(JOB_STATUS_KEYS[job.status]) : job.status}
                            </Badge>
                          </TD>
                          <TD>{format.dateTime(job.started_at)}</TD>
                          <TD>{format.dateTime(job.finished_at)}</TD>
                          <TD>
                            {stats === null ? (
                              '—'
                            ) : (
                              <span className="flex flex-wrap gap-1">
                                <Badge>
                                  {t('admin.connectors.stats.items')} {format.number(stats.items)}
                                </Badge>
                                <Badge tone="accent">
                                  {t('admin.connectors.stats.indexed')} {format.number(stats.indexed)}
                                </Badge>
                                <Badge>
                                  {t('admin.connectors.stats.refsOnly')} {format.number(stats.refs_only)}
                                </Badge>
                                {stats.failed > 0 && (
                                  <Badge tone="danger">
                                    {t('admin.connectors.stats.failed')} {format.number(stats.failed)}
                                  </Badge>
                                )}
                              </span>
                            )}
                          </TD>
                          <TD className="max-w-[260px]">{job.error ?? '—'}</TD>
                        </TR>
                      )
                    })}
                  </TBody>
                </Table>
              </TableWrap>
            )}
            {jobs.some((job) => job.status === 'queued' || job.status === 'running') && (
              <p className="text-[11px] text-fg-3">{t('admin.connectors.polling')}</p>
            )}
          </div>
        </AdminQueryGate>
      )}
    </Card>
  )
}

function ExternalRefsCard() {
  const { t, format } = useI18n()
  const [draft, setDraft] = useState({ doc_id: '', provider: '' })
  const [filters, setFilters] = useState({ doc_id: '', provider: '' })
  const query = useQuery({
    queryKey: [...adminKeys.connectors, 'refs', filters],
    queryFn: () =>
      listExternalRefs({
        doc_id: filters.doc_id.trim() || undefined,
        provider: filters.provider.trim() || undefined,
        limit: 200,
      }),
  })
  const refs = query.data?.external_refs ?? []

  const apply = (event: FormEvent) => {
    event.preventDefault()
    setFilters(draft)
  }

  return (
    <Card title={t('admin.connectors.section.refs')} description={t('admin.connectors.refsHint')}>
      <div className="space-y-3">
        <form className="grid items-end gap-3 sm:grid-cols-3" onSubmit={apply}>
          <TextInput
            label={t('admin.connectors.filterDocId')}
            value={draft.doc_id}
            onChange={(value) => setDraft({ ...draft, doc_id: value })}
          />
          <TextInput
            label={t('admin.connectors.filterProvider')}
            value={draft.provider}
            onChange={(value) => setDraft({ ...draft, provider: value })}
          />
          <Button type="submit">{t('action.apply')}</Button>
        </form>
        <AdminQueryGate
          pending={query.isPending}
          error={query.error}
          onRetry={() => void query.refetch()}
          featureDisabledKey="admin.connectors.disabled"
        >
          {refs.length === 0 ? (
            <EmptyState title={t('admin.connectors.emptyRefs')} />
          ) : (
            <TableWrap>
              <Table>
                <THead>
                  <TR>
                    <TH>{t('admin.connectors.field.provider')}</TH>
                    <TH>{t('admin.connectors.field.url')}</TH>
                    <TH>{t('admin.connectors.field.revision')}</TH>
                    <TH>{t('admin.connectors.field.docId')}</TH>
                    <TH>{t('admin.connectors.field.fetchedAt')}</TH>
                    <TH>{t('admin.connectors.field.status')}</TH>
                  </TR>
                </THead>
                <TBody>
                  {refs.map((ref, index) => (
                    <TR key={`${ref.provider}:${ref.external_id}:${index}`}>
                      <TD>{ref.provider}</TD>
                      <TD className="max-w-[240px]">
                        {ref.url !== '' ? (
                          <a
                            className="block truncate text-accent"
                            href={ref.url}
                            target="_blank"
                            rel="noreferrer"
                            title={ref.url}
                          >
                            {ref.url}
                          </a>
                        ) : (
                          '—'
                        )}
                        {ref.external_id !== '' && (
                          <div className="truncate font-mono text-[10px] text-fg-3">{ref.external_id}</div>
                        )}
                      </TD>
                      <TD className="font-mono text-[11px]">{ref.revision ?? '—'}</TD>
                      <TD className="font-mono text-[10.5px]">{ref.doc_id !== '' ? ref.doc_id : '—'}</TD>
                      <TD>{format.dateTime(ref.fetched_at)}</TD>
                      <TD>
                        <Badge tone={statusTone(ref.status)}>{ref.status}</Badge>
                      </TD>
                    </TR>
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

function ConnectorEditDialog({ connector, onClose }: { connector: Connector; onClose: () => void }) {
  const { t } = useI18n()
  const invalidate = useAdminInvalidate()
  const toast = useAdminToast()
  const spacesQuery = useQuery({ queryKey: ['spaces'], queryFn: listSpaces })
  const [name, setName] = useState(connector.name)
  const [spaceId, setSpaceId] = useState(connector.space_id)
  const [status, setStatus] = useState(connector.status)
  const [secretEnv, setSecretEnv] = useState(connector.secret_env ?? '')
  const [config, setConfig] = useState('')

  const configValid = config.trim() === '' || isJsonText(config)
  const dirty =
    name !== connector.name ||
    spaceId !== connector.space_id ||
    status !== connector.status ||
    secretEnv !== (connector.secret_env ?? '') ||
    config.trim() !== ''
  useUnsavedWarning(dirty)

  const save = useMutation({
    mutationFn: () =>
      patchConnector(connector.connector_id, {
        name: name.trim(),
        space_id: spaceId,
        status: status.trim(),
        ...(secretEnv !== (connector.secret_env ?? '')
          ? { secret_env: secretEnv.trim() === '' ? null : secretEnv.trim() }
          : {}),
        ...(config.trim() !== '' ? { config_json: config.trim() } : {}),
      }),
    onSuccess: () => {
      toast.success(`${t('admin.connectors.updated')} · ${t('admin.toast.audited')}`)
      invalidate(adminKeys.connectors)
      onClose()
    },
  })

  const spaces = spacesQuery.data?.spaces ?? []

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose()
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={t('admin.connectors.editTitle')}
        className="thin-scroll max-h-[90vh] w-full max-w-[540px] overflow-y-auto rounded-xl border border-line bg-surface p-4 shadow-xl"
      >
        <h2 className="text-[13.5px] font-semibold text-fg">
          {t('admin.connectors.editTitle')} ·{' '}
          <span className="font-mono text-[11px] text-fg-3">{connector.connector_id}</span>
        </h2>
        <div className="mt-3 space-y-3">
          <div className="grid gap-3 sm:grid-cols-2">
            <TextInput label={t('admin.connectors.field.name')} value={name} onChange={setName} />
            <SelectInput
              label={t('admin.connectors.field.space')}
              value={spaceId}
              onChange={setSpaceId}
              options={spaces.map((space) => ({ value: space.space_id, label: `${space.name} (${space.space_id})` }))}
            />
            <TextInput
              label={t('admin.connectors.field.status')}
              value={status}
              onChange={setStatus}
              hint={t('admin.connectors.statusHint')}
            />
            <TextInput
              label={t('admin.connectors.field.secretEnv')}
              value={secretEnv}
              onChange={setSecretEnv}
              hint={t('admin.connectors.secretEnvEditHint')}
            />
          </div>
          <div className="rounded-lg border border-line bg-panel px-3 py-2">
            <div className="text-[10.5px] uppercase tracking-[0.1em] text-fg-3">
              {t('admin.connectors.currentConfig')}
            </div>
            <pre className="mt-1 whitespace-pre-wrap break-all font-mono text-[10.5px] text-fg-2">
              {JSON.stringify(connector.config, null, 2)}
            </pre>
            <p className="mt-1 text-[10.5px] text-fg-3">{t('admin.connectors.configRedactedHint')}</p>
          </div>
          <JsonInput
            label={t('admin.connectors.field.config')}
            value={config}
            onChange={setConfig}
            hint={t('admin.connectors.configEditHint')}
            error={!configValid ? t('error.bad_request') : undefined}
          />
          {save.error != null && <ErrorState error={save.error} />}
          <div className="flex justify-end gap-2">
            <Button size="sm" variant="ghost" disabled={save.isPending} onClick={onClose}>
              {t('action.cancel')}
            </Button>
            <Button
              size="sm"
              variant="primary"
              disabled={save.isPending || !configValid || name.trim() === '' || spaceId === ''}
              onClick={() => save.mutate()}
            >
              {t('action.save')}
            </Button>
          </div>
        </div>
      </div>
    </div>
  )
}
