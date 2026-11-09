/** Wire types — field names match the JSON produced by `src/web.rs` exactly. */

export interface ErrorBody {
  error: string
  message: string
}

/* ---------------------------------------------------------------- governance */

export type SpaceRole = 'viewer' | 'editor' | 'space_admin' | 'space_owner'
export type GroupRole = 'group_member' | 'group_admin'
export type PrincipalType = 'user' | 'group' | 'agent'
export type SpaceStatus = 'active' | 'archived' | 'deleted'
export type Visibility = 'private' | 'internal'
export type Enforcement = 'hard' | 'soft'
export type QueryMode = 'search' | 'answer' | 'both'

export interface Space {
  space_id: string
  tenant_id: string
  name: string
  visibility: string
  owner_type: string
  owner_id: string
  status: string
  role: SpaceRole | null
  created_at: string
  archived_at: string | null
  deleted_at: string | null
}

/** `GET /api/v1/admin/spaces` returns the raw store record. */
export interface SpaceRecord {
  tenant_id: string
  space_id: string
  name: string
  visibility: string
  default_acl_json: string
  owner_principal_id: string
  owner_type: string
  owner_id: string
  status: string
  settings: string
  archived_at: string | null
  deleted_at: string | null
  created_at: string
}

export interface SpaceMember {
  space_id: string
  principal_type: string
  principal_id: string
  role: string
  added_by: string
  created_at: string
}

export interface Group {
  group_id: string
  tenant_id: string
  name: string
  description: string
  created_by: string
  status: string
  created_at: string
  updated_at: string
}

export interface GroupMember {
  principal_id: string
  role: string
  added_by: string
  created_at: string
}

export interface StorageUsage {
  space_id: string
  source_bytes: number
  index_bytes: number
  history_bytes: number
  total_bytes: number
  reserved_bytes: number
  documents: number
  chunks: number
  members: number
  updated_at: string
}

export interface SpaceQuota {
  scope_type: string
  scope_id: string
  storage_bytes: number | null
  documents: number | null
  chunks: number | null
  members: number | null
  spaces: number | null
  enforcement: string
  updated_by: string
  updated_at: string
}

export interface AuditEvent {
  event_id: string
  tenant_id: string
  space_id: string | null
  actor_id: string
  actor_type: string
  action: string
  target_type: string
  target_id: string
  result: string
  status_code: number
  request_id: string | null
  ip: string | null
  user_agent: string | null
  detail: unknown
  created_at: string
}

export interface FeaturesResponse {
  mode: 'solo' | 'team' | 'enterprise_internal' | 'enterprise_external'
  features: {
    signup: boolean
    billing: boolean
    connectors: boolean
    sso: boolean
    multi_tenant: boolean
    public_api: boolean
    mcp: boolean
    audit: boolean
    rate_limit: boolean
    public_share: boolean
    /** Space/token quota management is part of the subscription tier. */
    quotas: boolean
  }
  i18n: { default_locale: string; supported_locales: string[] }
}

/* ---------------------------------------------------------------------- query */

export interface Degraded {
  code: string
  category: 'evidence' | 'quality' | 'security' | 'performance'
  capability: string
  expected_level: string
  actual_level: string
  reason: string
  impact: string
  fallback: string
}

export interface GovernanceFields {
  tenant_id: string
  space_id: string
  acl_json: string
  acl_principals: string
  lang: string
  valid_from: string
  valid_to: string | null
  superseded_by: string | null
  version: number
}

export type SearchMode = 'bm25' | 'bm25_judge' | 'event' | 'hybrid' | 'auto'

export interface SearchHit {
  chunk_id: string
  doc_id: string
  path: string
  title: string
  line_start: number
  line_end: number
  quote: string
  score: number
  governance: GovernanceFields
  event_id: string | null
  exact_fallback: boolean
  rerank_score: number | null
}

export interface SearchResponse {
  query: string
  mode_requested: SearchMode
  mode_used: SearchMode
  lang: string | null
  keyword_engine: string
  as_of: string
  hits: SearchHit[]
  degraded: Degraded[]
}

export interface Citation {
  doc_id: string
  chunk_id: string
  path: string
  line_start: number
  line_end: number
  quote: string
  source_url?: string
}

export interface AnswerUsage {
  input_tokens: number
  output_tokens: number
  cached_tokens: number
  token_source: string
  cost_micros: number
  currency: string
}

export interface Answer {
  answer: string | null
  citations: Citation[]
  confidence: { score: number; band: 'high' | 'medium' | 'low'; basis: string[] }
  conflicts: {
    conflict_id: string
    topic: string
    candidates: string[]
    reason: string
    resolution: string
    preferred_doc_id?: string
  }[]
  degraded: Degraded[]
  knowledge_as_of: string | null
  mode: 'extractive' | 'generative' | 'hybrid'
  model?: string
  usage?: AnswerUsage
  cache_hit: boolean
  conversation_id?: string
  message_id?: string
  summary_kind?: string
  summary_source?: string
}

export interface QueryRequest {
  query: string
  space?: string
  lang?: string
  conversation_id?: string
  mode?: QueryMode
}

