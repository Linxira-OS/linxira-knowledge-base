import { Fragment, useState, type FormEvent } from 'react'
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
  patchRoute,
} from '../../api/admin'
import type { ModelAlias, ModelRoute } from '../../api/types'
import { Badge, Banner, Button, Card, EmptyState, ErrorState, Table, TableWrap, TBody, TD, TH, THead, TR, statusTone } from '../../components/ui'
import { useI18n } from '../../lib/i18n'
import { AdminQueryGate, JsonInput, SelectInput, TextInput, adminKeys, parseOptionalNumber, useAdminInvalidate, useJsonDraft } from './shared'

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
  const [notice, setNotice] = useState<string | null>(null)
  const [formError, setFormError] = useState<string | null>(null)
  const [providerId, setProviderId] = useState('')
  const [kind, setKind] = useState('')
  const [baseUrl, setBaseUrl] = useState('')
  const [apiKeyEnv, setApiKeyEnv] = useState('')
  const [status, setStatus] = useState('')
  const [priority, setPriority] = useState('')
  const [weight, setWeight] = useState('')
  const json = useJsonDraft(['limits_json', 'timeouts_json'])

  const query = useQuery({ queryKey: adminKeys.providers, queryFn: listProviders })
  const create = useMutation({
    mutationFn: createProvider,
    onSuccess: () => {
      setNotice(t('admin.models.created'))
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
      setNotice(t('admin.models.deleted'))
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
          {notice && <Banner tone="accent">{notice}</Banner>}
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
                      <TR key={provider.provider_id}>
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
                          <Button
                            size="sm"
                            variant="danger"
                            disabled={remove.isPending && remove.variables === provider.provider_id}
                            onClick={() => {
                              setNotice(null)
                              remove.mutate(provider.provider_id)
                            }}
                          >
                            {t('action.delete')}
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
    </>
  )
}

function ProviderModelsCard() {
  const { t, format } = useI18n()
  const invalidate = useAdminInvalidate()
  const [notice, setNotice] = useState<string | null>(null)
  const [formError, setFormError] = useState<string | null>(null)
  const [modelId, setModelId] = useState('')
  const [providerId, setProviderId] = useState('')
  const [upstreamModel, setUpstreamModel] = useState('')
  const [contextWindow, setContextWindow] = useState('')
  const json = useJsonDraft(['pricing_json', 'capabilities_json'])

  const query = useQuery({ queryKey: adminKeys.providerModels, queryFn: listProviderModels })
  const create = useMutation({
    mutationFn: createProviderModel,
    onSuccess: () => {
      setNotice(t('admin.models.created'))
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
          {notice && <Banner tone="accent">{notice}</Banner>}
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
  const [notice, setNotice] = useState<string | null>(null)
  const [editing, setEditing] = useState<string | null>(null)
  const [publicId, setPublicId] = useState('')
  const [displayName, setDisplayName] = useState('')
  const [status, setStatus] = useState('')
  const json = useJsonDraft(['capabilities_json', 'access_policy_json', 'price_json'])

  const query = useQuery({ queryKey: adminKeys.aliases, queryFn: listAliases })
  const create = useMutation({
    mutationFn: createAlias,
    onSuccess: () => {
      setNotice(t('admin.models.created'))
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
      setNotice(t('admin.models.deleted'))
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
          {notice && <Banner tone="accent">{notice}</Banner>}
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
                                  setNotice(null)
                                  remove.mutate(alias.public_id)
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
                                  setNotice(t('admin.models.updated'))
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
  const [notice, setNotice] = useState<string | null>(null)
  const [formError, setFormError] = useState<string | null>(null)
  const [editing, setEditing] = useState<string | null>(null)
  const [aliasFilter, setAliasFilter] = useState('')
  const [routeId, setRouteId] = useState('')
  const [aliasPublicId, setAliasPublicId] = useState('')
  const [providerModelId, setProviderModelId] = useState('')
  const [weight, setWeight] = useState('')
  const [priority, setPriority] = useState('')

  const aliasesQuery = useQuery({ queryKey: adminKeys.aliases, queryFn: listAliases })
  const query = useQuery({
    queryKey: [...adminKeys.routes, aliasFilter],
    queryFn: () => listRoutes(aliasFilter || undefined),
    placeholderData: keepPreviousData,
  })
  const create = useMutation({
    mutationFn: createRoute,
    onSuccess: () => {
      setNotice(t('admin.models.created'))
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
      setNotice(t('admin.models.deleted'))
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
          {notice && <Banner tone="accent">{notice}</Banner>}
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
                                  setNotice(null)
                                  remove.mutate(route.route_id)
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
                                  setNotice(t('admin.models.updated'))
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
