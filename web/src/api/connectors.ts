/**
 * Connector data access (`/api/v1/connectors/...`, `/api/v1/external-refs`).
 *
 * All endpoints need the admin scope (session users must be owner/admin) and
 * are behind the deployment feature gate `features.connectors`. Records mirror
 * the backend shapes; `config` arrives redacted (`***` for secret-looking keys)
 * and `stats_json`/`error` are decoded by the panel. Writes go through `api()`
 * and therefore carry the session CSRF header.
 */
import { api, encodePathSegment, queryString } from './client'
import { asJsonObject } from '../lib/json'

export interface Connector {
  connector_id: string
  space_id: string
  kind: string
  name: string
  /** Redacted server-side; secret-looking keys arrive as "***". */
  config: unknown
  secret_env: string | null
  secret_configured: boolean
  status: string
  last_sync_at: string | null
  created_at: string
  updated_at: string
}

export const CONNECTOR_KINDS = [
  'filesystem_export',
  'jsonl',
  'http_json',
  'feishu',
  'sharepoint',
  'onedrive',
  'confluence',
  'mediawiki',
] as const

export type ConnectorKind = (typeof CONNECTOR_KINDS)[number]

/** Kinds with a real implementation; every other kind answers connector_not_configured. */
export const REAL_CONNECTOR_KINDS: readonly ConnectorKind[] = ['filesystem_export', 'jsonl', 'http_json']

export interface ConnectorTestResult {
  connector_id: string
  ok: boolean
  status: string
  details?: unknown
  code?: string
  message?: string
}

export interface ConnectorJob {
  job_id: string
  connector_id: string
  status: string
  started_at: string
  finished_at: string | null
  stats_json: string
  error: string | null
}

export interface ConnectorJobStats {
  items: number
  indexed: number
  refs_only: number
  failed: number
  errors: { code?: string; message?: string; external_id?: string }[]
}

export interface ExternalRef {
  tenant_id: string
  space_id: string
  doc_id: string
  provider: string
  external_id: string
  url: string
  revision: string | null
  acl_json: string
  fetched_at: string
  status: string
}

/* ------------------------------------------------------------------- reads */

export function listConnectors(): Promise<{ connectors: Connector[] }> {
  return api('/api/v1/connectors')
}

export function listConnectorJobs(connectorId: string, limit = 50): Promise<{ jobs: ConnectorJob[] }> {
  return api(`/api/v1/connectors/${encodePathSegment(connectorId)}/jobs${queryString({ limit })}`)
}

export function listExternalRefs(params: {
  doc_id?: string
  provider?: string
  limit?: number
}): Promise<{ external_refs: ExternalRef[] }> {
  return api(
    `/api/v1/external-refs${queryString({ doc_id: params.doc_id, provider: params.provider, limit: params.limit })}`,
  )
}

/* ------------------------------------------------------------------ writes */

export interface ConnectorCreateInput {
  space_id: string
  kind: string
  name: string
  /** Validated JSON text; omitted when empty so the backend defaults to `{}`. */
  config_json?: string
  secret_env?: string
}

export function createConnector(input: ConnectorCreateInput): Promise<{ connector: Connector }> {
  return api('/api/v1/connectors', { method: 'POST', body: input })
}

export interface ConnectorPatchInput {
  name?: string
  space_id?: string
  status?: string
  secret_env?: string | null
  config_json?: string
}

export function patchConnector(connectorId: string, input: ConnectorPatchInput): Promise<{ connector: Connector }> {
  return api(`/api/v1/connectors/${encodePathSegment(connectorId)}`, { method: 'PATCH', body: input })
}

export function deleteConnector(connectorId: string): Promise<{ connector_id: string; removed: boolean }> {
  return api(`/api/v1/connectors/${encodePathSegment(connectorId)}`, { method: 'DELETE' })
}

export function testConnector(connectorId: string): Promise<ConnectorTestResult> {
  return api(`/api/v1/connectors/${encodePathSegment(connectorId)}/test`, { method: 'POST' })
}

export function syncConnector(connectorId: string): Promise<{ job_id: string; status: string }> {
  return api(`/api/v1/connectors/${encodePathSegment(connectorId)}/sync`, { method: 'POST' })
}

/* ----------------------------------------------------------------- decoding */

function statNumber(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : 0
}

/** `stats_json` is a JSON string on the wire; unparsable stats render as `null`. */
export function decodeJobStats(statsJson: string): ConnectorJobStats | null {
  try {
    const parsed = asJsonObject(JSON.parse(statsJson))
    if (!parsed) return null
    const errors = Array.isArray(parsed.errors)
      ? parsed.errors.map((item) => {
          const record = asJsonObject(item)
          if (!record) return {}
          return {
            code: typeof record.code === 'string' ? record.code : undefined,
            message: typeof record.message === 'string' ? record.message : undefined,
            external_id: typeof record.external_id === 'string' ? record.external_id : undefined,
          }
        })
      : []
    return {
      items: statNumber(parsed.items),
      indexed: statNumber(parsed.indexed),
      refs_only: statNumber(parsed.refs_only),
      failed: statNumber(parsed.failed),
      errors,
    }
  } catch {
    return null
  }
}

export function isStubConnectorKind(kind: string): boolean {
  return (CONNECTOR_KINDS as readonly string[]).includes(kind) && !(REAL_CONNECTOR_KINDS as readonly string[]).includes(kind)
}
