import { api, queryString } from './client'
import type {
  AdminRole,
  AdminUser,
  AuditEvent,
  Group,
  ModelAlias,
  ModelRoute,
  Provider,
  ProviderModel,
  QuotaCounter,
  SpaceRecord,
  TokenQuota,
  UsageAggregate,
  UsageEvent,
} from './types'

/* ----------------------------------------------------------------- providers */

export function listProviders(): Promise<{ providers: Provider[]; models: ProviderModel[] }> {
  return api('/api/v1/admin/providers')
}

export interface ProviderBody {
  provider_id: string
  kind: string
  base_url: string
  api_key_env?: string | null
  status?: string
  priority?: number
  weight?: number
  limits_json?: string
  timeouts_json?: string
}

export function createProvider(body: ProviderBody): Promise<{ provider: Provider }> {
  return api('/api/v1/admin/providers', { method: 'POST', body })
}

export function patchProvider(providerId: string, body: ProviderBody): Promise<{ provider: Provider }> {
  return api(`/api/v1/admin/providers/${encodeURIComponent(providerId)}`, { method: 'PATCH', body })
}

export function deleteProvider(providerId: string): Promise<{ removed: boolean }> {
  return api(`/api/v1/admin/providers/${encodeURIComponent(providerId)}`, { method: 'DELETE' })
}

export function listProviderModels(): Promise<{ models: ProviderModel[] }> {
  return api('/api/v1/admin/provider-models')
}

export interface ProviderModelBody {
  provider_model_id: string
  provider_id: string
  upstream_model: string
  context_window?: number
  pricing_json?: string
  capabilities_json?: string
}

export function createProviderModel(body: ProviderModelBody): Promise<{ model: ProviderModel }> {
  return api('/api/v1/admin/provider-models', { method: 'POST', body })
}

/* -------------------------------------------------------------------- aliases */

export function listAliases(): Promise<{ aliases: ModelAlias[] }> {
  return api('/api/v1/admin/aliases')
}

export interface AliasBody {
  public_id: string
  display_name?: string | null
  capabilities_json?: string
  status?: string
  access_policy_json?: string
  price_json?: string
}

export function createAlias(body: AliasBody): Promise<{ alias: ModelAlias }> {
  return api('/api/v1/admin/aliases', { method: 'POST', body })
}

export function patchAlias(publicId: string, body: AliasBody): Promise<{ alias: ModelAlias }> {
  return api(`/api/v1/admin/aliases/${encodeURIComponent(publicId)}`, { method: 'PATCH', body })
}

export function deleteAlias(publicId: string): Promise<{ removed: boolean }> {
  return api(`/api/v1/admin/aliases/${encodeURIComponent(publicId)}`, { method: 'DELETE' })
}

/* --------------------------------------------------------------------- routes */

export function listRoutes(alias?: string): Promise<{ routes: ModelRoute[] }> {
  return api(`/api/v1/admin/routes${queryString({ alias })}`)
}

export interface RouteBody {
  route_id: string
  alias_public_id: string
  provider_model_id: string
  weight?: number
  priority?: number
}

export function createRoute(body: RouteBody): Promise<{ route: ModelRoute }> {
  return api('/api/v1/admin/routes', { method: 'POST', body })
}

export function patchRoute(routeId: string, body: RouteBody): Promise<{ route: ModelRoute }> {
  return api(`/api/v1/admin/routes/${encodeURIComponent(routeId)}`, { method: 'PATCH', body })
}

export function deleteRoute(routeId: string): Promise<{ removed: boolean }> {
  return api(`/api/v1/admin/routes/${encodeURIComponent(routeId)}`, { method: 'DELETE' })
}

/* ------------------------------------------------------------- token quotas */

export function listTokenQuotas(): Promise<{ quotas: TokenQuota[]; counters: QuotaCounter[] }> {
  return api('/api/v1/admin/quotas')
}

export interface TokenQuotaBody {
  quota_id?: string
  scope_type: string
  scope_id: string
  period?: string
  limit_tokens?: number | null
  limit_requests?: number | null
  limit_cost_micros?: number | null
  currency?: string
  enforcement?: string
  action?: string
  enabled?: boolean
}

export function createTokenQuota(body: TokenQuotaBody): Promise<{ quota: TokenQuota }> {
  return api('/api/v1/admin/quotas', { method: 'POST', body })
}

export function deleteTokenQuota(quotaId: string): Promise<{ removed: boolean }> {
  return api(`/api/v1/admin/quotas/${encodeURIComponent(quotaId)}`, { method: 'DELETE' })
}

export interface QuotaOverrideBody {
  scope_type: 'tenant' | 'group' | 'space' | 'user'
  scope_id: string
  dimension: string
  limit_value: number
  reason: string
  expires_at?: string | null
}

export function createQuotaOverride(body: QuotaOverrideBody): Promise<{
  override_id: string
  scope_type: string
  scope_id: string
  dimension: string
  limit_value: number
  expires_at: string | null
  request_id: string
}> {
  return api('/api/v1/admin/quota-overrides', { method: 'POST', body })
}

/* ----------------------------------------------------------------------- usage */

export type UsageQuery = {
  space?: string
  user?: string
  alias?: string
  from?: string
  to?: string
  limit?: number
}

export function listUsage(params: UsageQuery): Promise<{ totals: UsageAggregate; events: UsageEvent[] }> {
  return api(`/api/v1/admin/usage${queryString(params)}`)
}

export type UsageAggregateQuery = UsageQuery & {
  group_by?: 'user' | 'space' | 'alias' | 'period' | 'source'
  offset?: number
}

export interface UsageAggregateResponse {
  group_by: string
  totals: UsageAggregate
  groups: { key: string; requests: number; input_tokens: number; output_tokens: number; cost_micros: number }[]
  total: number
  offset: number
  limit: number
}

export function aggregateUsage(params: UsageAggregateQuery): Promise<UsageAggregateResponse> {
  return api(`/api/v1/admin/usage/aggregate${queryString(params)}`)
}

/* ----------------------------------------------------------------------- users */

export function listAdminUsers(params: { offset?: number; limit?: number }): Promise<{
  users: AdminUser[]
  total: number
  offset: number
  limit: number
}> {
  return api(`/api/v1/admin/users${queryString(params)}`)
}

export function patchAdminUser(userId: string, body: { role?: string; disabled?: boolean }): Promise<{
  user_id: string
  updated: boolean
}> {
  return api(`/api/v1/admin/users/${encodeURIComponent(userId)}`, { method: 'PATCH', body })
}

export function disableAdminUser(userId: string): Promise<{ user_id: string; disabled: boolean }> {
  return api(`/api/v1/admin/users/${encodeURIComponent(userId)}/disable`, { method: 'POST' })
}

export function listAdminRoles(): Promise<{ roles: AdminRole[] }> {
  return api('/api/v1/admin/roles')
}

export function listAdminGroups(): Promise<{ groups: Group[] }> {
  return api('/api/v1/admin/groups')
}

export function listAdminSpaces(): Promise<{ spaces: SpaceRecord[] }> {
  return api('/api/v1/admin/spaces')
}

/* ----------------------------------------------------------------------- audit */

export function listAudit(params: {
  actor?: string
  action?: string
  space?: string
  from?: string
  to?: string
  limit?: number
}): Promise<{ events: AuditEvent[] }> {
  return api(`/api/v1/admin/audit${queryString(params)}`)
}
