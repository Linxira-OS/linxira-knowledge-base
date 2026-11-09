// SPDX-License-Identifier: AGPL-3.0-only
use crate::config::{Backend, Config, ProfileName};
use crate::error::StoreError;
use crate::governance::{
    Acl, GovernanceFields, GroupRole, Policy, PrincipalKind, PrincipalType, SpaceRole, SqlDialect,
    TokenScope, UserRole,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub mod postgres;
pub mod sqlite;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceRecord {
    pub tenant_id: String,
    pub space_id: String,
    pub name: String,
    pub visibility: String,
    pub default_acl_json: String,
    pub owner_principal_id: String,
    pub owner_type: String,
    pub owner_id: String,
    pub status: String,
    pub settings: String,
    pub archived_at: Option<String>,
    pub deleted_at: Option<String>,
    pub created_at: String,
}

impl SpaceRecord {
    pub fn new(
        tenant_id: &str,
        space_id: &str,
        name: &str,
        acl: &Acl,
        owner: &str,
        created_at: &str,
    ) -> Self {
        Self {
            tenant_id: tenant_id.to_string(),
            space_id: space_id.to_string(),
            name: name.to_string(),
            visibility: "internal".to_string(),
            default_acl_json: acl.to_json(),
            owner_principal_id: owner.to_string(),
            owner_type: "user".to_string(),
            owner_id: owner.to_string(),
            status: "active".to_string(),
            settings: "{}".to_string(),
            archived_at: None,
            deleted_at: None,
            created_at: created_at.to_string(),
        }
    }

    pub fn with_owner(mut self, owner_type: &str, owner_id: &str) -> Self {
        self.owner_type = owner_type.to_string();
        self.owner_id = owner_id.to_string();
        if owner_type == "user" {
            self.owner_principal_id = owner_id.to_string();
        }
        self
    }

    pub fn default_acl(&self) -> Result<Acl, StoreError> {
        Acl::from_json(&self.default_acl_json)
    }

    pub fn is_writable(&self) -> bool {
        self.status == "active"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageRecord {
    pub tenant_id: String,
    pub space_id: String,
    pub slug: String,
    pub lang: String,
    pub title: String,
    pub summary: String,
    pub order_index: i32,
    pub current_revision_id: String,
    pub content_hash: String,
    pub acl_json: String,
    pub acl_principals: String,
    pub valid_from: String,
    pub valid_to: Option<String>,
    pub version: u32,
    pub superseded_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevisionRecord {
    pub tenant_id: String,
    pub revision_id: String,
    pub space_id: String,
    pub slug: String,
    pub lang: String,
    pub title: String,
    pub markdown: String,
    pub content_hash: String,
    pub author: String,
    pub created_at: String,
    pub version: u32,
    pub acl_json: String,
    pub acl_principals: String,
    pub valid_from: String,
    pub valid_to: Option<String>,
    pub superseded_by: Option<String>,
    pub source_url: Option<String>,
    pub source_revision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageSummary {
    pub tenant_id: String,
    pub space_id: String,
    pub slug: String,
    pub lang: String,
    pub title: String,
    pub summary: String,
    pub order_index: i32,
    pub version: u32,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageView {
    pub page: PageRecord,
    pub revision: RevisionRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevisionSummary {
    pub revision_id: String,
    pub version: u32,
    pub author: String,
    pub created_at: String,
    pub content_hash: String,
    pub valid_from: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentRecord {
    pub tenant_id: String,
    pub doc_id: String,
    pub space_id: String,
    pub logical_path: String,
    pub source_uri: String,
    pub title: String,
    pub mime: String,
    pub raw_hash: String,
    pub content_hash: String,
    pub content: String,
    pub acl_json: String,
    pub acl_principals: String,
    pub lang: String,
    pub valid_from: String,
    pub valid_to: Option<String>,
    pub version: u32,
    pub superseded_by: Option<String>,
    pub status: String,
    pub source_role: String,
    pub canonicalization_version: String,
    pub extractor_version: String,
    pub chunker_version: String,
    pub created_at: String,
    pub updated_at: String,
}

impl DocumentRecord {
    pub fn governance(&self) -> GovernanceFields {
        GovernanceFields {
            tenant_id: self.tenant_id.clone(),
            space_id: self.space_id.clone(),
            acl_json: self.acl_json.clone(),
            acl_principals: self.acl_principals.clone(),
            lang: self.lang.clone(),
            valid_from: self.valid_from.clone(),
            valid_to: self.valid_to.clone(),
            superseded_by: self.superseded_by.clone(),
            version: self.version,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkRecord {
    pub tenant_id: String,
    pub chunk_id: String,
    pub doc_id: String,
    pub space_id: String,
    pub seq: u32,
    pub text: String,
    pub line_start: i64,
    pub line_end: i64,
    pub content_hash: String,
    pub acl_json: String,
    pub acl_principals: String,
    pub lang: String,
    pub valid_from: String,
    pub valid_to: Option<String>,
    pub superseded_by: Option<String>,
    pub version: u32,
    pub embedding: Option<Vec<f32>>,
}

impl ChunkRecord {
    pub fn governance(&self) -> GovernanceFields {
        GovernanceFields {
            tenant_id: self.tenant_id.clone(),
            space_id: self.space_id.clone(),
            acl_json: self.acl_json.clone(),
            acl_principals: self.acl_principals.clone(),
            lang: self.lang.clone(),
            valid_from: self.valid_from.clone(),
            valid_to: self.valid_to.clone(),
            superseded_by: self.superseded_by.clone(),
            version: self.version,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub chunk_id: String,
    pub doc_id: String,
    pub path: String,
    pub title: String,
    pub line_start: i64,
    pub line_end: i64,
    pub quote: String,
    pub score: f64,
    pub governance: GovernanceFields,
    /// Present only on the optional event-entity channel.
    pub event_id: Option<String>,
    /// True when PostgreSQL could not produce FTS rows and exact substring
    /// matching was used. SQLite never sets this.
    pub exact_fallback: bool,
    /// Filled by the optional external judge. Membership never changes.
    pub rerank_score: Option<f64>,
}

impl SearchHit {
    pub fn key(&self) -> &str {
        &self.chunk_id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub chunk_id: String,
    pub doc_id: String,
    pub path: String,
    pub title: String,
    pub line_start: i64,
    pub line_end: i64,
    pub quote: String,
    pub embedding: Vec<f32>,
    pub governance: GovernanceFields,
}

impl Candidate {
    pub fn into_hit(self, score: f64) -> SearchHit {
        SearchHit {
            chunk_id: self.chunk_id,
            doc_id: self.doc_id,
            path: self.path,
            title: self.title,
            line_start: self.line_start,
            line_end: self.line_end,
            quote: self.quote,
            score,
            governance: self.governance,
            event_id: None,
            exact_fallback: false,
            rerank_score: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub event_id: String,
    pub tenant_id: String,
    pub space_id: String,
    pub doc_id: String,
    pub chunk_id: String,
    pub seq: u32,
    pub text: String,
    pub content_hash: String,
    pub extractor_version: String,
    pub extraction_model: String,
    pub acl_json: String,
    pub acl_principals: String,
    pub lang: String,
    pub valid_from: String,
    pub valid_to: Option<String>,
    pub version: u32,
    pub superseded_by: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRecord {
    pub entity_id: String,
    pub tenant_id: String,
    pub space_id: String,
    pub name: String,
    pub normalized_name: String,
    pub entity_type: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEntityRecord {
    pub event_id: String,
    pub entity_id: String,
    pub weight: f64,
    pub tenant_id: String,
    pub space_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventUpsertOutcome {
    Inserted,
    Skipped(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupRecord {
    pub group_id: String,
    pub tenant_id: String,
    pub name: String,
    pub description: String,
    pub created_by: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMemberRecord {
    pub group_id: String,
    pub principal_id: String,
    pub role: GroupRole,
    pub added_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMembership {
    pub group: GroupRecord,
    pub member: GroupMemberRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceMemberRecord {
    pub space_id: String,
    pub principal_type: PrincipalType,
    pub principal_id: String,
    pub role: SpaceRole,
    pub added_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaScope {
    Tenant,
    Group,
    Space,
    User,
}

impl QuotaScope {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "tenant" => Ok(QuotaScope::Tenant),
            "group" => Ok(QuotaScope::Group),
            "space" => Ok(QuotaScope::Space),
            "user" => Ok(QuotaScope::User),
            other => Err(format!(
                "unknown quota scope {other:?}; expected tenant, group, space or user"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            QuotaScope::Tenant => "tenant",
            QuotaScope::Group => "group",
            QuotaScope::Space => "space",
            QuotaScope::User => "user",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaRecord {
    pub tenant_id: String,
    pub scope_type: QuotaScope,
    pub scope_id: String,
    pub storage_bytes: Option<i64>,
    pub documents: Option<i64>,
    pub chunks: Option<i64>,
    pub members: Option<i64>,
    pub spaces: Option<i64>,
    pub enforcement: String,
    pub updated_by: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageUsage {
    pub tenant_id: String,
    pub space_id: String,
    pub source_bytes: i64,
    pub index_bytes: i64,
    pub history_bytes: i64,
    pub total_bytes: i64,
    pub reserved_bytes: i64,
    pub documents: i64,
    pub chunks: i64,
    pub members: i64,
    pub updated_at: String,
}

impl StorageUsage {
    pub fn zero(tenant_id: &str, space_id: &str, at: &str) -> Self {
        Self {
            tenant_id: tenant_id.to_string(),
            space_id: space_id.to_string(),
            source_bytes: 0,
            index_bytes: 0,
            history_bytes: 0,
            total_bytes: 0,
            reserved_bytes: 0,
            documents: 0,
            chunks: 0,
            members: 0,
            updated_at: at.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaOverrideRecord {
    pub override_id: String,
    pub tenant_id: String,
    pub scope_type: QuotaScope,
    pub scope_id: String,
    pub dimension: String,
    pub limit_value: i64,
    pub reason: String,
    pub actor: String,
    pub expires_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEventRecord {
    pub event_id: String,
    pub tenant_id: String,
    pub space_id: Option<String>,
    pub actor_id: String,
    pub actor_type: String,
    pub action: String,
    pub target_type: String,
    pub target_id: String,
    pub result: String,
    pub status_code: i64,
    pub request_id: Option<String>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub detail_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    pub tenant_id: String,
    pub actor_id: Option<String>,
    pub action: Option<String>,
    pub space_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRecord {
    pub provider_id: String,
    pub kind: String,
    pub base_url: String,
    pub api_key_env: Option<String>,
    pub status: String,
    pub priority: i32,
    pub weight: i32,
    pub limits_json: String,
    pub timeouts_json: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderModelRecord {
    pub provider_model_id: String,
    pub provider_id: String,
    pub upstream_model: String,
    pub context_window: i64,
    pub pricing_json: String,
    pub capabilities_json: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelAliasRecord {
    pub public_id: String,
    pub display_name: String,
    pub capabilities_json: String,
    pub status: String,
    pub access_policy_json: String,
    pub price_json: String,
    pub version: u32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteRecord {
    pub route_id: String,
    pub alias_public_id: String,
    pub provider_model_id: String,
    pub weight: i32,
    pub priority: i32,
    pub health: String,
    pub circuit_state: String,
    pub last_error_at: Option<String>,
    pub circuit_open_until: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageEventRecord {
    pub usage_event_id: String,
    pub request_id: String,
    pub tenant_id: String,
    pub space_id: Option<String>,
    pub user_id: String,
    pub role: String,
    pub source: String,
    pub external_usage_id: Option<String>,
    pub alias: String,
    pub provider_model_id: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_tokens: i64,
    pub token_source: String,
    pub cost_micros: i64,
    pub currency: String,
    pub provider_cost_micros: i64,
    pub latency_ms: i64,
    pub status: String,
    pub cache_hit: bool,
    pub rewritten_query: Option<String>,
    pub at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageAggregate {
    pub requests: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_tokens: i64,
    pub cost_micros: i64,
    pub provider_cost_micros: i64,
    pub errors: i64,
}

#[derive(Debug, Clone, Default)]
pub struct UsageFilter {
    pub tenant_id: String,
    pub space_id: Option<String>,
    pub user_id: Option<String>,
    pub alias: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenQuotaRecord {
    pub quota_id: String,
    pub tenant_id: String,
    pub scope_type: String,
    pub scope_id: String,
    pub period: String,
    pub limit_tokens: Option<i64>,
    pub limit_requests: Option<i64>,
    pub limit_cost_micros: Option<i64>,
    pub currency: String,
    pub enforcement: String,
    pub action: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaCounter {
    pub tenant_id: String,
    pub scope_type: String,
    pub scope_id: String,
    pub period_start: String,
    pub reserved_tokens: i64,
    pub used_tokens: i64,
    pub reserved_requests: i64,
    pub used_requests: i64,
    pub reserved_cost_micros: i64,
    pub used_cost_micros: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaReservation {
    pub reservation_id: String,
    pub request_id: String,
    pub tenant_id: String,
    pub scope_type: String,
    pub scope_id: String,
    pub period_start: String,
    pub reserved_tokens: i64,
    pub reserved_requests: i64,
    pub reserved_cost_micros: i64,
    pub status: String,
    pub expires_at: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSummaryRecord {
    pub tenant_id: String,
    pub space_id: String,
    pub doc_id: String,
    pub revision_id: String,
    pub lang: String,
    pub model: String,
    pub prompt_version: String,
    pub content_hash: String,
    pub status: String,
    pub summary_json: String,
    pub usage_event_id: Option<String>,
    pub generated_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryJobRecord {
    pub job_id: String,
    pub tenant_id: String,
    pub space_id: String,
    pub doc_id: String,
    pub revision_id: String,
    pub status: String,
    pub attempts: u32,
    pub scheduled_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationRecord {
    pub conversation_id: String,
    pub tenant_id: String,
    pub space_id: String,
    pub owner_id: String,
    pub title: String,
    pub external_session_id: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRecord {
    pub message_id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub citations_json: String,
    pub model: Option<String>,
    pub usage_json: Option<String>,
    pub degraded_json: Option<String>,
    pub request_id: String,
    pub mode: String,
    pub rewritten_query: Option<String>,
    pub conversation_context_hash: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedMeta {
    pub tenant_id: String,
    pub space_id: String,
    pub seed_version: String,
    pub source_manifest_hash: String,
    pub applied_at: String,
    pub page_count: u32,
    pub revision_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    pub user_id: String,
    pub tenant_id: String,
    pub username: String,
    pub password_hash: String,
    pub display_name: String,
    pub created_at: String,
    pub disabled: bool,
    pub role: UserRole,
    pub email: Option<String>,
    pub external_id: Option<String>,
    pub source: String,
    pub last_sso_at: Option<String>,
    pub last_login_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub session_hash: String,
    pub tenant_id: String,
    pub user_id: String,
    pub created_at: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRecord {
    pub token_hash: String,
    pub token_id: String,
    pub tenant_id: String,
    pub user_id: String,
    pub scope: TokenScope,
    pub kind: PrincipalKind,
    pub label: String,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub revoked_at: Option<String>,
}

impl TokenRecord {
    pub fn is_active(&self, now: &str) -> bool {
        if self.revoked_at.is_some() {
            return false;
        }
        match &self.expires_at {
            Some(expires) => expires.as_str() >= now,
            None => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpsertOutcome {
    Imported,
    Skipped(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletRecord {
    pub wallet_id: String,
    pub tenant_id: String,
    pub currency: String,
    pub balance_micros: i64,
    pub status: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntryRecord {
    pub entry_id: String,
    pub group_id: String,
    pub wallet_id: String,
    pub tenant_id: String,
    pub account: String,
    pub direction: String,
    pub amount_micros: i64,
    pub currency: String,
    pub idempotency_key: String,
    pub request_id: Option<String>,
    pub metadata_json: String,
    pub created_at: String,
}

pub enum LedgerTxResult {
    Applied { balance_micros: i64 },
    Duplicate,
    Insufficient,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopupRecord {
    pub topup_id: String,
    pub wallet_id: String,
    pub tenant_id: String,
    pub amount_micros: i64,
    pub currency: String,
    pub channel: String,
    pub provider_ref: Option<String>,
    pub status: String,
    pub idempotency_key: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanRecord {
    pub plan_id: String,
    pub name: String,
    pub price_micros: i64,
    pub included_tokens: i64,
    pub currency: String,
    pub period: String,
    pub status: String,
    pub features_json: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionRecord {
    pub subscription_id: String,
    pub tenant_id: String,
    pub plan_id: String,
    pub status: String,
    pub trial_ends_at: Option<String>,
    pub current_period_start: Option<String>,
    pub current_period_end: Option<String>,
    pub past_due_at: Option<String>,
    pub suspended_at: Option<String>,
    pub cancelled_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceRecord {
    pub invoice_id: String,
    pub tenant_id: String,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub amount_micros: i64,
    pub currency: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentEventRecord {
    pub provider_event_id: String,
    pub provider: String,
    pub event_type: String,
    pub payload_json: String,
    pub status: String,
    pub received_at: String,
    pub processed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitlementRecord {
    pub tenant_id: String,
    pub plan_id: Option<String>,
    pub status: String,
    pub balance_micros: i64,
    pub included_tokens: i64,
    pub used_tokens: i64,
    pub currency: String,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorRecord {
    pub connector_id: String,
    pub tenant_id: String,
    pub space_id: String,
    pub kind: String,
    pub name: String,
    pub config_json: String,
    pub secret_env: Option<String>,
    pub status: String,
    pub last_sync_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalRefRecord {
    pub tenant_id: String,
    pub space_id: String,
    pub doc_id: String,
    pub provider: String,
    pub external_id: String,
    pub url: String,
    pub revision: Option<String>,
    pub acl_json: String,
    pub fetched_at: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorJobRecord {
    pub job_id: String,
    pub connector_id: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub stats_json: String,
    pub error: Option<String>,
}

/// Parameters for [`Store::search_events`].
#[derive(Debug, Clone)]
pub struct EventSearchParams<'a> {
    pub query: &'a str,
    pub depth: u32,
    pub max_seeds: i64,
    pub max_events: i64,
    pub text_weight: f64,
    pub limit: i64,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdminOverview {
    pub users: i64,
    pub spaces: i64,
    pub documents: i64,
    pub chunks: i64,
    pub requests_24h: i64,
    pub errors_24h: i64,
    pub cost_micros_24h: i64,
    pub cache_hits_24h: i64,
}

#[async_trait]
pub trait Store: Send + Sync {
    async fn connector_list(&self, tenant_id: &str) -> Result<Vec<ConnectorRecord>, StoreError>;
    async fn connector_get(
        &self,
        tenant_id: &str,
        connector_id: &str,
    ) -> Result<Option<ConnectorRecord>, StoreError>;
    async fn connector_upsert(&self, connector: &ConnectorRecord) -> Result<(), StoreError>;
    async fn connector_delete(
        &self,
        tenant_id: &str,
        connector_id: &str,
    ) -> Result<bool, StoreError>;
    async fn connector_mark_synced(
        &self,
        tenant_id: &str,
        connector_id: &str,
        at: &str,
    ) -> Result<(), StoreError>;
    async fn external_ref_upsert(&self, reference: &ExternalRefRecord) -> Result<(), StoreError>;
    async fn external_ref_list(
        &self,
        tenant_id: &str,
        doc_id: Option<&str>,
        provider: Option<&str>,
        limit: i64,
    ) -> Result<Vec<ExternalRefRecord>, StoreError>;
    async fn connector_job_create(&self, job: &ConnectorJobRecord) -> Result<(), StoreError>;
    async fn connector_job_update(
        &self,
        job_id: &str,
        status: &str,
        finished_at: Option<&str>,
        stats_json: &str,
        error: Option<&str>,
    ) -> Result<(), StoreError>;
    async fn connector_job_list(
        &self,
        connector_id: &str,
        limit: i64,
    ) -> Result<Vec<ConnectorJobRecord>, StoreError>;
    async fn billing_get_wallet(
        &self,
        tenant_id: &str,
        currency: &str,
    ) -> Result<Option<WalletRecord>, StoreError>;
    async fn billing_list_wallets(&self, tenant_id: &str) -> Result<Vec<WalletRecord>, StoreError>;
    async fn billing_ensure_wallet(&self, wallet: &WalletRecord) -> Result<(), StoreError>;
    async fn billing_post_ledger(
        &self,
        entries: &[LedgerEntryRecord],
        balance_delta_micros: i64,
        allow_negative: bool,
        currency: &str,
    ) -> Result<LedgerTxResult, StoreError>;
    async fn billing_list_ledger(
        &self,
        tenant_id: &str,
        limit: i64,
    ) -> Result<Vec<LedgerEntryRecord>, StoreError>;
    async fn billing_upsert_topup(&self, topup: &TopupRecord) -> Result<(), StoreError>;
    async fn billing_get_topup_by_key(
        &self,
        tenant_id: &str,
        idempotency_key: &str,
    ) -> Result<Option<TopupRecord>, StoreError>;
    async fn billing_set_topup_status(
        &self,
        topup_id: &str,
        status: &str,
    ) -> Result<(), StoreError>;
    async fn billing_upsert_plan(&self, plan: &PlanRecord) -> Result<(), StoreError>;
    async fn billing_get_plan(&self, plan_id: &str) -> Result<Option<PlanRecord>, StoreError>;
    async fn billing_list_plans(&self) -> Result<Vec<PlanRecord>, StoreError>;
    async fn billing_upsert_subscription(
        &self,
        subscription: &SubscriptionRecord,
    ) -> Result<(), StoreError>;
    async fn billing_get_subscription(
        &self,
        tenant_id: &str,
    ) -> Result<Option<SubscriptionRecord>, StoreError>;
    async fn billing_upsert_entitlement(
        &self,
        entitlement: &EntitlementRecord,
    ) -> Result<(), StoreError>;
    async fn billing_get_entitlement(
        &self,
        tenant_id: &str,
    ) -> Result<Option<EntitlementRecord>, StoreError>;
    async fn billing_insert_payment_event(
        &self,
        event: &PaymentEventRecord,
    ) -> Result<bool, StoreError>;
    async fn billing_mark_payment_event(
        &self,
        provider_event_id: &str,
        status: &str,
    ) -> Result<(), StoreError>;
    async fn billing_upsert_invoice(&self, invoice: &InvoiceRecord) -> Result<(), StoreError>;
    async fn billing_list_invoices(
        &self,
        tenant_id: &str,
        limit: i64,
    ) -> Result<Vec<InvoiceRecord>, StoreError>;

    fn backend(&self) -> &'static str;
    fn dialect(&self) -> SqlDialect;
    fn fts_available(&self) -> bool;

    async fn migrate(&self) -> Result<(), StoreError>;
    async fn health(&self) -> Result<(), StoreError>;

    async fn ensure_space(&self, space: &SpaceRecord) -> Result<(), StoreError>;
    async fn get_space(
        &self,
        tenant_id: &str,
        space_id: &str,
    ) -> Result<Option<SpaceRecord>, StoreError>;
    async fn list_spaces(&self, tenant_id: &str) -> Result<Vec<SpaceRecord>, StoreError>;
    async fn update_space(&self, space: &SpaceRecord) -> Result<(), StoreError>;

    async fn create_group(&self, group: &GroupRecord) -> Result<(), StoreError>;
    async fn get_group(
        &self,
        tenant_id: &str,
        group_id: &str,
    ) -> Result<Option<GroupRecord>, StoreError>;
    async fn list_groups(&self, tenant_id: &str) -> Result<Vec<GroupRecord>, StoreError>;
    async fn update_group(&self, group: &GroupRecord) -> Result<(), StoreError>;
    async fn add_group_member(&self, member: &GroupMemberRecord) -> Result<(), StoreError>;
    async fn get_group_member(
        &self,
        group_id: &str,
        principal_id: &str,
    ) -> Result<Option<GroupMemberRecord>, StoreError>;
    async fn list_group_members(
        &self,
        group_id: &str,
    ) -> Result<Vec<GroupMemberRecord>, StoreError>;
    async fn list_group_memberships(
        &self,
        tenant_id: &str,
        principal_id: &str,
    ) -> Result<Vec<GroupMembership>, StoreError>;
    async fn update_group_member_role(
        &self,
        group_id: &str,
        principal_id: &str,
        role: GroupRole,
    ) -> Result<(), StoreError>;
    async fn remove_group_member(
        &self,
        group_id: &str,
        principal_id: &str,
    ) -> Result<(), StoreError>;
    async fn count_group_admins(&self, group_id: &str) -> Result<i64, StoreError>;

    async fn get_space_member(
        &self,
        space_id: &str,
        principal_type: PrincipalType,
        principal_id: &str,
    ) -> Result<Option<SpaceMemberRecord>, StoreError>;
    async fn list_space_members(
        &self,
        space_id: &str,
    ) -> Result<Vec<SpaceMemberRecord>, StoreError>;
    async fn upsert_space_member(&self, member: &SpaceMemberRecord) -> Result<(), StoreError>;
    async fn remove_space_member(
        &self,
        space_id: &str,
        principal_type: PrincipalType,
        principal_id: &str,
    ) -> Result<(), StoreError>;
    async fn count_space_owners(&self, space_id: &str) -> Result<i64, StoreError>;

    async fn list_pages(&self, policy: &Policy, limit: i64)
        -> Result<Vec<PageSummary>, StoreError>;
    async fn list_page_records(
        &self,
        policy: &Policy,
        limit: i64,
    ) -> Result<Vec<PageRecord>, StoreError>;
    async fn get_revision(
        &self,
        tenant_id: &str,
        revision_id: &str,
    ) -> Result<Option<RevisionRecord>, StoreError>;
    async fn get_page(
        &self,
        policy: &Policy,
        space_id: &str,
        slug: &str,
        lang: Option<&str>,
    ) -> Result<Option<PageView>, StoreError>;
    /// Lookup without ACL/temporal filtering. Used only to refuse overwriting
    /// a page the caller cannot see; it must never feed recall or output.
    async fn get_page_unfiltered(
        &self,
        tenant_id: &str,
        space_id: &str,
        slug: &str,
        lang: Option<&str>,
    ) -> Result<Option<PageView>, StoreError>;
    async fn save_page(
        &self,
        page: &PageRecord,
        revision: &RevisionRecord,
    ) -> Result<(), StoreError>;
    async fn archive_document_path(
        &self,
        tenant_id: &str,
        space_id: &str,
        logical_path: &str,
    ) -> Result<usize, StoreError>;
    async fn restore_document_path(
        &self,
        tenant_id: &str,
        space_id: &str,
        logical_path: &str,
    ) -> Result<usize, StoreError>;
    async fn archive_page(
        &self,
        tenant_id: &str,
        space_id: &str,
        slug: &str,
        lang: &str,
    ) -> Result<bool, StoreError>;
    async fn restore_page(
        &self,
        tenant_id: &str,
        space_id: &str,
        slug: &str,
        lang: &str,
    ) -> Result<bool, StoreError>;
    async fn list_revisions(
        &self,
        tenant_id: &str,
        space_id: &str,
        slug: &str,
        lang: Option<&str>,
    ) -> Result<Vec<RevisionSummary>, StoreError>;

    async fn upsert_document(
        &self,
        doc: &DocumentRecord,
        chunks: &[ChunkRecord],
        expected_doc_id: Option<&str>,
    ) -> Result<UpsertOutcome, StoreError>;
    async fn list_documents(
        &self,
        tenant_id: &str,
        space_id: Option<&str>,
    ) -> Result<Vec<DocumentRecord>, StoreError>;

    async fn search_keyword(
        &self,
        policy: &Policy,
        query: &str,
        limit: i64,
    ) -> Result<Vec<SearchHit>, StoreError>;
    async fn fetch_embedding_candidates(
        &self,
        policy: &Policy,
        limit: i64,
    ) -> Result<Vec<Candidate>, StoreError>;

    async fn has_events(&self, policy: &Policy) -> Result<bool, StoreError>;
    /// True when at least one visible chunk carries a stored dense vector.
    /// Used by `auto` to decide whether the dense channel can contribute; the
    /// visibility predicate keeps the probe inside the caller's tenant/spaces.
    async fn has_embeddings(&self, policy: &Policy) -> Result<bool, StoreError>;
    async fn has_current_event(
        &self,
        tenant_id: &str,
        space_id: &str,
        chunk_id: &str,
        extractor_version: &str,
        extraction_model: &str,
    ) -> Result<bool, StoreError>;
    async fn upsert_event(
        &self,
        event: &EventRecord,
        entities: &[EntityRecord],
        links: &[EventEntityRecord],
    ) -> Result<EventUpsertOutcome, StoreError>;
    async fn search_events(
        &self,
        policy: &Policy,
        params: EventSearchParams<'_>,
    ) -> Result<Vec<SearchHit>, StoreError>;

    async fn get_seed_meta(
        &self,
        tenant_id: &str,
        space_id: &str,
    ) -> Result<Option<SeedMeta>, StoreError>;
    async fn upsert_seed_meta(&self, meta: &SeedMeta) -> Result<(), StoreError>;
    async fn is_database_empty(&self, tenant_id: &str) -> Result<bool, StoreError>;

    async fn get_user_by_email(
        &self,
        tenant_id: &str,
        email: &str,
    ) -> Result<Option<UserRecord>, StoreError>;
    async fn get_user_by_external_id(
        &self,
        tenant_id: &str,
        source: &str,
        external_id: &str,
    ) -> Result<Option<UserRecord>, StoreError>;
    async fn create_sso_user(&self, user: &UserRecord) -> Result<(), StoreError>;
    async fn link_sso_identity(
        &self,
        tenant_id: &str,
        user_id: &str,
        email: &str,
        external_id: &str,
        last_sso_at: &str,
    ) -> Result<(), StoreError>;
    async fn touch_sso_login(
        &self,
        tenant_id: &str,
        user_id: &str,
        last_sso_at: &str,
    ) -> Result<(), StoreError>;
    async fn touch_user_login(
        &self,
        tenant_id: &str,
        user_id: &str,
        at: &str,
    ) -> Result<(), StoreError>;
    async fn set_user_display_name(
        &self,
        tenant_id: &str,
        user_id: &str,
        display_name: &str,
    ) -> Result<(), StoreError>;
    async fn set_user_password(
        &self,
        tenant_id: &str,
        user_id: &str,
        password_hash: &str,
    ) -> Result<(), StoreError>;
    async fn get_audit_event(
        &self,
        tenant_id: &str,
        event_id: &str,
    ) -> Result<Option<AuditEventRecord>, StoreError>;
    async fn billing_list_subscriptions(
        &self,
        tenant_id: &str,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<SubscriptionRecord>, StoreError>;
    async fn billing_count_active_subscriptions(&self, plan_id: &str) -> Result<i64, StoreError>;
    async fn billing_delete_plan(&self, plan_id: &str) -> Result<bool, StoreError>;
    async fn update_provider_model_record(
        &self,
        tenant_id: &str,
        model: &ProviderModelRecord,
    ) -> Result<(), StoreError>;
    async fn delete_provider_model(
        &self,
        tenant_id: &str,
        provider_model_id: &str,
    ) -> Result<bool, StoreError>;
    async fn admin_overview(
        &self,
        tenant_id: &str,
        since: &str,
    ) -> Result<AdminOverview, StoreError>;
    async fn add_user(
        &self,
        tenant_id: &str,
        username: &str,
        password_hash: &str,
    ) -> Result<UserRecord, StoreError>;
    async fn add_user_with_role(
        &self,
        tenant_id: &str,
        username: &str,
        password_hash: &str,
        role: UserRole,
    ) -> Result<UserRecord, StoreError>;
    async fn set_user_disabled(
        &self,
        tenant_id: &str,
        user_id: &str,
        disabled: bool,
    ) -> Result<(), StoreError>;
    async fn set_user_role(
        &self,
        tenant_id: &str,
        username: &str,
        role: UserRole,
    ) -> Result<bool, StoreError>;
    async fn list_users(&self, tenant_id: &str) -> Result<Vec<UserRecord>, StoreError>;
    async fn get_user(
        &self,
        tenant_id: &str,
        username: &str,
    ) -> Result<Option<UserRecord>, StoreError>;
    async fn get_user_by_id(
        &self,
        tenant_id: &str,
        user_id: &str,
    ) -> Result<Option<UserRecord>, StoreError>;

    async fn create_session(&self, session: &SessionRecord) -> Result<(), StoreError>;
    async fn get_session(&self, session_hash: &str) -> Result<Option<SessionRecord>, StoreError>;
    async fn delete_session(&self, session_hash: &str) -> Result<(), StoreError>;

    async fn create_token(&self, token: &TokenRecord) -> Result<(), StoreError>;
    async fn get_token(&self, token_hash: &str) -> Result<Option<TokenRecord>, StoreError>;
    async fn list_tokens(
        &self,
        tenant_id: &str,
        user_id: &str,
    ) -> Result<Vec<TokenRecord>, StoreError>;
    async fn revoke_token(&self, token_id: &str) -> Result<bool, StoreError>;

    async fn get_quota(
        &self,
        tenant_id: &str,
        scope_type: QuotaScope,
        scope_id: &str,
    ) -> Result<Option<QuotaRecord>, StoreError>;
    async fn list_quotas(&self, tenant_id: &str) -> Result<Vec<QuotaRecord>, StoreError>;
    async fn upsert_quota(&self, quota: &QuotaRecord) -> Result<(), StoreError>;
    async fn delete_quota(
        &self,
        tenant_id: &str,
        scope_type: QuotaScope,
        scope_id: &str,
    ) -> Result<bool, StoreError>;
    async fn list_quota_overrides(
        &self,
        tenant_id: &str,
        scope_type: QuotaScope,
        scope_id: &str,
        dimension: &str,
        now: &str,
    ) -> Result<Vec<QuotaOverrideRecord>, StoreError>;
    async fn upsert_quota_override(&self, item: &QuotaOverrideRecord) -> Result<(), StoreError>;
    async fn get_storage_usage(
        &self,
        tenant_id: &str,
        space_id: &str,
    ) -> Result<Option<StorageUsage>, StoreError>;
    async fn list_storage_usage(&self, tenant_id: &str) -> Result<Vec<StorageUsage>, StoreError>;
    async fn upsert_storage_usage(&self, usage: &StorageUsage) -> Result<(), StoreError>;
    async fn recompute_storage_usage(
        &self,
        tenant_id: &str,
        space_id: &str,
    ) -> Result<StorageUsage, StoreError>;
    async fn add_reserved_bytes(
        &self,
        tenant_id: &str,
        space_id: &str,
        delta: i64,
        at: &str,
    ) -> Result<(), StoreError>;

    async fn list_providers(&self, tenant_id: &str) -> Result<Vec<ProviderRecord>, StoreError>;
    async fn get_provider(
        &self,
        tenant_id: &str,
        provider_id: &str,
    ) -> Result<Option<ProviderRecord>, StoreError>;
    async fn upsert_provider(
        &self,
        tenant_id: &str,
        provider: &ProviderRecord,
    ) -> Result<(), StoreError>;
    async fn delete_provider(&self, tenant_id: &str, provider_id: &str)
        -> Result<bool, StoreError>;

    async fn list_provider_models(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<ProviderModelRecord>, StoreError>;
    async fn get_provider_model(
        &self,
        tenant_id: &str,
        id: &str,
    ) -> Result<Option<ProviderModelRecord>, StoreError>;
    async fn upsert_provider_model(
        &self,
        tenant_id: &str,
        model: &ProviderModelRecord,
    ) -> Result<(), StoreError>;

    async fn list_model_aliases(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<ModelAliasRecord>, StoreError>;
    async fn get_model_alias(
        &self,
        tenant_id: &str,
        public_id: &str,
    ) -> Result<Option<ModelAliasRecord>, StoreError>;
    async fn upsert_model_alias(
        &self,
        tenant_id: &str,
        alias: &ModelAliasRecord,
    ) -> Result<(), StoreError>;
    async fn delete_model_alias(
        &self,
        tenant_id: &str,
        public_id: &str,
    ) -> Result<bool, StoreError>;

    async fn list_routes(
        &self,
        tenant_id: &str,
        alias: Option<&str>,
    ) -> Result<Vec<RouteRecord>, StoreError>;
    async fn upsert_route(&self, tenant_id: &str, route: &RouteRecord) -> Result<(), StoreError>;
    async fn delete_route(&self, tenant_id: &str, route_id: &str) -> Result<bool, StoreError>;
    async fn update_route_health(
        &self,
        tenant_id: &str,
        route_id: &str,
        health: &str,
        circuit_state: &str,
        last_error_at: Option<&str>,
        circuit_open_until: Option<&str>,
    ) -> Result<(), StoreError>;

    async fn insert_usage_event(&self, event: &UsageEventRecord) -> Result<(), StoreError>;
    async fn list_usage_events(
        &self,
        filter: &UsageFilter,
    ) -> Result<Vec<UsageEventRecord>, StoreError>;
    async fn set_usage_rewritten_query(
        &self,
        usage_event_id: &str,
        rewritten_query: &str,
    ) -> Result<(), StoreError>;
    async fn aggregate_usage_events(
        &self,
        filter: &UsageFilter,
    ) -> Result<UsageAggregate, StoreError>;

    async fn list_token_quotas(&self, tenant_id: &str)
        -> Result<Vec<TokenQuotaRecord>, StoreError>;
    async fn upsert_token_quota(&self, quota: &TokenQuotaRecord) -> Result<(), StoreError>;
    async fn delete_token_quota(&self, tenant_id: &str, quota_id: &str)
        -> Result<bool, StoreError>;

    async fn get_quota_counter(
        &self,
        tenant_id: &str,
        scope_type: &str,
        scope_id: &str,
        period_start: &str,
    ) -> Result<Option<QuotaCounter>, StoreError>;
    async fn list_quota_counters(&self, tenant_id: &str) -> Result<Vec<QuotaCounter>, StoreError>;
    async fn add_quota_reservation(&self, reservation: &QuotaReservation)
        -> Result<(), StoreError>;
    async fn get_quota_reservation(
        &self,
        tenant_id: &str,
        request_id: &str,
    ) -> Result<Option<QuotaReservation>, StoreError>;
    async fn update_quota_reservation(
        &self,
        tenant_id: &str,
        request_id: &str,
        status: &str,
    ) -> Result<(), StoreError>;
    async fn list_quota_reservations(
        &self,
        tenant_id: &str,
        request_id: &str,
    ) -> Result<Vec<QuotaReservation>, StoreError>;
    async fn adjust_quota_counter(
        &self,
        tenant_id: &str,
        scope_type: &str,
        scope_id: &str,
        period_start: &str,
        token_delta: i64,
        cost_delta: i64,
    ) -> Result<(), StoreError>;
    async fn cleanup_expired_quota_reservations(
        &self,
        tenant_id: &str,
        now: &str,
    ) -> Result<usize, StoreError>;

    async fn upsert_document_summary(
        &self,
        summary: &DocumentSummaryRecord,
    ) -> Result<(), StoreError>;
    async fn get_document_summary(
        &self,
        tenant_id: &str,
        space_id: &str,
        doc_id: &str,
        revision_id: &str,
        model: &str,
        prompt_version: &str,
    ) -> Result<Option<DocumentSummaryRecord>, StoreError>;

    async fn create_summary_job(&self, job: &SummaryJobRecord) -> Result<bool, StoreError>;
    async fn list_summary_jobs(
        &self,
        status: &str,
        limit: i64,
    ) -> Result<Vec<SummaryJobRecord>, StoreError>;
    async fn update_summary_job(
        &self,
        job_id: &str,
        status: &str,
        attempts: u32,
        started_at: Option<&str>,
        finished_at: Option<&str>,
        error: Option<&str>,
    ) -> Result<(), StoreError>;

    async fn create_conversation(
        &self,
        conversation: &ConversationRecord,
    ) -> Result<(), StoreError>;
    async fn get_conversation(
        &self,
        tenant_id: &str,
        conversation_id: &str,
    ) -> Result<Option<ConversationRecord>, StoreError>;
    async fn list_conversations(
        &self,
        tenant_id: &str,
        owner_id: &str,
        limit: i64,
    ) -> Result<Vec<ConversationRecord>, StoreError>;
    async fn restore_conversation(
        &self,
        tenant_id: &str,
        conversation_id: &str,
    ) -> Result<bool, StoreError>;
    async fn update_conversation(
        &self,
        tenant_id: &str,
        conversation_id: &str,
        title: Option<&str>,
        status: Option<&str>,
    ) -> Result<(), StoreError>;
    async fn bind_conversation_external_session(
        &self,
        tenant_id: &str,
        conversation_id: &str,
        external_session_id: &str,
    ) -> Result<(), StoreError>;
    async fn insert_message(&self, message: &MessageRecord) -> Result<(), StoreError>;
    async fn list_messages(
        &self,
        conversation_id: &str,
        limit: i64,
    ) -> Result<Vec<MessageRecord>, StoreError>;

    async fn insert_audit(&self, event: &AuditEventRecord) -> Result<(), StoreError>;
    async fn list_audit(&self, filter: &AuditFilter) -> Result<Vec<AuditEventRecord>, StoreError>;
}

pub async fn open(config: &Config) -> anyhow::Result<Arc<dyn Store>> {
    let store: Arc<dyn Store> = match config.storage.backend {
        Backend::Sqlite => {
            if let Some(parent) = config.storage.sqlite_path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            let store = sqlite::SqliteStore::open(&config.storage.sqlite_path).await?;
            let started = std::time::Instant::now();
            store.migrate().await?;
            tracing::info!(
                service = "lkb-db",
                operation = "migrate",
                db_ms = started.elapsed().as_millis() as u64,
                tenant = config.storage.tenant_id,
                route = "migrate",
                method = "DB",
                status = 200u64,
                "db.migrate"
            );
            Arc::new(store)
        }
        Backend::Postgres => {
            let url = config.database_url().ok_or_else(|| {
                anyhow::anyhow!("environment variable {} is not set", config.storage.url_env)
            })?;
            let store = postgres::PostgresStore::open(&url).await?;
            let started = std::time::Instant::now();
            store.migrate().await?;
            tracing::info!(
                service = "lkb-db",
                operation = "migrate",
                db_ms = started.elapsed().as_millis() as u64,
                tenant = config.storage.tenant_id,
                route = "migrate",
                method = "DB",
                status = 200u64,
                "db.migrate"
            );
            Arc::new(store)
        }
    };
    crate::gateway::materialize_legacy(&store, config).await?;
    Ok(store)
}

/// Solo must have an explicit allow-all space record. Team/enterprise spaces
/// are created explicitly (import with `--acl-json`, or first wiki save), so
/// no implicit allow-all space is manufactured there.
pub async fn bootstrap_space(store: &Arc<dyn Store>, config: &Config) -> anyhow::Result<()> {
    if config.profile_name() != ProfileName::Solo {
        return Ok(());
    }
    let tenant = &config.storage.tenant_id;
    let space = &config.storage.default_space;
    if store.get_space(tenant, space).await?.is_none() {
        let record = SpaceRecord::new(
            tenant,
            space,
            "Personal",
            &Acl::allow_all(),
            "local",
            &crate::util::now_rfc3339(),
        );
        store.ensure_space(&record).await?;
    }
    Ok(())
}

pub fn scope_rank(scope: TokenScope) -> u8 {
    match scope {
        TokenScope::Read => 1,
        TokenScope::Write => 2,
        TokenScope::Admin => 3,
    }
}

#[derive(Debug, Clone)]
pub(crate) enum BindValue {
    Text(String),
    Int(i64),
    Float(f64),
}

pub(crate) struct SqlBinds {
    dialect: SqlDialect,
    next: usize,
    pub values: Vec<BindValue>,
}

impl SqlBinds {
    pub(crate) fn new(dialect: SqlDialect, start: usize) -> Self {
        Self {
            dialect,
            next: start,
            values: Vec::new(),
        }
    }

    pub(crate) fn placeholder(&mut self) -> String {
        let placeholder = self.dialect.placeholder(self.next);
        self.next += 1;
        placeholder
    }

    pub(crate) fn next(&self) -> usize {
        self.next
    }

    pub(crate) fn text(&mut self, value: impl Into<String>) -> String {
        let placeholder = self.placeholder();
        self.values.push(BindValue::Text(value.into()));
        placeholder
    }

    pub(crate) fn int(&mut self, value: i64) -> String {
        let placeholder = self.placeholder();
        self.values.push(BindValue::Int(value));
        placeholder
    }

    pub(crate) fn float(&mut self, value: f64) -> String {
        let placeholder = self.placeholder();
        self.values.push(BindValue::Float(value));
        placeholder
    }

    pub(crate) fn append_visibility(&mut self, visibility: &crate::governance::Visibility) {
        self.next += visibility.binds.len();
        for value in &visibility.binds {
            self.values.push(BindValue::Text(value.clone()));
        }
    }
}