export interface QueryResponse {
  conversation_id: string
  message_id: string
  user_message_id: string
  mode: QueryMode
  rewritten_query: string
  conversation_context_hash: string
  degraded: { code: string }[]
  results: SearchResponse | null
  answer: Answer | null
}

/* -------------------------------------------------------------- conversation */

export interface Conversation {
  conversation_id: string
  tenant_id: string
  space_id: string
  owner_id: string
  title: string
  external_session_id: string | null
  status: 'active' | 'deleted'
  created_at: string
  updated_at: string
}

export interface ConversationMessage {
  message_id: string
  conversation_id: string
  role: 'user' | 'assistant' | string
  content: string
  citations_json: string
  model: string | null
  usage_json: string | null
  degraded_json: string | null
  request_id: string
  mode: QueryMode | string
  rewritten_query: string | null
  conversation_context_hash: string | null
  created_at: string
}

/* -------------------------------------------------------------------- summary */

export interface SummaryPayload {
  tldr?: string
  bullets?: string[]
  keywords?: string[]
  entities?: { name?: string; type?: string }[]
  lang?: string
  source_revision?: string
  fallback?: boolean
  [key: string]: unknown
}

export interface DocumentSummary {
  doc_id: string
  revision_id: string
  lang: string
  model: string
  prompt_version: string
  content_hash: string
  status: 'ready' | 'failed'
  summary: SummaryPayload | null
  usage_event_id: string | null
  generated_at: string | null
  error: string | null
}

export type PageSummaryResponse =
  | { space_id: string; slug: string; source_revision: string; cache_hit: boolean; summary: DocumentSummary }
  | { space_id: string; slug: string; source_revision: string; status: 'pending'; summary: null; degraded: Degraded[] }

/* ---------------------------------------------------------------------- admin */

export interface Provider {
  provider_id: string
  kind: string
  base_url: string
  key_configured: boolean
  status: string
  priority: number
  weight: number
  limits_json: string
  timeouts_json: string
  created_at: string
  updated_at: string
}

export interface ProviderModel {
  provider_model_id: string
  provider_id: string
  upstream_model: string
  context_window: number
  pricing_json: string
  capabilities_json: string
  status: string
}

export interface ModelAlias {
  public_id: string
  display_name: string
  capabilities_json: string
  status: string
  access_policy_json: string
  price_json: string
  version: number
  created_at: string
  updated_at: string
}

export interface ModelRoute {
  route_id: string
  alias_public_id: string
  provider_model_id: string
  weight: number
  priority: number
  health: string
  circuit_state: string
  last_error_at: string | null
  circuit_open_until: string | null
}

export interface TokenQuota {
  quota_id: string
  scope_type: string
  scope_id: string
  period: string
  limit_tokens: number | null
  limit_requests: number | null
  limit_cost_micros: number | null
  currency: string
  enforcement: string
  action: string
  enabled: boolean
  updated_at: string
}

export interface QuotaCounter {
  tenant_id: string
  scope_type: string
  scope_id: string
  period_start: string
  reserved_tokens: number
  used_tokens: number
  reserved_requests: number
  used_requests: number
  reserved_cost_micros: number
  used_cost_micros: number
  updated_at: string
}

export interface UsageEvent {
  usage_event_id: string
  request_id: string
  tenant_id: string
  space_id: string | null
  user_id: string
  role: string
  source: string
  alias: string
  provider_model_id: string
  input_tokens: number
  output_tokens: number
  cached_tokens: number
  token_source: string
  cost_micros: number
  currency: string
  provider_cost_micros: number
  latency_ms: number
  status: string
  cache_hit: boolean
  at: string
}

export interface UsageAggregate {
  requests: number
  input_tokens: number
  output_tokens: number
  cached_tokens: number
  cost_micros: number
  provider_cost_micros: number
  errors: number
}

export interface AdminUser {
  user_id: string
  username: string
  display_name: string
  role: 'owner' | 'admin' | 'user'
  disabled: boolean
  created_at: string
  email: string | null
  source: string
  last_login_at: string | null
  last_sso_at: string | null
}

export interface AdminPlan {
  plan_id: string
  name: string
  price_micros: number
  included_tokens: number
  currency: string
  period: string
  status: string
  features: unknown
  created_at: string
  updated_at: string
}

export interface AdminOverview {
  counts: { users: number; spaces: number; documents: number; chunks: number }
  last_24h: { requests: number; errors: number; cost_micros: number; cache_hit_rate: number }
  recent_audit: AuditEvent[]
}

export interface AdminRole {
  role: string
  description: string
}

export const SPACE_ROLE_RANK: Record<SpaceRole, number> = {
  viewer: 1,
  editor: 2,
  space_admin: 3,
  space_owner: 4,
}

export function hasSpaceRole(role: string | null | undefined, minimum: SpaceRole): boolean {
  if (!role) return false
  const value = SPACE_ROLE_RANK[role as SpaceRole]
  if (value === undefined) return false
  return value >= SPACE_ROLE_RANK[minimum]
}
