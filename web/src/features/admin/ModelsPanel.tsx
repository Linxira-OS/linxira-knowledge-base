import { Fragment, useEffect, useState, type FormEvent } from 'react'
import { keepPreviousData, useMutation, useQuery } from '@tanstack/react-query'
import {
  createAlias,
  createProvider,
  createProviderModel,
  createRoute,
  deleteAlias,
  deleteProvider,
  deleteRoute,
  listAliases,
  listProviderModels,
  listProviders,
  listRoutes,
  patchAlias,
  patchProvider,
  patchRoute,
  testAdminProvider,
} from '../../api/admin'
import type { ModelAlias, ModelRoute, Provider } from '../../api/types'
import { Badge, Button, Card, EmptyState, ErrorState, KeyValue, Table, TableWrap, TBody, TD, TH, THead, TR, statusTone } from '../../components/ui'
import { isJsonText } from '../../lib/json'
import { useI18n, type MessageKey } from '../../lib/i18n'
import { hasMessage } from '../../lib/i18n/catalog'
import { useUnsavedWarning } from '../../lib/hooks'
import { AdminQueryGate, JsonInput, SelectInput, TextInput, adminKeys, parseOptionalNumber, useAdminInvalidate, useJsonDraft } from './shared'
import { ConfirmDialog, useAdminToast } from './kit'

/** Provider-test result codes with a localized explanation. */
const TEST_CODE_KEYS: Record<string, MessageKey> = {
  provider_not_configured: 'admin.models.test.code.provider_not_configured',
  provider_unreachable: 'admin.models.test.code.provider_unreachable',
  provider_http_error: 'admin.models.test.code.provider_http_error',
}

export function ModelsPanel() {
  return (
    <div className="space-y-4">
      <ProvidersCard />
      <ProviderModelsCard />
      <AliasesCard />
      <RoutesCard />
    </div>
  )
}

function ProvidersCard() {
  const { t, format } = useI18n()
  const invalidate = useAdminInvalidate()
  const toast = useAdminToast()
  const [formError, setFormError] = useState<string | null>(null)
  const [providerId, setProviderId] = useState('')
  const [kind, setKind] = useState('')
  const [baseUrl, setBaseUrl] = useState('')
  const [apiKeyEnv, setApiKeyEnv] = useState('')
  const [status, setStatus] = useState('')
  const [priority, setPriority] = useState('')
  const [weight, setWeight] = useState('')
  const json = useJsonDraft(['limits_json', 'timeouts_json'])
  useUnsavedWarning(
    [providerId, kind, baseUrl, apiKeyEnv, status, priority, weight].some((value) => value.trim() !== ''),
  )
  const [editingProvider, setEditingProvider] = useState<string | null>(null)
  const [confirmProvider, setConfirmProvider] = useState<Provider | null>(null)
  const [testProvider, setTestProvider] = useState<Provider | null>(null)

  const query = useQuery({ queryKey: adminKeys.providers, queryFn: listProviders })
  const create = useMutation({
    mutationFn: createProvider,
    onSuccess: () => {
      toast.success(t('admin.models.created'))
      setProviderId('')
      setKind('')
      setBaseUrl('')
      setApiKeyEnv('')
      setStatus('')
      setPriority('')
      setWeight('')
      json.reset()
      invalidate(adminKeys.providers)
    },
  })
  const remove = useMutation({
    mutationFn: deleteProvider,
    onSuccess: () => {
      toast.success(t('admin.models.deleted'))
      setConfirmProvider(null)
      invalidate(adminKeys.providers)
    },
  })

  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (!json.validate()) return
    const priorityValue = parseOptionalNumber(priority)
    const weightValue = parseOptionalNumber(weight)
    if (priorityValue === null || weightValue === null) {
      setFormError(t('error.bad_request'))
      return
    }
    setFormError(null)
    create.mutate({
      provider_id: providerId.trim(),
      kind: kind.trim(),
      base_url: baseUrl.trim(),
      api_key_env: apiKeyEnv.trim() || null,
      status: status.trim() || undefined,
      priority: priorityValue,
      weight: weightValue,
      limits_json: json.values.limits_json.trim() || undefined,
      timeouts_json: json.values.timeouts_json.trim() || undefined,
    })
  }

  const providers = query.data?.providers ?? []

  return (
    <>
      <Card title={t('admin.models.providers')}>
        <div className="space-y-3">
          {remove.isError && <ErrorState error={remove.error} compact />}
          <AdminQueryGate pending={query.isPending} error={query.error} onRetry={() => void query.refetch()}>
            {providers.length === 0 ? (
              <EmptyState title={t('admin.models.empty')} />
            ) : (
              <TableWrap>
                <Table>
                  <THead>
                    <TR>
                      <TH>{t('admin.models.providerId')}</TH>
                      <TH>{t('admin.models.kind')}</TH>
                      <TH>{t('admin.models.baseUrl')}</TH>
                      <TH>{t('admin.models.keyConfigured')}</TH>
                      <TH>{t('admin.models.status')}</TH>
                      <TH>{t('admin.models.priority')}</TH>
                      <TH>{t('admin.models.weight')}</TH>
                      <TH>{t('label.actions')}</TH>
                    </TR>
                  </THead>
                  <TBody>
                    {providers.map((provider) => (
                      <Fragment key={provider.provider_id}>
                        <TR>
                        <TD className="font-mono text-[11.5px]">{provider.provider_id}</TD>
                        <TD>{provider.kind}</TD>
                        <TD className="break-all font-mono text-[11px]">{provider.base_url}</TD>
                        <TD>
                          <Badge tone={provider.key_configured ? 'accent' : 'neutral'}>
                            {provider.key_configured ? t('admin.models.keyConfigured') : t('label.none')}
                          </Badge>
                        </TD>
                        <TD>
                          <Badge tone={statusTone(provider.status)}>{provider.status}</Badge>
                        </TD>
                        <TD>{format.number(provider.priority)}</TD>
                        <TD>{format.number(provider.weight)}</TD>
                        <TD>
                          <div className="flex flex-wrap items-center gap-1.5">
                            <Button size="sm" variant="ghost" onClick={() => setTestProvider(provider)}>
                              {t('admin.models.providerTest')}
                            </Button>
                            <Button
                              size="sm"
                              variant="ghost"
                              onClick={() =>
                                setEditingProvider(editingProvider === provider.provider_id ? null : provider.provider_id)
                              }
                            >
                              {t('action.edit')}
                            </Button>
                            <Button
                              size="sm"
                              variant="danger"
                              disabled={remove.isPending && remove.variables === provider.provider_id}
                              onClick={() => {
                                setConfirmProvider(provider)
                              }}
                            >
                              {t('action.delete')}
                            </Button>
                          </div>
                        </TD>
                      </TR>
                      {editingProvider === provider.provider_id && (
                        <TR>
                          <TD colSpan={8} className="bg-panel">
                            <ProviderEditRow
                              provider={provider}
                              onDone={(updated) => {
                                if (updated) toast.success(t('admin.models.updated'))
                                setEditingProvider(null)
                                invalidate(adminKeys.providers)
                              }}
                            />
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
      <Card title={t('admin.models.createProvider')} description={t('admin.models.patchReplaces')}>
        <form className="grid gap-3 sm:grid-cols-2" onSubmit={submit}>
          <TextInput label={t('admin.models.providerId')} value={providerId} onChange={setProviderId} />
          <TextInput label={t('admin.models.kind')} value={kind} onChange={setKind} />
          <TextInput label={t('admin.models.baseUrl')} value={baseUrl} onChange={setBaseUrl} />
          <TextInput label={t('admin.models.apiKeyEnv')} value={apiKeyEnv} onChange={setApiKeyEnv} hint={t('admin.models.apiKeyHint')} />
          <TextInput label={t('admin.models.status')} value={status} onChange={setStatus} />
          <TextInput type="number" label={t('admin.models.priority')} value={priority} onChange={setPriority} />
          <TextInput type="number" label={t('admin.models.weight')} value={weight} onChange={setWeight} />
          <JsonInput
            label={t('admin.models.limitsJson')}
            value={json.values.limits_json}
            onChange={(value) => json.set('limits_json', value)}
            error={json.error('limits_json')}
          />
          <JsonInput
            label={t('admin.models.timeoutsJson')}
            value={json.values.timeouts_json}
            onChange={(value) => json.set('timeouts_json', value)}
            error={json.error('timeouts_json')}
          />
          {formError && <p className="text-[11.5px] text-danger sm:col-span-2">{formError}</p>}
          {create.isError && (
            <div className="sm:col-span-2">
              <ErrorState error={create.error} compact />
            </div>
          )}
          <div className="sm:col-span-2">
            <Button
              type="submit"
              disabled={providerId.trim() === '' || kind.trim() === '' || baseUrl.trim() === '' || create.isPending}
            >
              {t('action.create')}
            </Button>
          </div>
        </form>
      </Card>
      {testProvider !== null && (
        <ProviderTestDialog
          key={testProvider.provider_id}
          provider={testProvider}
          onClose={() => setTestProvider(null)}
        />
      )}
      <ConfirmDialog
        open={confirmProvider !== null}
        title={t('admin.models.deleteProvider')}
        description={t('admin.models.deleteConfirm', { name: confirmProvider?.provider_id ?? '' })}
        confirmLabel={t('action.delete')}
        pending={remove.isPending}
        onConfirm={() => {
          if (confirmProvider !== null) remove.mutate(confirmProvider.provider_id)
        }}
        onCancel={() => setConfirmProvider(null)}
      />
    </>
  )
}

function ProviderEditRow({ provider, onDone }: { provider: Provider; onDone: (updated: boolean) => void }) {
  const { t } = useI18n()
  const [kind, setKind] = useState(provider.kind)
  const [baseUrl, setBaseUrl] = useState(provider.base_url)
  const [apiKeyEnv, setApiKeyEnv] = useState('')
  const [status, setStatus] = useState(provider.status)
  const [priority, setPriority] = useState(String(provider.priority))
  const [weight, setWeight] = useState(String(provider.weight))
  const [limits, setLimits] = useState(provider.limits_json)
  const [timeouts, setTimeouts] = useState(provider.timeouts_json)
  const [formError, setFormError] = useState<string | null>(null)

  const save = useMutation({
    mutationFn: (values: { priority: number; weight: number }) =>
      patchProvider(provider.provider_id, {
        provider_id: provider.provider_id,
        kind: kind.trim(),
        base_url: baseUrl.trim(),
        status: status.trim() || undefined,
        priority: values.priority,
        weight: values.weight,
        limits_json: limits.trim() || undefined,
        timeouts_json: timeouts.trim() || undefined,
        ...(apiKeyEnv.trim() !== '' ? { api_key_env: apiKeyEnv.trim() } : {}),
      }),
    onSuccess: () => onDone(true),
    onError: () => onDone(false),
  })

  const submit = () => {
    const priorityValue = parseOptionalNumber(priority)
    const weightValue = parseOptionalNumber(weight)
    if (
      priorityValue === null ||
      weightValue === null ||
      priorityValue === undefined ||
      weightValue === undefined ||
      !isJsonText(limits) ||
      !isJsonText(timeouts) ||
      kind.trim() === '' ||
      baseUrl.trim() === ''
    ) {
      setFormError(t('error.bad_request'))
      return
    }
    setFormError(null)
    save.mutate({ priority: priorityValue, weight: weightValue })
  }

  return (
    <div className="space-y-2">
      <div className="grid gap-3 sm:grid-cols-2">
        <TextInput label={t('admin.models.kind')} value={kind} onChange={setKind} />
        <TextInput label={t('admin.models.baseUrl')} value={baseUrl} onChange={setBaseUrl} />
        <TextInput
          label={t('admin.models.apiKeyEnv')}
          value={apiKeyEnv}
          onChange={setApiKeyEnv}
          hint={t('admin.models.apiKeyHint')}
        />
        <TextInput label={t('admin.models.status')} value={status} onChange={setStatus} />
        <TextInput type="number" label={t('admin.models.priority')} value={priority} onChange={setPriority} />
        <TextInput type="number" label={t('admin.models.weight')} value={weight} onChange={setWeight} />
        <JsonInput label={t('admin.models.limitsJson')} value={limits} onChange={setLimits} />
        <JsonInput label={t('admin.models.timeoutsJson')} value={timeouts} onChange={setTimeouts} />
      </div>
      {formError && <p className="text-[11.5px] text-danger">{formError}</p>}
      <div className="flex items-center gap-2">
        <Button size="sm" variant="primary" disabled={save.isPending} onClick={submit}>
          {t('action.save')}
        </Button>
        <Button size="sm" variant="ghost" disabled={save.isPending} onClick={() => onDone(false)}>
          {t('action.cancel')}
        </Button>
        {save.error != null && <ErrorState error={save.error} compact />}
      </div>
    </div>
  )
}

function ProviderModelsCard() {
  const { t, format } = useI18n()
  const invalidate = useAdminInvalidate()
  const toast = useAdminToast()
  const [formError, setFormError] = useState<string | null>(null)
  const [modelId, setModelId] = useState('')
  const [providerId, setProviderId] = useState('')
  const [upstreamModel, setUpstreamModel] = useState('')
  const [contextWindow, setContextWindow] = useState('')
  const json = useJsonDraft(['pricing_json', 'capabilities_json'])
  useUnsavedWarning([modelId, providerId, upstreamModel, contextWindow].some((value) => value.trim() !== ''))

  const query = useQuery({ queryKey: adminKeys.providerModels, queryFn: listProviderModels })
  const create = useMutation({
    mutationFn: createProviderModel,
    onSuccess: () => {
      toast.success(t('admin.models.created'))
      setModelId('')
      setProviderId('')
      setUpstreamModel('')
      setContextWindow('')
      json.reset()
      invalidate(adminKeys.providerModels, adminKeys.providers)
    },
  })

  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (!json.validate()) return
    const contextValue = parseOptionalNumber(contextWindow)
    if (contextValue === null) {
      setFormError(t('error.bad_request'))
      return
    }
    setFormError(null)
    create.mutate({
      provider_model_id: modelId.trim(),
      provider_id: providerId.trim(),
      upstream_model: upstreamModel.trim(),
      context_window: contextValue,
      pricing_json: json.values.pricing_json.trim() || undefined,
      capabilities_json: json.values.capabilities_json.trim() || undefined,
    })
  }

  const models = query.data?.models ?? []

  return (
    <>
      <Card title={t('admin.models.providerModels')}>
        <div className="space-y-3">
          <AdminQueryGate pending={query.isPending} error={query.error} onRetry={() => void query.refetch()}>
            {models.length === 0 ? (
              <EmptyState title={t('admin.models.empty')} />
            ) : (
              <TableWrap>
                <Table>
                  <THead>
                    <TR>
                      <TH>{t('admin.models.providerModelId')}</TH>
                      <TH>{t('admin.models.providerId')}</TH>
                      <TH>{t('admin.models.upstreamModel')}</TH>
                      <TH>{t('admin.models.contextWindow')}</TH>
                      <TH>{t('admin.models.status')}</TH>
                      <TH>{t('admin.models.pricingJson')}</TH>
                      <TH>{t('admin.models.capabilities')}</TH>
                    </TR>
                  </THead>
                  <TBody>
                    {models.map((model) => (
                      <TR key={model.provider_model_id}>
                        <TD className="font-mono text-[11.5px]">{model.provider_model_id}</TD>
                        <TD className="font-mono text-[11.5px]">{model.provider_id}</TD>
                        <TD>{model.upstream_model}</TD>
                        <TD>{format.number(model.context_window)}</TD>
                        <TD>
                          <Badge tone={statusTone(model.status)}>{model.status}</Badge>
                        </TD>
                        <TD className="break-all font-mono text-[11px]">{model.pricing_json || t('label.none')}</TD>
                        <TD className="break-all font-mono text-[11px]">{model.capabilities_json || t('label.none')}</TD>
                      </TR>
                    ))}
                  </TBody>
                </Table>
              </TableWrap>
            )}
          </AdminQueryGate>
        </div>
      </Card>
      <Card title={t('admin.models.createProviderModel')}>
        <form className="grid gap-3 sm:grid-cols-2" onSubmit={submit}>
          <TextInput label={t('admin.models.providerModelId')} value={modelId} onChange={setModelId} />
          <TextInput label={t('admin.models.providerId')} value={providerId} onChange={setProviderId} />
          <TextInput label={t('admin.models.upstreamModel')} value={upstreamModel} onChange={setUpstreamModel} />
          <TextInput type="number" label={t('admin.models.contextWindow')} value={contextWindow} onChange={setContextWindow} />
          <JsonInput
            label={t('admin.models.pricingJson')}
            value={json.values.pricing_json}
            onChange={(value) => json.set('pricing_json', value)}
            error={json.error('pricing_json')}
          />
          <JsonInput
            label={t('admin.models.capabilities')}
            value={json.values.capabilities_json}
            onChange={(value) => json.set('capabilities_json', value)}
            error={json.error('capabilities_json')}
          />
          {formError && <p className="text-[11.5px] text-danger sm:col-span-2">{formError}</p>}
          {create.isError && (
            <div className="sm:col-span-2">
              <ErrorState error={create.error} compact />
            </div>
          )}
          <div className="sm:col-span-2">
            <Button
              type="submit"
              disabled={modelId.trim() === '' || providerId.trim() === '' || upstreamModel.trim() === '' || create.isPending}
            >
              {t('action.create')}
            </Button>
          </div>
        </form>
      </Card>
    </>
  )
}

function AliasesCard() {
  const { t, format } = useI18n()
  const invalidate = useAdminInvalidate()
  const toast = useAdminToast()
  const [editing, setEditing] = useState<string | null>(null)
  const [publicId, setPublicId] = useState('')
  const [displayName, setDisplayName] = useState('')
  const [status, setStatus] = useState('')
  const json = useJsonDraft(['capabilities_json', 'access_policy_json', 'price_json'])
  useUnsavedWarning([publicId, displayName, status].some((value) => value.trim() !== ''))
  const [confirmAlias, setConfirmAlias] = useState<ModelAlias | null>(null)

  const query = useQuery({ queryKey: adminKeys.aliases, queryFn: listAliases })
  const create = useMutation({
    mutationFn: createAlias,
    onSuccess: () => {
      toast.success(t('admin.models.created'))
      setPublicId('')
      setDisplayName('')
      setStatus('')
      json.reset()
      invalidate(adminKeys.aliases)
    },
  })
  const remove = useMutation({
    mutationFn: deleteAlias,
    onSuccess: () => {
      toast.success(t('admin.models.deleted'))
      setConfirmAlias(null)
      invalidate(adminKeys.aliases, adminKeys.routes)
    },
  })

  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (!json.validate()) return
    create.mutate({
      public_id: publicId.trim(),
      display_name: displayName.trim() || null,
      status: status.trim() || undefined,
      capabilities_json: json.values.capabilities_json.trim() || undefined,
      access_policy_json: json.values.access_policy_json.trim() || undefined,
      price_json: json.values.price_json.trim() || undefined,
    })
  }

  const aliases = query.data?.aliases ?? []

  return (
    <>
      <Card title={t('admin.models.aliases')}>
        <div className="space-y-3">
          {remove.isError && <ErrorState error={remove.error} compact />}
          <AdminQueryGate pending={query.isPending} error={query.error} onRetry={() => void query.refetch()}>
            {aliases.length === 0 ? (
              <EmptyState title={t('admin.models.empty')} />
            ) : (
              <TableWrap>
                <Table>
                  <THead>
                    <TR>
                      <TH>{t('admin.models.publicId')}</TH>
                      <TH>{t('admin.models.displayName')}</TH>
                      <TH>{t('admin.models.version')}</TH>
                      <TH>{t('admin.models.status')}</TH>
                      <TH>{t('admin.models.capabilities')}</TH>
                      <TH>{t('label.actions')}</TH>
                    </TR>
                  </THead>
                  <TBody>
                    {aliases.map((alias) => (
                      <Fragment key={alias.public_id}>
                        <TR>
                          <TD className="font-mono text-[11.5px]">{alias.public_id}</TD>
                          <TD className="text-fg">{alias.display_name}</TD>
                          <TD>{format.number(alias.version)}</TD>
                          <TD>
                            <Badge tone={statusTone(alias.status)}>{alias.status}</Badge>
                          </TD>
                          <TD className="break-all font-mono text-[11px]">{alias.capabilities_json || t('label.none')}</TD>
                          <TD>
                            <div className="flex items-center gap-1.5">
                              <Button
                                size="sm"
                                onClick={() => setEditing(editing === alias.public_id ? null : alias.public_id)}
                              >
                                {t('action.edit')}
                              </Button>
                              <Button
                                size="sm"
                                variant="danger"
                                disabled={remove.isPending && remove.variables === alias.public_id}
                                onClick={() => {
                                  setConfirmAlias(alias)
                                }}
                              >
                                {t('action.delete')}
                              </Button>
                            </div>
                          </TD>
                        </TR>
                        {editing === alias.public_id && (
                          <TR>
                            <TD colSpan={6} className="bg-panel">
                              <AliasEditRow
                                alias={alias}
                                onDone={() => {
                                  toast.success(t('admin.models.updated'))
                                  setEditing(null)
                                }}
                              />
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
      <Card title={t('admin.models.createAlias')}>
        <form className="grid gap-3 sm:grid-cols-2" onSubmit={submit}>
          <TextInput label={t('admin.models.publicId')} value={publicId} onChange={setPublicId} />
          <TextInput label={t('admin.models.displayName')} value={displayName} onChange={setDisplayName} />
          <TextInput label={t('admin.models.status')} value={status} onChange={setStatus} />
          <JsonInput
            label={t('admin.models.capabilities')}
            value={json.values.capabilities_json}
            onChange={(value) => json.set('capabilities_json', value)}
            error={json.error('capabilities_json')}
          />
          <JsonInput
            label={t('admin.models.accessPolicy')}
            value={json.values.access_policy_json}
            onChange={(value) => json.set('access_policy_json', value)}
            error={json.error('access_policy_json')}
          />
          <JsonInput
            label={t('admin.models.price')}
            value={json.values.price_json}
            onChange={(value) => json.set('price_json', value)}
            error={json.error('price_json')}
          />
          {create.isError && (
            <div className="sm:col-span-2">
              <ErrorState error={create.error} compact />
            </div>
          )}
          <div className="sm:col-span-2">
            <Button type="submit" disabled={publicId.trim() === '' || create.isPending}>
              {t('action.create')}
            </Button>
          </div>
        </form>
      </Card>
      <ConfirmDialog
        open={confirmAlias !== null}
        title={t('admin.models.deleteAlias')}
        description={t('admin.models.deleteConfirm', { name: confirmAlias?.public_id ?? '' })}
        confirmLabel={t('action.delete')}
        pending={remove.isPending}
        onConfirm={() => {
          if (confirmAlias !== null) remove.mutate(confirmAlias.public_id)
        }}
        onCancel={() => setConfirmAlias(null)}
      />
    </>
  )
}

function AliasEditRow({ alias, onDone }: { alias: ModelAlias; onDone: () => void }) {
  const { t } = useI18n()
  const invalidate = useAdminInvalidate()
  const [displayName, setDisplayName] = useState(alias.display_name)
  const [status, setStatus] = useState(alias.status)
  const json = useJsonDraft(['capabilities_json', 'access_policy_json', 'price_json'], {
    capabilities_json: alias.capabilities_json,
    access_policy_json: alias.access_policy_json,
    price_json: alias.price_json,
  })

  const patch = useMutation({
    mutationFn: () =>
      patchAlias(alias.public_id, {
        public_id: alias.public_id,
        display_name: displayName.trim() || null,
        status: status.trim() || undefined,
        capabilities_json: json.values.capabilities_json.trim() || undefined,
        access_policy_json: json.values.access_policy_json.trim() || undefined,
        price_json: json.values.price_json.trim() || undefined,
      }),
    onSuccess: () => {
      invalidate(adminKeys.aliases)
      onDone()
    },
  })

  return (
    <form
      className="grid gap-3 py-1 sm:grid-cols-2"
      onSubmit={(event) => {
        event.preventDefault()
        if (!json.validate()) return
        patch.mutate()
      }}
    >
      <div className="sm:col-span-2">
        <p className="text-[11px] text-fg-3">{t('admin.models.patchReplaces')}</p>
      </div>
      <TextInput label={t('admin.models.displayName')} value={displayName} onChange={setDisplayName} />
      <TextInput label={t('admin.models.status')} value={status} onChange={setStatus} />
      <JsonInput
        label={t('admin.models.capabilities')}
        value={json.values.capabilities_json}
        onChange={(value) => json.set('capabilities_json', value)}
        error={json.error('capabilities_json')}
      />
      <JsonInput
        label={t('admin.models.accessPolicy')}
        value={json.values.access_policy_json}
        onChange={(value) => json.set('access_policy_json', value)}
        error={json.error('access_policy_json')}
      />
      <JsonInput
        label={t('admin.models.price')}
        value={json.values.price_json}
        onChange={(value) => json.set('price_json', value)}
        error={json.error('price_json')}
      />
      {patch.isError && (
        <div className="sm:col-span-2">
          <ErrorState error={patch.error} compact />
        </div>
      )}
      <div className="flex items-center gap-1.5 sm:col-span-2">
        <Button type="submit" size="sm" disabled={patch.isPending}>
          {t('action.save')}
        </Button>
        <Button size="sm" variant="ghost" onClick={onDone}>
          {t('action.cancel')}
        </Button>
      </div>
    </form>
  )
}

function RoutesCard() {
  const { t, format } = useI18n()
  const invalidate = useAdminInvalidate()
  const toast = useAdminToast()
  const [formError, setFormError] = useState<string | null>(null)
  const [editing, setEditing] = useState<string | null>(null)
  const [aliasFilter, setAliasFilter] = useState('')
  const [routeId, setRouteId] = useState('')
  const [aliasPublicId, setAliasPublicId] = useState('')
  const [providerModelId, setProviderModelId] = useState('')
  const [weight, setWeight] = useState('')
  const [priority, setPriority] = useState('')
  const [confirmRoute, setConfirmRoute] = useState<ModelRoute | null>(null)
  useUnsavedWarning(
    [routeId, aliasPublicId, providerModelId, weight, priority].some((value) => value.trim() !== ''),
  )

  const aliasesQuery = useQuery({ queryKey: adminKeys.aliases, queryFn: listAliases })
  const query = useQuery({
    queryKey: [...adminKeys.routes, aliasFilter],
    queryFn: () => listRoutes(aliasFilter || undefined),
    placeholderData: keepPreviousData,
  })
  const create = useMutation({
    mutationFn: createRoute,
    onSuccess: () => {
      toast.success(t('admin.models.created'))
      setRouteId('')
      setAliasPublicId('')
      setProviderModelId('')
      setWeight('')
      setPriority('')
      invalidate(adminKeys.routes)
    },
  })
  const remove = useMutation({
    mutationFn: deleteRoute,
    onSuccess: () => {
      toast.success(t('admin.models.deleted'))
      setConfirmRoute(null)
      invalidate(adminKeys.routes)
    },
  })

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const weightValue = parseOptionalNumber(weight)
    const priorityValue = parseOptionalNumber(priority)
    if (weightValue === null || priorityValue === null) {
      setFormError(t('error.bad_request'))
      return
    }
    setFormError(null)
    create.mutate({
      route_id: routeId.trim(),
      alias_public_id: aliasPublicId.trim(),
      provider_model_id: providerModelId.trim(),
      weight: weightValue,
      priority: priorityValue,
    })
  }

  const routes = query.data?.routes ?? []
  const aliasOptions = [
    { value: '', label: t('admin.models.allAliases') },
    ...(aliasesQuery.data?.aliases ?? []).map((alias) => ({ value: alias.public_id, label: alias.public_id })),
  ]

  return (
    <>
      <Card title={t('admin.models.routes')}>
        <div className="space-y-3">
          {remove.isError && <ErrorState error={remove.error} compact />}
          <div className="max-w-[220px]">
            <SelectInput label={t('admin.models.filterAlias')} value={aliasFilter} onChange={setAliasFilter} options={aliasOptions} />
          </div>
          <AdminQueryGate pending={query.isPending} error={query.error} onRetry={() => void query.refetch()}>
            {routes.length === 0 ? (
              <EmptyState title={t('admin.models.empty')} />
            ) : (
              <TableWrap>
                <Table>
                  <THead>
                    <TR>
                      <TH>{t('admin.models.routeId')}</TH>
                      <TH>{t('admin.models.aliasPublicId')}</TH>
                      <TH>{t('admin.models.providerModelId')}</TH>
                      <TH>{t('admin.models.weight')}</TH>
                      <TH>{t('admin.models.priority')}</TH>
                      <TH>{t('admin.models.health')}</TH>
                      <TH>{t('admin.models.circuit')}</TH>
                      <TH>{t('admin.models.lastErrorAt')}</TH>
                      <TH>{t('admin.models.circuitOpenUntil')}</TH>
                      <TH>{t('label.actions')}</TH>
                    </TR>
                  </THead>
                  <TBody>
                    {routes.map((route) => (
                      <Fragment key={route.route_id}>
                        <TR>
                          <TD className="font-mono text-[11.5px]">{route.route_id}</TD>
                          <TD className="font-mono text-[11.5px]">{route.alias_public_id}</TD>
                          <TD className="font-mono text-[11.5px]">{route.provider_model_id}</TD>
                          <TD>{format.number(route.weight)}</TD>
                          <TD>{format.number(route.priority)}</TD>
                          <TD>
                            <Badge tone={statusTone(route.health)}>{route.health}</Badge>
                          </TD>
                          <TD>
                            <Badge tone={statusTone(route.circuit_state)}>{route.circuit_state}</Badge>
                          </TD>
                          <TD className="whitespace-nowrap text-[11.5px] text-fg-3">{format.dateTime(route.last_error_at)}</TD>
                          <TD className="whitespace-nowrap text-[11.5px] text-fg-3">
                            {format.dateTime(route.circuit_open_until)}
                          </TD>
                          <TD>
                            <div className="flex items-center gap-1.5">
                              <Button
                                size="sm"
                                onClick={() => setEditing(editing === route.route_id ? null : route.route_id)}
                              >
                                {t('action.edit')}
                              </Button>
                              <Button
                                size="sm"
                                variant="danger"
                                disabled={remove.isPending && remove.variables === route.route_id}
                                onClick={() => {
                                  setConfirmRoute(route)
                                }}
                              >
                                {t('action.delete')}
                              </Button>
                            </div>
                          </TD>
                        </TR>
                        {editing === route.route_id && (
                          <TR>
                            <TD colSpan={10} className="bg-panel">
                              <RouteEditRow
                                route={route}
                                onDone={() => {
                                  toast.success(t('admin.models.updated'))
                                  setEditing(null)
                                }}
                              />
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
      <Card title={t('admin.models.createRoute')}>
        <form className="grid gap-3 sm:grid-cols-2" onSubmit={submit}>
          <TextInput label={t('admin.models.routeId')} value={routeId} onChange={setRouteId} />
          <TextInput label={t('admin.models.aliasPublicId')} value={aliasPublicId} onChange={setAliasPublicId} />
          <TextInput label={t('admin.models.providerModelId')} value={providerModelId} onChange={setProviderModelId} />
          <TextInput type="number" label={t('admin.models.weight')} value={weight} onChange={setWeight} />
          <TextInput type="number" label={t('admin.models.priority')} value={priority} onChange={setPriority} />
          {formError && <p className="text-[11.5px] text-danger sm:col-span-2">{formError}</p>}
          {create.isError && (
            <div className="sm:col-span-2">
              <ErrorState error={create.error} compact />
            </div>
          )}
          <div className="sm:col-span-2">
            <Button
              type="submit"
              disabled={
                routeId.trim() === '' ||
                aliasPublicId.trim() === '' ||
                providerModelId.trim() === '' ||
                create.isPending
              }
            >
              {t('action.create')}
            </Button>
          </div>
        </form>
      </Card>
      <ConfirmDialog
        open={confirmRoute !== null}
        title={t('admin.models.deleteRoute')}
        description={t('admin.models.deleteConfirm', { name: confirmRoute?.route_id ?? '' })}
        confirmLabel={t('action.delete')}
        pending={remove.isPending}
        onConfirm={() => {
          if (confirmRoute !== null) remove.mutate(confirmRoute.route_id)
        }}
        onCancel={() => setConfirmRoute(null)}
      />
    </>
  )
}

function RouteEditRow({ route, onDone }: { route: ModelRoute; onDone: () => void }) {
  const { t } = useI18n()
  const invalidate = useAdminInvalidate()
  const [aliasPublicId, setAliasPublicId] = useState(route.alias_public_id)
  const [providerModelId, setProviderModelId] = useState(route.provider_model_id)
  const [weight, setWeight] = useState(String(route.weight))
  const [priority, setPriority] = useState(String(route.priority))
  const [formError, setFormError] = useState<string | null>(null)

  const patch = useMutation({
    mutationFn: (numbers: { weight: number | undefined; priority: number | undefined }) =>
      patchRoute(route.route_id, {
        route_id: route.route_id,
        alias_public_id: aliasPublicId.trim(),
        provider_model_id: providerModelId.trim(),
        weight: numbers.weight,
        priority: numbers.priority,
      }),
    onSuccess: () => {
      invalidate(adminKeys.routes)
      onDone()
    },
  })

  return (
    <form
      className="grid gap-3 py-1 sm:grid-cols-2"
      onSubmit={(event) => {
        event.preventDefault()
        const weightValue = parseOptionalNumber(weight)
        const priorityValue = parseOptionalNumber(priority)
        if (weightValue === null || priorityValue === null) {
          setFormError(t('error.bad_request'))
          return
        }
        setFormError(null)
        patch.mutate({ weight: weightValue, priority: priorityValue })
      }}
    >
      <TextInput label={t('admin.models.aliasPublicId')} value={aliasPublicId} onChange={setAliasPublicId} />
      <TextInput label={t('admin.models.providerModelId')} value={providerModelId} onChange={setProviderModelId} />
      <TextInput type="number" label={t('admin.models.weight')} value={weight} onChange={setWeight} />
      <TextInput type="number" label={t('admin.models.priority')} value={priority} onChange={setPriority} />
      {formError && <p className="text-[11.5px] text-danger sm:col-span-2">{formError}</p>}
      {patch.isError && (
        <div className="sm:col-span-2">
          <ErrorState error={patch.error} compact />
        </div>
      )}
      <div className="flex items-center gap-1.5 sm:col-span-2">
        <Button
          type="submit"
          size="sm"
          disabled={
            patch.isPending || aliasPublicId.trim() === '' || providerModelId.trim() === ''
          }
        >
          {t('action.save')}
        </Button>
        <Button size="sm" variant="ghost" onClick={onDone}>
          {t('action.cancel')}
        </Button>
      </div>
    </form>
  )
}

/**
 * Provider connectivity test dialog. `POST /admin/providers/{id}/test` always
 * answers 200 with an outcome object; only metadata is shown — never the API
 * key or the upstream response body.
 */
function ProviderTestDialog({ provider, onClose }: { provider: Provider; onClose: () => void }) {
  const { t, locale, format } = useI18n()
  const test = useMutation({ mutationFn: () => testAdminProvider(provider.provider_id) })

  useEffect(() => {
    test.mutate()
    // Runs once per mount; the dialog is keyed by provider id.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const result = test.data
  const codeKey = result?.code ? TEST_CODE_KEYS[result.code] : undefined
  const localized = codeKey !== undefined && hasMessage(locale, codeKey)
  const codeText = localized ? t(codeKey) : (result?.code ?? '—')
  const messageText = !localized && result?.message ? result.message : undefined

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
        aria-label={t('admin.models.providerTest')}
        className="w-full max-w-[440px] rounded-xl border border-line bg-surface p-4 shadow-xl"
      >
        <h2 className="text-[13.5px] font-semibold text-fg">
          {t('admin.models.providerTest')} ·{' '}
          <span className="font-mono text-[11px] text-fg-3">{provider.provider_id}</span>
        </h2>
        <div className="mt-3 space-y-2">
          {test.isPending && <p className="text-[12px] text-fg-2">{t('admin.models.test.running')}</p>}
          {test.error != null && <ErrorState error={test.error} compact />}
          {result && (
            <>
              <Badge tone={result.ok ? 'accent' : 'warn'}>
                {result.ok ? t('admin.models.test.ok') : t('admin.models.test.failed')}
              </Badge>
              <KeyValue
                items={[
                  [t('admin.models.test.code'), codeText],
                  [
                    t('admin.models.test.latency'),
                    result.latency_ms === undefined ? '—' : `${format.number(result.latency_ms)} ms`,
                  ],
                  [
                    t('admin.models.test.statusCode'),
                    result.status_code === undefined ? '—' : format.number(result.status_code),
                  ],
                ]}
              />
              {messageText && <p className="text-[11.5px] leading-5 text-fg-3">{messageText}</p>}
            </>
          )}
          <p className="text-[11px] leading-5 text-fg-3">{t('admin.models.test.privacy')}</p>
        </div>
        <div className="mt-4 flex justify-end gap-2">
          <Button size="sm" variant="ghost" disabled={test.isPending} onClick={() => test.mutate()}>
            {t('admin.models.test.retest')}
          </Button>
          <Button size="sm" onClick={onClose}>
            {t('action.close')}
          </Button>
        </div>
      </div>
    </div>
  )
}
