use super::*;
use crate::util::{fts5_query, now_rfc3339, query_terms};
use async_trait::async_trait;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteConnection, SqliteJournalMode, SqlitePool, SqlitePoolOptions,
};
use sqlx::Row;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const MIGRATIONS: &[(&str, &str)] = &[
    (
        "0001_init",
        include_str!("../../migrations/sqlite/0001_init.sql"),
    ),
    (
        "0002_fts",
        include_str!("../../migrations/sqlite/0002_fts.sql"),
    ),
    (
        "0003_events",
        include_str!("../../migrations/sqlite/0003_events.sql"),
    ),
    (
        "0004_lang_seed",
        include_str!("../../migrations/sqlite/0004_lang_seed.sql"),
    ),
    (
        "0005_roles_spaces_quota_audit",
        include_str!("../../migrations/sqlite/0005_roles_spaces_quota_audit.sql"),
    ),
    (
        "0006_model_gateway",
        include_str!("../../migrations/sqlite/0006_model_gateway.sql"),
    ),
    (
        "0007_summaries_conversations",
        include_str!("../../migrations/sqlite/0007_summaries_conversations.sql"),
    ),
    (
        "0012_user_last_login",
        include_str!("../../migrations/sqlite/0012_user_last_login.sql"),
    ),
    (
        "0011_sso",
        include_str!("../../migrations/sqlite/0011_sso.sql"),
    ),
    (
        "0010_connectors",
        include_str!("../../migrations/sqlite/0010_connectors.sql"),
    ),
    (
        "0009_billing",
        include_str!("../../migrations/sqlite/0009_billing.sql"),
    ),
    (
        "0008_message_context",
        include_str!("../../migrations/sqlite/0008_message_context.sql"),
    ),
];

pub struct SqliteStore {
    pool: SqlitePool,
    fts_available: AtomicBool,
}

impl SqliteStore {
    pub async fn open(path: &Path) -> Result<Self, StoreError> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;
        Ok(Self {
            pool,
            fts_available: AtomicBool::new(false),
        })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    async fn event_seed_entities(
        &self,
        policy: &Policy,
        query: &str,
        max_seeds: i64,
    ) -> Result<Vec<String>, StoreError> {
        let mut terms = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for term in crate::util::query_terms(query) {
            let normalized = crate::event::normalize_entity_name(&term);
            if !normalized.is_empty() && seen.insert(normalized.clone()) {
                terms.push(normalized);
            }
            if terms.len() >= 8 {
                break;
            }
        }
        if terms.is_empty() {
            return Ok(Vec::new());
        }

        let visibility = policy.visibility_from("e", self.dialect(), 1)?;
        let mut binds = SqlBinds::new(self.dialect(), 1);
        binds.append_visibility(&visibility);
        let mut conditions = Vec::new();
        for term in terms {
            let exact = binds.text(term.clone());
            conditions.push(format!("en.normalized_name = {exact}"));
            let length = binds.text(term.clone());
            let prefix = binds.text(term.clone());
            conditions.push(format!(
                "substr(en.normalized_name, 1, length({length})) = {prefix}"
            ));
            let contains = binds.text(term);
            conditions.push(format!("instr(en.normalized_name, {contains}) > 0"));
        }
        let limit = binds.int(max_seeds);
        let sql = format!(
            "SELECT en.entity_id
             FROM entities en
             JOIN event_entities ee
               ON ee.tenant_id = en.tenant_id AND ee.space_id = en.space_id
              AND ee.entity_id = en.entity_id
             JOIN events e
               ON e.tenant_id = ee.tenant_id AND e.space_id = ee.space_id
              AND e.event_id = ee.event_id
             WHERE {vis} AND ({conditions})
             GROUP BY en.entity_id
             ORDER BY en.normalized_name
             LIMIT {limit}",
            vis = visibility.sql,
            conditions = conditions.join(" OR "),
        );
        let mut query = sqlx::query_scalar::<_, String>(&sql);
        for value in &binds.values {
            query = match value {
                BindValue::Text(value) => query.bind(value.as_str()),
                BindValue::Int(value) => query.bind(*value),
                BindValue::Float(value) => query.bind(*value),
            };
        }
        Ok(query.fetch_all(&self.pool).await?)
    }

    async fn event_expand(
        &self,
        policy: &Policy,
        entity_ids: &[String],
        fts_query: &str,
        text_weight: f64,
        max_events: i64,
    ) -> Result<Vec<EventRow>, StoreError> {
        if entity_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut binds = SqlBinds::new(self.dialect(), 1);
        let mut placeholders = Vec::new();
        for entity_id in entity_ids {
            placeholders.push(binds.text(entity_id.clone()));
        }
        let visibility = policy.visibility_from("e", self.dialect(), binds.next())?;
        binds.append_visibility(&visibility);
        let max_events_ph = binds.int(max_events);
        let weight_ph = binds.float(text_weight);
        let fts_parts = if fts_query.is_empty() {
            None
        } else {
            let tenant = binds.text(policy.principal.tenant_id.clone());
            let mut spaces = Vec::new();
            for space in &policy.principal.allowed_space_ids {
                spaces.push(binds.text(space.clone()));
            }
            let query = binds.text(fts_query);
            Some((tenant, spaces, query))
        };
        let limit_ph = binds.int(max_events);
        let (fts_join, text_score) = match fts_parts {
            Some((tenant, spaces, query)) => (
                format!(
                    "LEFT JOIN (SELECT event_id, bm25(events_fts) AS fts_score
                               FROM events_fts
                               WHERE tenant_id = {tenant}
                                 AND space_id IN ({spaces})
                                 AND events_fts MATCH {query}) f
                       ON f.event_id = e.event_id",
                    spaces = spaces.join(", "),
                ),
                "CASE WHEN COALESCE(f.fts_score, 0.0) < 0 \
                     THEN -COALESCE(f.fts_score, 0.0) ELSE 0.0 END"
                    .to_string(),
            ),
            None => (String::new(), "0.0".to_string()),
        };
        let sql = format!(
            "WITH scored AS (
                 SELECT ee.event_id, SUM(ee.weight) AS entity_score
                 FROM event_entities ee
                 JOIN events e
                   ON e.tenant_id = ee.tenant_id AND e.space_id = ee.space_id
                  AND e.event_id = ee.event_id
                 WHERE ee.entity_id IN ({entity_placeholders}) AND {vis}
                 GROUP BY ee.event_id
                 ORDER BY entity_score DESC
                 LIMIT {max_events_ph}
             )
             SELECT s.event_id, e.chunk_id, e.doc_id, d.logical_path, d.title,
                    c.line_start, c.line_end, c.body, c.tenant_id, c.space_id,
                    c.valid_from, c.valid_to, c.superseded_by, c.acl_json,
                    c.acl_principals, c.lang, c.version,
                    (s.entity_score + {weight_ph} * {text_score}) AS score
             FROM scored s
             JOIN events e ON e.event_id = s.event_id
             JOIN chunks c
               ON c.tenant_id = e.tenant_id AND c.chunk_id = e.chunk_id
             JOIN documents d
               ON d.tenant_id = c.tenant_id AND d.doc_id = c.doc_id
             {fts_join}
             ORDER BY score DESC, e.event_id
             LIMIT {limit_ph}",
            entity_placeholders = placeholders.join(", "),
            vis = visibility.sql,
            fts_join = fts_join,
            text_score = text_score,
        );
        let mut query = sqlx::query(&sql);
        for value in &binds.values {
            query = match value {
                BindValue::Text(value) => query.bind(value.as_str()),
                BindValue::Int(value) => query.bind(*value),
                BindValue::Float(value) => query.bind(*value),
            };
        }
        let rows = query.fetch_all(&self.pool).await?;
        let mut out = Vec::new();
        for row in rows {
            out.push(EventRow {
                event_id: row.try_get("event_id")?,
                chunk_id: row.try_get("chunk_id")?,
                doc_id: row.try_get("doc_id")?,
                path: row.try_get("logical_path")?,
                title: row.try_get("title")?,
                line_start: row.try_get("line_start")?,
                line_end: row.try_get("line_end")?,
                quote: row.try_get("body")?,
                governance: GovernanceFields {
                    tenant_id: row.try_get("tenant_id")?,
                    space_id: row.try_get("space_id")?,
                    acl_json: row.try_get("acl_json")?,
                    acl_principals: row.try_get("acl_principals")?,
                    lang: row.try_get("lang")?,
                    valid_from: row.try_get("valid_from")?,
                    valid_to: row.try_get("valid_to")?,
                    superseded_by: row.try_get("superseded_by")?,
                    version: row.try_get::<i64, _>("version")? as u32,
                },
                score: row.try_get("score")?,
            });
        }
        Ok(out)
    }

    async fn event_entities_for_events(
        &self,
        policy: &Policy,
        event_ids: &[String],
        max_entities: i64,
    ) -> Result<Vec<String>, StoreError> {
        if event_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut binds = SqlBinds::new(self.dialect(), 1);
        let mut placeholders = Vec::new();
        for event_id in event_ids {
            placeholders.push(binds.text(event_id.clone()));
        }
        let visibility = policy.visibility_from("e", self.dialect(), binds.next())?;
        binds.append_visibility(&visibility);
        let limit = binds.int(max_entities);
        let sql = format!(
            "SELECT DISTINCT ee.entity_id
             FROM event_entities ee
             JOIN events e
               ON e.tenant_id = ee.tenant_id AND e.space_id = ee.space_id
              AND e.event_id = ee.event_id
             WHERE ee.event_id IN ({event_placeholders}) AND {vis}
             LIMIT {limit}",
            event_placeholders = placeholders.join(", "),
            vis = visibility.sql,
        );
        let mut query = sqlx::query_scalar::<_, String>(&sql);
        for value in &binds.values {
            query = match value {
                BindValue::Text(value) => query.bind(value.as_str()),
                BindValue::Int(value) => query.bind(*value),
                BindValue::Float(value) => query.bind(*value),
            };
        }
        Ok(query.fetch_all(&self.pool).await?)
    }
}

fn statement_has_sql(statement: &str) -> bool {
    statement.lines().any(|line| {
        let line = line.trim();
        !line.is_empty() && !line.starts_with("--")
    })
}

async fn execute_script(connection: &mut SqliteConnection, sql: &str) -> Result<(), sqlx::Error> {
    for statement in sql.split(';') {
        if !statement_has_sql(statement) {
            continue;
        }
        sqlx::query(statement).execute(&mut *connection).await?;
    }
    Ok(())
}

fn document_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<DocumentRecord, sqlx::Error> {
    Ok(DocumentRecord {
        tenant_id: row.try_get("tenant_id")?,
        doc_id: row.try_get("doc_id")?,
        space_id: row.try_get("space_id")?,
        logical_path: row.try_get("logical_path")?,
        source_uri: row.try_get("source_uri")?,
        title: row.try_get("title")?,
        mime: row.try_get("mime")?,
        raw_hash: row.try_get("raw_hash")?,
        content_hash: row.try_get("content_hash")?,
        content: row.try_get("content")?,
        acl_json: row.try_get("acl_json")?,
        acl_principals: row.try_get("acl_principals")?,
        lang: row.try_get("lang")?,
        valid_from: row.try_get("valid_from")?,
        valid_to: row.try_get("valid_to")?,
        version: row.try_get::<i64, _>("version")? as u32,
        superseded_by: row.try_get("superseded_by")?,
        status: row.try_get("status")?,
        source_role: row.try_get("source_role")?,
        canonicalization_version: row.try_get("canonicalization_version")?,
        extractor_version: row.try_get("extractor_version")?,
        chunker_version: row.try_get("chunker_version")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn audit_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<AuditEventRecord, sqlx::Error> {
    Ok(AuditEventRecord {
        event_id: row.try_get("event_id")?,
        tenant_id: row.try_get("tenant_id")?,
        space_id: row.try_get("space_id")?,
        actor_id: row.try_get("actor_id")?,
        actor_type: row.try_get("actor_type")?,
        action: row.try_get("action")?,
        target_type: row.try_get("target_type")?,
        target_id: row.try_get("target_id")?,
        result: row.try_get("result")?,
        status_code: row.try_get("status_code")?,
        request_id: row.try_get("request_id")?,
        ip: row.try_get("ip")?,
        user_agent: row.try_get("user_agent")?,
        detail_json: row.try_get("detail_json")?,
        created_at: row.try_get("created_at")?,
    })
}

fn decode_error(err: String) -> sqlx::Error {
    sqlx::Error::Decode(Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        err,
    )))
}

fn space_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<SpaceRecord, sqlx::Error> {
    Ok(SpaceRecord {
        tenant_id: row.try_get("tenant_id")?,
        space_id: row.try_get("space_id")?,
        name: row.try_get("name")?,
        visibility: row.try_get("visibility")?,
        default_acl_json: row.try_get("default_acl_json")?,
        owner_principal_id: row.try_get("owner_principal_id")?,
        owner_type: row.try_get("owner_type")?,
        owner_id: row.try_get("owner_id")?,
        status: row.try_get("status")?,
        settings: row.try_get("settings")?,
        archived_at: row.try_get("archived_at")?,
        deleted_at: row.try_get("deleted_at")?,
        created_at: row.try_get("created_at")?,
    })
}

fn group_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<GroupRecord, sqlx::Error> {
    Ok(GroupRecord {
        group_id: row.try_get("group_id")?,
        tenant_id: row.try_get("tenant_id")?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        created_by: row.try_get("created_by")?,
        status: row.try_get("status")?,
        created_at: row
            .try_get("created_at")
            .or_else(|_| row.try_get("group_created_at"))?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn group_member_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<GroupMemberRecord, sqlx::Error> {
    Ok(GroupMemberRecord {
        group_id: row.try_get("group_id")?,
        principal_id: row.try_get("principal_id")?,
        role: GroupRole::parse(&row.try_get::<String, _>("role")?).map_err(decode_error)?,
        added_by: row.try_get("added_by")?,
        created_at: row.try_get("created_at")?,
    })
}

fn space_member_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<SpaceMemberRecord, sqlx::Error> {
    Ok(SpaceMemberRecord {
        space_id: row.try_get("space_id")?,
        principal_type: PrincipalType::parse(&row.try_get::<String, _>("principal_type")?)
            .map_err(decode_error)?,
        principal_id: row.try_get("principal_id")?,
        role: SpaceRole::parse(&row.try_get::<String, _>("role")?).map_err(decode_error)?,
        added_by: row.try_get("added_by")?,
        created_at: row.try_get("created_at")?,
    })
}

fn quota_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<QuotaRecord, sqlx::Error> {
    Ok(QuotaRecord {
        tenant_id: row.try_get("tenant_id")?,
        scope_type: QuotaScope::parse(&row.try_get::<String, _>("scope_type")?)
            .map_err(decode_error)?,
        scope_id: row.try_get("scope_id")?,
        storage_bytes: row.try_get("storage_bytes")?,
        documents: row.try_get("documents")?,
        chunks: row.try_get("chunks")?,
        members: row.try_get("members")?,
        spaces: row.try_get("spaces")?,
        enforcement: row.try_get("enforcement")?,
        updated_by: row.try_get("updated_by")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn usage_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<StorageUsage, sqlx::Error> {
    Ok(StorageUsage {
        tenant_id: row.try_get("tenant_id")?,
        space_id: row.try_get("space_id")?,
        source_bytes: row.try_get("source_bytes")?,
        index_bytes: row.try_get("index_bytes")?,
        history_bytes: row.try_get("history_bytes")?,
        total_bytes: row.try_get("total_bytes")?,
        reserved_bytes: row.try_get("reserved_bytes")?,
        documents: row.try_get("documents")?,
        chunks: row.try_get("chunks")?,
        members: row.try_get("members")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn quota_override_from_row(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<QuotaOverrideRecord, sqlx::Error> {
    Ok(QuotaOverrideRecord {
        override_id: row.try_get("override_id")?,
        tenant_id: row.try_get("tenant_id")?,
        scope_type: QuotaScope::parse(&row.try_get::<String, _>("scope_type")?)
            .map_err(decode_error)?,
        scope_id: row.try_get("scope_id")?,
        dimension: row.try_get("dimension")?,
        limit_value: row.try_get("limit_value")?,
        reason: row.try_get("reason")?,
        actor: row.try_get("actor")?,
        expires_at: row.try_get("expires_at")?,
        created_at: row.try_get("created_at")?,
    })
}

fn user_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<UserRecord, sqlx::Error> {
    Ok(UserRecord {
        user_id: row.try_get("user_id")?,
        tenant_id: row.try_get("tenant_id")?,
        username: row.try_get("username")?,
        password_hash: row.try_get("password_hash")?,
        display_name: row.try_get("display_name")?,
        created_at: row.try_get("created_at")?,
        disabled: row.try_get::<i64, _>("disabled")? != 0,
        email: row.try_get("email")?,
        external_id: row.try_get("external_id")?,
        source: row
            .try_get::<Option<String>, _>("source")?
            .unwrap_or_else(|| "local".to_string()),
        last_sso_at: row.try_get("last_sso_at")?,
        last_login_at: row.try_get("last_login_at")?,
        role: UserRole::parse(&row.try_get::<String, _>("role")?).map_err(|err| {
            sqlx::Error::Decode(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                err,
            )))
        })?,
    })
}

fn provider_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<ProviderRecord, sqlx::Error> {
    Ok(ProviderRecord {
        provider_id: row.try_get("provider_id")?,
        kind: row.try_get("kind")?,
        base_url: row.try_get("base_url")?,
        api_key_env: row.try_get("api_key_env")?,
        status: row.try_get("status")?,
        priority: row.try_get::<i64, _>("priority")? as i32,
        weight: row.try_get::<i64, _>("weight")? as i32,
        limits_json: row.try_get("limits_json")?,
        timeouts_json: row.try_get("timeouts_json")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
fn provider_model_from_row(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<ProviderModelRecord, sqlx::Error> {
    Ok(ProviderModelRecord {
        provider_model_id: row.try_get("provider_model_id")?,
        provider_id: row.try_get("provider_id")?,
        upstream_model: row.try_get("upstream_model")?,
        context_window: row.try_get("context_window")?,
        pricing_json: row.try_get("pricing_json")?,
        capabilities_json: row.try_get("capabilities_json")?,
        status: row.try_get("status")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
fn alias_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<ModelAliasRecord, sqlx::Error> {
    Ok(ModelAliasRecord {
        public_id: row.try_get("public_id")?,
        display_name: row.try_get("display_name")?,
        capabilities_json: row.try_get("capabilities_json")?,
        status: row.try_get("status")?,
        access_policy_json: row.try_get("access_policy_json")?,
        price_json: row.try_get("price_json")?,
        version: row.try_get::<i64, _>("version")? as u32,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
fn route_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<RouteRecord, sqlx::Error> {
    Ok(RouteRecord {
        route_id: row.try_get("route_id")?,
        alias_public_id: row.try_get("alias_public_id")?,
        provider_model_id: row.try_get("provider_model_id")?,
        weight: row.try_get::<i64, _>("weight")? as i32,
        priority: row.try_get::<i64, _>("priority")? as i32,
        health: row.try_get("health")?,
        circuit_state: row.try_get("circuit_state")?,
        last_error_at: row.try_get("last_error_at")?,
        circuit_open_until: row.try_get("circuit_open_until")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
fn usage_from_row_gateway(row: &sqlx::sqlite::SqliteRow) -> Result<UsageEventRecord, sqlx::Error> {
    Ok(UsageEventRecord {
        usage_event_id: row.try_get("usage_event_id")?,
        request_id: row.try_get("request_id")?,
        tenant_id: row.try_get("tenant_id")?,
        space_id: row.try_get("space_id")?,
        user_id: row.try_get("user_id")?,
        role: row.try_get("role")?,
        source: row.try_get("source")?,
        external_usage_id: row.try_get("external_usage_id")?,
        alias: row.try_get("alias")?,
        provider_model_id: row.try_get("provider_model_id")?,
        input_tokens: row.try_get("input_tokens")?,
        output_tokens: row.try_get("output_tokens")?,
        cached_tokens: row.try_get("cached_tokens")?,
        token_source: row.try_get("token_source")?,
        cost_micros: row.try_get("cost_micros")?,
        currency: row.try_get("currency")?,
        provider_cost_micros: row.try_get("provider_cost_micros")?,
        latency_ms: row.try_get("latency_ms")?,
        status: row.try_get("status")?,
        cache_hit: row.try_get::<i64, _>("cache_hit")? != 0,
        rewritten_query: row.try_get("rewritten_query")?,
        at: row.try_get("at")?,
    })
}
fn token_quota_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<TokenQuotaRecord, sqlx::Error> {
    Ok(TokenQuotaRecord {
        quota_id: row.try_get("quota_id")?,
        tenant_id: row.try_get("tenant_id")?,
        scope_type: row.try_get("scope_type")?,
        scope_id: row.try_get("scope_id")?,
        period: row.try_get("period")?,
        limit_tokens: row.try_get("limit_tokens")?,
        limit_requests: row.try_get("limit_requests")?,
        limit_cost_micros: row.try_get("limit_cost_micros")?,
        currency: row.try_get("currency")?,
        enforcement: row.try_get("enforcement")?,
        action: row.try_get("action")?,
        enabled: row.try_get::<i64, _>("enabled")? != 0,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
fn quota_counter_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<QuotaCounter, sqlx::Error> {
    Ok(QuotaCounter {
        tenant_id: row.try_get("tenant_id")?,
        scope_type: row.try_get("scope_type")?,
        scope_id: row.try_get("scope_id")?,
        period_start: row.try_get("period_start")?,
        reserved_tokens: row.try_get("reserved_tokens")?,
        used_tokens: row.try_get("used_tokens")?,
        reserved_requests: row.try_get("reserved_requests")?,
        used_requests: row.try_get("used_requests")?,
        reserved_cost_micros: row.try_get("reserved_cost_micros")?,
        used_cost_micros: row.try_get("used_cost_micros")?,
        updated_at: row.try_get("updated_at")?,
    })
}
fn quota_reservation_from_row(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<QuotaReservation, sqlx::Error> {
    Ok(QuotaReservation {
        reservation_id: row.try_get("reservation_id")?,
        request_id: row.try_get("request_id")?,
        tenant_id: row.try_get("tenant_id")?,
        scope_type: row.try_get("scope_type")?,
        scope_id: row.try_get("scope_id")?,
        period_start: row.try_get("period_start")?,
        reserved_tokens: row.try_get("reserved_tokens")?,
        reserved_requests: row.try_get("reserved_requests")?,
        reserved_cost_micros: row.try_get("reserved_cost_micros")?,
        status: row.try_get("status")?,
        expires_at: row.try_get("expires_at")?,
        created_at: row.try_get("created_at")?,
    })
}

fn summary_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<DocumentSummaryRecord, sqlx::Error> {
    Ok(DocumentSummaryRecord {
        tenant_id: row.try_get("tenant_id")?,
        space_id: row.try_get("space_id")?,
        doc_id: row.try_get("doc_id")?,
        revision_id: row.try_get("revision_id")?,
        lang: row.try_get("lang")?,
        model: row.try_get("model")?,
        prompt_version: row.try_get("prompt_version")?,
        content_hash: row.try_get("content_hash")?,
        status: row.try_get("status")?,
        summary_json: row.try_get("summary_json")?,
        usage_event_id: row.try_get("usage_event_id")?,
        generated_at: row.try_get("generated_at")?,
        error: row.try_get("error")?,
    })
}
fn summary_job_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<SummaryJobRecord, sqlx::Error> {
    Ok(SummaryJobRecord {
        job_id: row.try_get("job_id")?,
        tenant_id: row.try_get("tenant_id")?,
        space_id: row.try_get("space_id")?,
        doc_id: row.try_get("doc_id")?,
        revision_id: row.try_get("revision_id")?,
        status: row.try_get("status")?,
        attempts: row.try_get::<i64, _>("attempts")? as u32,
        scheduled_at: row.try_get("scheduled_at")?,
        started_at: row.try_get("started_at")?,
        finished_at: row.try_get("finished_at")?,
        error: row.try_get("error")?,
    })
}
fn conversation_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<ConversationRecord, sqlx::Error> {
    Ok(ConversationRecord {
        conversation_id: row.try_get("conversation_id")?,
        tenant_id: row.try_get("tenant_id")?,
        space_id: row.try_get("space_id")?,
        owner_id: row.try_get("owner_id")?,
        title: row.try_get("title")?,
        external_session_id: row.try_get("external_session_id")?,
        status: row.try_get("status")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
fn message_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<MessageRecord, sqlx::Error> {
    Ok(MessageRecord {
        message_id: row.try_get("message_id")?,
        conversation_id: row.try_get("conversation_id")?,
        role: row.try_get("role")?,
        content: row.try_get("content")?,
        citations_json: row.try_get("citations_json")?,
        model: row.try_get("model")?,
        usage_json: row.try_get("usage_json")?,
        degraded_json: row.try_get("degraded_json")?,
        request_id: row.try_get("request_id")?,
        mode: row.try_get("mode")?,
        rewritten_query: row.try_get("rewritten_query")?,
        conversation_context_hash: row.try_get("conversation_context_hash")?,
        created_at: row.try_get("created_at")?,
    })
}
fn token_kind_str(kind: PrincipalKind) -> &'static str {
    match kind {
        PrincipalKind::Human => "user",
        PrincipalKind::Agent => "agent",
        PrincipalKind::Service => "service",
    }
}

fn token_kind_parse(value: &str) -> Result<PrincipalKind, sqlx::Error> {
    match value {
        "user" => Ok(PrincipalKind::Human),
        "agent" => Ok(PrincipalKind::Agent),
        "service" => Ok(PrincipalKind::Service),
        other => Err(sqlx::Error::Decode(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unknown token kind {other:?}"),
        )))),
    }
}

fn token_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<TokenRecord, sqlx::Error> {
    let scope_value: String = row.try_get("scope")?;
    let scope = TokenScope::parse(&scope_value).map_err(|err| {
        sqlx::Error::Decode(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            err,
        )))
    })?;
    Ok(TokenRecord {
        token_hash: row.try_get("token_hash")?,
        token_id: row.try_get("token_id")?,
        tenant_id: row.try_get("tenant_id")?,
        user_id: row.try_get("user_id")?,
        scope,
        kind: token_kind_parse(&row.try_get::<String, _>("kind")?)?,
        label: row.try_get("label")?,
        created_at: row.try_get("created_at")?,
        expires_at: row.try_get("expires_at")?,
        revoked_at: row.try_get("revoked_at")?,
    })
}

fn page_view_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<PageView, sqlx::Error> {
    let page = PageRecord {
        tenant_id: row.try_get("tenant_id")?,
        space_id: row.try_get("space_id")?,
        slug: row.try_get("slug")?,
        lang: row.try_get("lang")?,
        title: row.try_get("title")?,
        summary: row.try_get("summary")?,
        order_index: row.try_get::<i64, _>("order_index")? as i32,
        current_revision_id: row.try_get("current_revision_id")?,
        content_hash: row.try_get("content_hash")?,
        acl_json: row.try_get("acl_json")?,
        acl_principals: row.try_get("acl_principals")?,
        valid_from: row.try_get("valid_from")?,
        valid_to: row.try_get("valid_to")?,
        version: row.try_get::<i64, _>("version")? as u32,
        superseded_by: row.try_get("superseded_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    };
    let revision = RevisionRecord {
        tenant_id: page.tenant_id.clone(),
        revision_id: row.try_get("revision_id")?,
        space_id: page.space_id.clone(),
        slug: page.slug.clone(),
        lang: row.try_get("revision_lang")?,
        title: page.title.clone(),
        markdown: row.try_get("markdown")?,
        content_hash: row.try_get("revision_content_hash")?,
        author: row.try_get("author")?,
        created_at: row.try_get("revision_created_at")?,
        version: row.try_get::<i64, _>("revision_version")? as u32,
        acl_json: page.acl_json.clone(),
        acl_principals: page.acl_principals.clone(),
        valid_from: page.valid_from.clone(),
        valid_to: page.valid_to.clone(),
        superseded_by: page.superseded_by.clone(),
        source_url: row.try_get("source_url")?,
        source_revision: row.try_get("source_revision")?,
    };
    Ok(PageView { page, revision })
}

fn hit_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<SearchHit, sqlx::Error> {
    Ok(SearchHit {
        chunk_id: row.try_get("chunk_id")?,
        doc_id: row.try_get("doc_id")?,
        path: row.try_get("logical_path")?,
        title: row.try_get("title")?,
        line_start: row.try_get("line_start")?,
        line_end: row.try_get("line_end")?,
        quote: row.try_get("body")?,
        score: row.try_get("score")?,
        governance: GovernanceFields {
            tenant_id: row.try_get("tenant_id")?,
            space_id: row.try_get("space_id")?,
            acl_json: row.try_get("acl_json")?,
            acl_principals: row.try_get("acl_principals")?,
            lang: row.try_get("lang")?,
            valid_from: row.try_get("valid_from")?,
            valid_to: row.try_get("valid_to")?,
            superseded_by: row.try_get("superseded_by")?,
            version: row.try_get::<i64, _>("version")? as u32,
        },
        event_id: None,
        exact_fallback: false,
        rerank_score: None,
    })
}

struct EventRow {
    event_id: String,
    chunk_id: String,
    doc_id: String,
    path: String,
    title: String,
    line_start: i64,
    line_end: i64,
    quote: String,
    governance: GovernanceFields,
    score: f64,
}

impl EventRow {
    fn into_hit(self) -> SearchHit {
        SearchHit {
            chunk_id: self.chunk_id,
            doc_id: self.doc_id,
            path: self.path,
            title: self.title,
            line_start: self.line_start,
            line_end: self.line_end,
            quote: self.quote,
            score: self.score,
            governance: self.governance,
            event_id: Some(self.event_id),
            exact_fallback: false,
            rerank_score: None,
        }
    }
}

#[async_trait]
impl Store for SqliteStore {
    fn backend(&self) -> &'static str {
        "sqlite"
    }

    fn dialect(&self) -> SqlDialect {
        SqlDialect::Sqlite
    }

    fn fts_available(&self) -> bool {
        self.fts_available.load(Ordering::Relaxed)
    }

    async fn migrate(&self) -> Result<(), StoreError> {
        sqlx::query("CREATE TABLE IF NOT EXISTS schema_migrations (version TEXT PRIMARY KEY, applied_at TEXT NOT NULL)")
            .execute(&self.pool)
            .await?;
        for &(version, sql) in MIGRATIONS {
            let already: Option<(String,)> =
                sqlx::query_as("SELECT version FROM schema_migrations WHERE version = ?")
                    .bind(version)
                    .fetch_optional(&self.pool)
                    .await?;
            if already.is_some() {
                if version == "0002_fts" {
                    self.fts_available.store(true, Ordering::Relaxed);
                }
                continue;
            }
            let mut tx = self.pool.begin().await?;
            let result = execute_script(&mut tx, sql).await;
            match result {
                Ok(_) => {
                    sqlx::query(
                        "INSERT INTO schema_migrations (version, applied_at) VALUES (?, ?)",
                    )
                    .bind(version)
                    .bind(now_rfc3339())
                    .execute(&mut *tx)
                    .await?;
                    tx.commit().await?;
                    if version == "0002_fts" {
                        self.fts_available.store(true, Ordering::Relaxed);
                    }
                }
                Err(err) if version == "0002_fts" => {
                    // Keep the core schema usable; recall will report
                    // keyword_unavailable rather than pretending FTS worked.
                    tracing::warn!(error = %err, "SQLite FTS5 is unavailable");
                    self.fts_available.store(false, Ordering::Relaxed);
                    drop(tx);
                }
                Err(err) => return Err(StoreError::Database(err)),
            }
        }
        Ok(())
    }

    async fn health(&self) -> Result<(), StoreError> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    async fn ensure_space(&self, space: &SpaceRecord) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO spaces (tenant_id, space_id, name, visibility, default_acl_json,
                owner_principal_id, owner_type, owner_id, status, settings, archived_at,
                deleted_at, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (tenant_id, space_id) DO UPDATE SET
               name = excluded.name,
               visibility = excluded.visibility,
               default_acl_json = excluded.default_acl_json,
               owner_principal_id = excluded.owner_principal_id,
               owner_type = excluded.owner_type,
               owner_id = excluded.owner_id,
               status = excluded.status,
               settings = excluded.settings,
               archived_at = excluded.archived_at,
               deleted_at = excluded.deleted_at",
        )
        .bind(&space.tenant_id)
        .bind(&space.space_id)
        .bind(&space.name)
        .bind(&space.visibility)
        .bind(&space.default_acl_json)
        .bind(&space.owner_principal_id)
        .bind(&space.owner_type)
        .bind(&space.owner_id)
        .bind(&space.status)
        .bind(&space.settings)
        .bind(&space.archived_at)
        .bind(&space.deleted_at)
        .bind(&space.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_space(
        &self,
        tenant_id: &str,
        space_id: &str,
    ) -> Result<Option<SpaceRecord>, StoreError> {
        let row = sqlx::query(
            "SELECT tenant_id, space_id, name, visibility, default_acl_json, owner_principal_id,
                    owner_type, owner_id, status, settings, archived_at, deleted_at, created_at
             FROM spaces WHERE tenant_id = ? AND space_id = ?",
        )
        .bind(tenant_id)
        .bind(space_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| space_from_row(&row))
            .transpose()
            .map_err(StoreError::Database)
    }

    async fn list_spaces(&self, tenant_id: &str) -> Result<Vec<SpaceRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT tenant_id, space_id, name, visibility, default_acl_json, owner_principal_id,
                    owner_type, owner_id, status, settings, archived_at, deleted_at, created_at
             FROM spaces WHERE tenant_id = ? ORDER BY space_id",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(space_from_row)
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(StoreError::Database)
    }

    async fn update_space(&self, space: &SpaceRecord) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE spaces SET name = ?, visibility = ?, default_acl_json = ?,
                owner_principal_id = ?, owner_type = ?, owner_id = ?, status = ?,
                settings = ?, archived_at = ?, deleted_at = ?
             WHERE tenant_id = ? AND space_id = ?",
        )
        .bind(&space.name)
        .bind(&space.visibility)
        .bind(&space.default_acl_json)
        .bind(&space.owner_principal_id)
        .bind(&space.owner_type)
        .bind(&space.owner_id)
        .bind(&space.status)
        .bind(&space.settings)
        .bind(&space.archived_at)
        .bind(&space.deleted_at)
        .bind(&space.tenant_id)
        .bind(&space.space_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_pages(
        &self,
        policy: &Policy,
        limit: i64,
    ) -> Result<Vec<PageSummary>, StoreError> {
        let visibility = policy.visibility("p", self.dialect())?;
        let sql = format!(
            "SELECT p.tenant_id, p.space_id, p.slug, p.lang, p.title, p.summary,
                    p.order_index, p.version, p.updated_at
             FROM pages p
             WHERE {vis}
             ORDER BY p.space_id, p.lang, p.order_index, p.slug
             LIMIT ?",
            vis = visibility.sql
        );
        let mut query = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                String,
                String,
                i64,
                i64,
                String,
            ),
        >(&sql);
        for value in &visibility.binds {
            query = query.bind(value.as_str());
        }
        let rows = query.bind(limit).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(
                |(
                    tenant_id,
                    space_id,
                    slug,
                    lang,
                    title,
                    summary,
                    order_index,
                    version,
                    updated_at,
                )| PageSummary {
                    tenant_id,
                    space_id,
                    slug,
                    lang,
                    title,
                    summary,
                    order_index: order_index as i32,
                    version: version as u32,
                    updated_at,
                },
            )
            .collect())
    }

    async fn get_page(
        &self,
        policy: &Policy,
        space_id: &str,
        slug: &str,
        lang: Option<&str>,
    ) -> Result<Option<PageView>, StoreError> {
        let visibility = policy.visibility("p", self.dialect())?;
        let lang_filter = if lang.is_some() { "AND p.lang = ?" } else { "" };
        let order = if lang.is_some() {
            "ORDER BY p.lang"
        } else {
            "ORDER BY CASE WHEN p.lang = 'zh' THEN 0 ELSE 1 END, p.lang"
        };
        let sql = format!(
            "SELECT p.tenant_id, p.space_id, p.slug, p.lang, p.title, p.summary,
                    p.order_index, p.current_revision_id,
                    p.content_hash, p.acl_json, p.acl_principals, p.valid_from, p.valid_to,
                    p.version, p.superseded_by, p.created_at, p.updated_at,
                    r.revision_id, r.lang AS revision_lang, r.markdown, r.author,
                    r.created_at AS revision_created_at,
                    r.content_hash AS revision_content_hash, r.version AS revision_version,
                    r.source_url, r.source_revision
             FROM pages p
             JOIN revisions r
               ON r.tenant_id = p.tenant_id AND r.revision_id = p.current_revision_id
             WHERE p.tenant_id = ? AND p.space_id = ? AND p.slug = ? {lang_filter} AND {vis}
             {order}
             LIMIT 1",
            vis = visibility.sql
        );
        let mut query = sqlx::query(&sql)
            .bind(&policy.principal.tenant_id)
            .bind(space_id)
            .bind(slug);
        if let Some(lang) = lang {
            query = query.bind(lang);
        }
        for value in &visibility.binds {
            query = query.bind(value.as_str());
        }
        let row = query.fetch_optional(&self.pool).await?;
        row.map(|row| page_view_from_row(&row))
            .transpose()
            .map_err(StoreError::Database)
    }

    async fn get_page_unfiltered(
        &self,
        tenant_id: &str,
        space_id: &str,
        slug: &str,
        lang: Option<&str>,
    ) -> Result<Option<PageView>, StoreError> {
        let lang_filter = if lang.is_some() { "AND p.lang = ?" } else { "" };
        let order = if lang.is_some() {
            "ORDER BY p.lang"
        } else {
            "ORDER BY CASE WHEN p.lang = 'zh' THEN 0 ELSE 1 END, p.lang"
        };
        let sql = format!(
            "SELECT p.tenant_id, p.space_id, p.slug, p.lang, p.title, p.summary,
                    p.order_index, p.current_revision_id,
                    p.content_hash, p.acl_json, p.acl_principals, p.valid_from, p.valid_to,
                    p.version, p.superseded_by, p.created_at, p.updated_at,
                    r.revision_id, r.lang AS revision_lang, r.markdown, r.author,
                    r.created_at AS revision_created_at,
                    r.content_hash AS revision_content_hash, r.version AS revision_version,
                    r.source_url, r.source_revision
             FROM pages p
             JOIN revisions r
               ON r.tenant_id = p.tenant_id AND r.revision_id = p.current_revision_id
             WHERE p.tenant_id = ? AND p.space_id = ? AND p.slug = ? {lang_filter}
             {order}
             LIMIT 1"
        );
        let mut query = sqlx::query(&sql).bind(tenant_id).bind(space_id).bind(slug);
        if let Some(lang) = lang {
            query = query.bind(lang);
        }
        let row = query.fetch_optional(&self.pool).await?;
        row.map(|row| page_view_from_row(&row))
            .transpose()
            .map_err(StoreError::Database)
    }

    async fn save_page(
        &self,
        page: &PageRecord,
        revision: &RevisionRecord,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "UPDATE revisions SET superseded_by = ?, valid_to = ?
             WHERE tenant_id = ? AND space_id = ? AND lang = ? AND slug = ?
               AND revision_id <> ? AND superseded_by IS NULL",
        )
        .bind(&revision.revision_id)
        .bind(&revision.valid_from)
        .bind(&revision.tenant_id)
        .bind(&revision.space_id)
        .bind(&revision.lang)
        .bind(&revision.slug)
        .bind(&revision.revision_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO revisions (tenant_id, revision_id, space_id, slug, lang, title, markdown,
                content_hash, author, created_at, version, acl_json, acl_principals,
                valid_from, valid_to, superseded_by, source_url, source_revision)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (tenant_id, revision_id) DO NOTHING",
        )
        .bind(&revision.tenant_id)
        .bind(&revision.revision_id)
        .bind(&revision.space_id)
        .bind(&revision.slug)
        .bind(&revision.lang)
        .bind(&revision.title)
        .bind(&revision.markdown)
        .bind(&revision.content_hash)
        .bind(&revision.author)
        .bind(&revision.created_at)
        .bind(revision.version as i64)
        .bind(&revision.acl_json)
        .bind(&revision.acl_principals)
        .bind(&revision.valid_from)
        .bind(&revision.valid_to)
        .bind(&revision.superseded_by)
        .bind(&revision.source_url)
        .bind(&revision.source_revision)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO pages (tenant_id, space_id, slug, lang, title, summary, order_index,
                current_revision_id, content_hash, acl_json, acl_principals, valid_from,
                valid_to, version, superseded_by, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (tenant_id, space_id, lang, slug) DO UPDATE SET
               title = excluded.title,
               summary = excluded.summary,
               order_index = excluded.order_index,
               current_revision_id = excluded.current_revision_id,
               content_hash = excluded.content_hash,
               acl_json = excluded.acl_json,
               acl_principals = excluded.acl_principals,
               valid_from = excluded.valid_from,
               valid_to = excluded.valid_to,
               version = excluded.version,
               superseded_by = excluded.superseded_by,
               updated_at = excluded.updated_at",
        )
        .bind(&page.tenant_id)
        .bind(&page.space_id)
        .bind(&page.slug)
        .bind(&page.lang)
        .bind(&page.title)
        .bind(&page.summary)
        .bind(page.order_index as i64)
        .bind(&page.current_revision_id)
        .bind(&page.content_hash)
        .bind(&page.acl_json)
        .bind(&page.acl_principals)
        .bind(&page.valid_from)
        .bind(&page.valid_to)
        .bind(page.version as i64)
        .bind(&page.superseded_by)
        .bind(&page.created_at)
        .bind(&page.updated_at)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn list_revisions(
        &self,
        tenant_id: &str,
        space_id: &str,
        slug: &str,
        lang: Option<&str>,
    ) -> Result<Vec<RevisionSummary>, StoreError> {
        let lang_filter = if lang.is_some() { " AND lang = ?" } else { "" };
        let sql = format!(
            "SELECT revision_id, version, author, created_at, content_hash, valid_from
             FROM revisions WHERE tenant_id = ? AND space_id = ? AND slug = ?{lang_filter}
             ORDER BY version DESC"
        );
        let mut query = sqlx::query_as::<_, (String, i64, String, String, String, String)>(&sql)
            .bind(tenant_id)
            .bind(space_id)
            .bind(slug);
        if let Some(lang) = lang {
            query = query.bind(lang);
        }
        let rows = query.fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(
                |(revision_id, version, author, created_at, content_hash, valid_from)| {
                    RevisionSummary {
                        revision_id,
                        version: version as u32,
                        author,
                        created_at,
                        content_hash,
                        valid_from,
                    }
                },
            )
            .collect())
    }

    async fn upsert_document(
        &self,
        doc: &DocumentRecord,
        chunks: &[ChunkRecord],
        expected_doc_id: Option<&str>,
    ) -> Result<UpsertOutcome, StoreError> {
        if let Some(expected) = expected_doc_id {
            if expected != doc.doc_id {
                return Err(StoreError::Invalid(format!(
                    "doc_id mismatch for {}: expected {expected}, computed {}",
                    doc.logical_path, doc.doc_id
                )));
            }
        }
        let mut tx = self.pool.begin().await?;

        let existing: Option<(String,)> =
            sqlx::query_as("SELECT doc_id FROM documents WHERE tenant_id = ? AND doc_id = ?")
                .bind(&doc.tenant_id)
                .bind(&doc.doc_id)
                .fetch_optional(&mut *tx)
                .await?;
        if existing.is_some() {
            tx.commit().await?;
            return Ok(UpsertOutcome::Skipped("doc_id already indexed".to_string()));
        }

        let mut version = doc.version.max(1);
        let mut status = doc.status.clone();
        let mut superseded_by = doc.superseded_by.clone();
        let mut valid_to = doc.valid_to.clone();

        if doc.status == "active" {
            let old: Option<(String, i64, String)> = sqlx::query_as(
                "SELECT doc_id, version, content_hash FROM documents
                 WHERE tenant_id = ? AND space_id = ? AND logical_path = ? AND status = 'active'
                 LIMIT 1",
            )
            .bind(&doc.tenant_id)
            .bind(&doc.space_id)
            .bind(&doc.logical_path)
            .fetch_optional(&mut *tx)
            .await?;
            if let Some((old_id, old_version, old_hash)) = old {
                if old_hash == doc.content_hash {
                    tx.commit().await?;
                    return Ok(UpsertOutcome::Skipped("content unchanged".to_string()));
                }
                let now = now_rfc3339();
                sqlx::query(
                    "UPDATE documents SET status = 'superseded', superseded_by = ?,
                        valid_to = ?, updated_at = ? WHERE tenant_id = ? AND doc_id = ?",
                )
                .bind(&doc.doc_id)
                .bind(&now)
                .bind(&now)
                .bind(&doc.tenant_id)
                .bind(&old_id)
                .execute(&mut *tx)
                .await?;
                sqlx::query(
                    "UPDATE chunks SET superseded_by = ?, valid_to = ?
                     WHERE tenant_id = ? AND doc_id = ?",
                )
                .bind(&doc.doc_id)
                .bind(&now)
                .bind(&doc.tenant_id)
                .bind(&old_id)
                .execute(&mut *tx)
                .await?;
                version = version.max((old_version as u32).saturating_add(1));
            }
            status = "active".to_string();
            superseded_by = None;
        }

        if status != "active" && valid_to.is_none() {
            valid_to = Some(now_rfc3339());
        }
        let chunk_superseded_by = if status == "active" {
            superseded_by.clone()
        } else {
            Some(superseded_by.clone().unwrap_or_else(|| doc.doc_id.clone()))
        };
        let chunk_valid_to = chunks
            .iter()
            .find_map(|chunk| chunk.valid_to.clone())
            .or_else(|| valid_to.clone());

        sqlx::query(
            "INSERT INTO documents (tenant_id, doc_id, space_id, logical_path, source_uri,
                title, mime, raw_hash, content_hash, content, acl_json, acl_principals, lang,
                valid_from, valid_to, version, superseded_by, status, source_role,
                canonicalization_version, extractor_version, chunker_version,
                created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&doc.tenant_id)
        .bind(&doc.doc_id)
        .bind(&doc.space_id)
        .bind(&doc.logical_path)
        .bind(&doc.source_uri)
        .bind(&doc.title)
        .bind(&doc.mime)
        .bind(&doc.raw_hash)
        .bind(&doc.content_hash)
        .bind(&doc.content)
        .bind(&doc.acl_json)
        .bind(&doc.acl_principals)
        .bind(&doc.lang)
        .bind(&doc.valid_from)
        .bind(&valid_to)
        .bind(version as i64)
        .bind(&superseded_by)
        .bind(&status)
        .bind(&doc.source_role)
        .bind(&doc.canonicalization_version)
        .bind(&doc.extractor_version)
        .bind(&doc.chunker_version)
        .bind(&doc.created_at)
        .bind(&doc.updated_at)
        .execute(&mut *tx)
        .await?;

        for chunk in chunks {
            let embedding_json = match &chunk.embedding {
                Some(vector) => Some(serde_json::to_string(vector)?),
                None => None,
            };
            sqlx::query(
                "INSERT INTO chunks (tenant_id, chunk_id, doc_id, space_id, seq, body,
                    line_start, line_end, content_hash, acl_json, acl_principals, lang,
                    valid_from, valid_to, version, superseded_by, embedding_json)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&chunk.tenant_id)
            .bind(&chunk.chunk_id)
            .bind(&chunk.doc_id)
            .bind(&chunk.space_id)
            .bind(chunk.seq as i64)
            .bind(&chunk.text)
            .bind(chunk.line_start)
            .bind(chunk.line_end)
            .bind(&chunk.content_hash)
            .bind(&chunk.acl_json)
            .bind(&chunk.acl_principals)
            .bind(&chunk.lang)
            .bind(&chunk.valid_from)
            .bind(&chunk_valid_to)
            .bind(version as i64)
            .bind(&chunk_superseded_by)
            .bind(embedding_json)
            .execute(&mut *tx)
            .await?;

            if self.fts_available.load(Ordering::Relaxed) {
                let result = sqlx::query(
                    "INSERT INTO chunks_fts (chunk_id, doc_id, tenant_id, body) VALUES (?, ?, ?, ?)",
                )
                .bind(&chunk.chunk_id)
                .bind(&chunk.doc_id)
                .bind(&chunk.tenant_id)
                .bind(&chunk.text)
                .execute(&mut *tx)
                .await;
                if let Err(err) = result {
                    tracing::warn!(error = %err, "FTS insert failed; disabling keyword index");
                    self.fts_available.store(false, Ordering::Relaxed);
                }
            }
        }

        tx.commit().await?;
        Ok(UpsertOutcome::Imported)
    }

    async fn list_documents(
        &self,
        tenant_id: &str,
        space_id: Option<&str>,
    ) -> Result<Vec<DocumentRecord>, StoreError> {
        let rows = match space_id {
            Some(space) => {
                sqlx::query(
                    "SELECT * FROM documents WHERE tenant_id = ? AND space_id = ?
                     ORDER BY logical_path, version",
                )
                .bind(tenant_id)
                .bind(space)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query(
                    "SELECT * FROM documents WHERE tenant_id = ?
                     ORDER BY logical_path, version",
                )
                .bind(tenant_id)
                .fetch_all(&self.pool)
                .await?
            }
        };
        rows.iter()
            .map(document_from_row)
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(StoreError::Database)
    }

    async fn search_keyword(
        &self,
        policy: &Policy,
        query_text: &str,
        limit: i64,
    ) -> Result<Vec<SearchHit>, StoreError> {
        if !self.fts_available() {
            return Err(StoreError::FtsUnavailable);
        }
        let fts_query = fts5_query(query_text);
        if fts_query.is_empty() {
            return Ok(Vec::new());
        }
        let visibility = policy.visibility("c", self.dialect())?;
        let sql = format!(
            "SELECT c.chunk_id, c.doc_id, d.logical_path, d.title, c.line_start, c.line_end,
                    c.body, bm25(chunks_fts) AS score, c.tenant_id, c.space_id,
                    c.valid_from, c.valid_to, c.superseded_by, c.acl_json,
                    c.acl_principals, c.lang, c.version
             FROM chunks_fts
             JOIN chunks c
               ON c.tenant_id = chunks_fts.tenant_id AND c.chunk_id = chunks_fts.chunk_id
             JOIN documents d
               ON d.tenant_id = c.tenant_id AND d.doc_id = c.doc_id
             WHERE chunks_fts MATCH ? AND {vis}
             ORDER BY score ASC, c.seq ASC
             LIMIT ?",
            vis = visibility.sql
        );
        let mut query = sqlx::query(&sql).bind(fts_query);
        for value in &visibility.binds {
            query = query.bind(value.as_str());
        }
        let rows = query.bind(limit).fetch_all(&self.pool).await?;
        if !rows.is_empty() {
            return rows
                .iter()
                .map(hit_from_row)
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(StoreError::Database);
        }

        // Fallback for short queries and CJK strings that the trigram
        // tokenizer cannot tokenize. Terms are explicitly marked, never
        // silently mixed into BM25 ranking.
        let terms = query_terms(query_text);
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let term_clauses = (0..terms.len())
            .map(|_| "instr(c.body, ?) > 0")
            .collect::<Vec<_>>()
            .join(" OR ");
        let fallback_sql = format!(
            "SELECT c.chunk_id, c.doc_id, d.logical_path, d.title, c.line_start, c.line_end,
                    c.body, 0.0 AS score, c.tenant_id, c.space_id,
                    c.valid_from, c.valid_to, c.superseded_by, c.acl_json,
                    c.acl_principals, c.lang, c.version
             FROM chunks c
             JOIN documents d
               ON d.tenant_id = c.tenant_id AND d.doc_id = c.doc_id
             WHERE ({terms}) AND {vis}
             ORDER BY c.tenant_id, c.doc_id, c.seq
             LIMIT ?",
            terms = term_clauses,
            vis = visibility.sql
        );
        let mut query = sqlx::query(&fallback_sql);
        for term in &terms {
            query = query.bind(term.as_str());
        }
        for value in &visibility.binds {
            query = query.bind(value.as_str());
        }
        let rows = query.bind(limit).fetch_all(&self.pool).await?;
        rows.iter()
            .map(|row| {
                let mut hit = hit_from_row(row)?;
                hit.exact_fallback = true;
                Ok(hit)
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(StoreError::Database)
    }

    async fn fetch_embedding_candidates(
        &self,
        policy: &Policy,
        limit: i64,
    ) -> Result<Vec<Candidate>, StoreError> {
        let visibility = policy.visibility("c", self.dialect())?;
        let sql = format!(
            "SELECT c.chunk_id, c.doc_id, d.logical_path, d.title, c.line_start, c.line_end,
                    c.body, c.embedding_json, c.tenant_id, c.space_id, c.valid_from,
                    c.valid_to, c.superseded_by, c.acl_json, c.acl_principals, c.lang, c.version
             FROM chunks c
             JOIN documents d
               ON d.tenant_id = c.tenant_id AND d.doc_id = c.doc_id
             WHERE c.embedding_json IS NOT NULL AND {vis}
             ORDER BY c.tenant_id, c.doc_id, c.seq
             LIMIT ?",
            vis = visibility.sql
        );
        let mut query = sqlx::query(&sql);
        for value in &visibility.binds {
            query = query.bind(value.as_str());
        }
        let rows = query.bind(limit).fetch_all(&self.pool).await?;
        let mut candidates = Vec::new();
        for row in rows {
            let embedding_json: String = row.try_get("embedding_json")?;
            let embedding: Vec<f32> = match serde_json::from_str(&embedding_json) {
                Ok(value) => value,
                Err(_) => continue,
            };
            candidates.push(Candidate {
                chunk_id: row.try_get("chunk_id")?,
                doc_id: row.try_get("doc_id")?,
                path: row.try_get("logical_path")?,
                title: row.try_get("title")?,
                line_start: row.try_get("line_start")?,
                line_end: row.try_get("line_end")?,
                quote: row.try_get("body")?,
                embedding,
                governance: GovernanceFields {
                    tenant_id: row.try_get("tenant_id")?,
                    space_id: row.try_get("space_id")?,
                    acl_json: row.try_get("acl_json")?,
                    acl_principals: row.try_get("acl_principals")?,
                    lang: row.try_get("lang")?,
                    valid_from: row.try_get("valid_from")?,
                    valid_to: row.try_get("valid_to")?,
                    superseded_by: row.try_get("superseded_by")?,
                    version: row.try_get::<i64, _>("version")? as u32,
                },
            });
        }
        Ok(candidates)
    }

    async fn has_events(&self, policy: &Policy) -> Result<bool, StoreError> {
        let visibility = policy.visibility_from("e", self.dialect(), 1)?;
        let sql = format!("SELECT 1 FROM events e WHERE {} LIMIT 1", visibility.sql);
        let mut query = sqlx::query_scalar::<_, i64>(&sql);
        for value in &visibility.binds {
            query = query.bind(value.as_str());
        }
        Ok(query.fetch_optional(&self.pool).await?.is_some())
    }

    async fn has_current_event(
        &self,
        tenant_id: &str,
        space_id: &str,
        chunk_id: &str,
        extractor_version: &str,
        extraction_model: &str,
    ) -> Result<bool, StoreError> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT event_id FROM events
             WHERE tenant_id = ? AND space_id = ? AND chunk_id = ? AND extractor_version = ?
               AND extraction_model = ? AND superseded_by IS NULL
             LIMIT 1",
        )
        .bind(tenant_id)
        .bind(space_id)
        .bind(chunk_id)
        .bind(extractor_version)
        .bind(extraction_model)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.is_some())
    }

    async fn upsert_event(
        &self,
        event: &EventRecord,
        entities: &[EntityRecord],
        links: &[EventEntityRecord],
    ) -> Result<EventUpsertOutcome, StoreError> {
        let mut tx = self.pool.begin().await?;
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT event_id FROM events WHERE event_id = ?")
                .bind(&event.event_id)
                .fetch_optional(&mut *tx)
                .await?;
        if existing.is_some() {
            tx.commit().await?;
            return Ok(EventUpsertOutcome::Skipped(
                "event already extracted for this version/model".to_string(),
            ));
        }
        let max_version: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version), 0) FROM events WHERE tenant_id = ? AND space_id = ? AND chunk_id = ?",
        )
        .bind(&event.tenant_id)
        .bind(&event.space_id)
        .bind(&event.chunk_id)
        .fetch_one(&mut *tx)
        .await?;
        let version = (max_version as u32).saturating_add(1).max(1);
        let now = now_rfc3339();
        sqlx::query(
            "UPDATE events SET superseded_by = ?, valid_to = ?
             WHERE tenant_id = ? AND space_id = ? AND chunk_id = ? AND superseded_by IS NULL",
        )
        .bind(&event.event_id)
        .bind(&now)
        .bind(&event.tenant_id)
        .bind(&event.space_id)
        .bind(&event.chunk_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO events (event_id, tenant_id, space_id, doc_id, chunk_id, seq, text,
                content_hash, extractor_version, extraction_model, acl_json, acl_principals,
                lang, valid_from, valid_to, version, superseded_by, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&event.event_id)
        .bind(&event.tenant_id)
        .bind(&event.space_id)
        .bind(&event.doc_id)
        .bind(&event.chunk_id)
        .bind(event.seq as i64)
        .bind(&event.text)
        .bind(&event.content_hash)
        .bind(&event.extractor_version)
        .bind(&event.extraction_model)
        .bind(&event.acl_json)
        .bind(&event.acl_principals)
        .bind(&event.lang)
        .bind(&event.valid_from)
        .bind(&event.valid_to)
        .bind(version as i64)
        .bind(&event.superseded_by)
        .bind(&event.created_at)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM events_fts WHERE event_id = ?")
            .bind(&event.event_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO events_fts (event_id, tenant_id, space_id, body)
             VALUES (?, ?, ?, ?)",
        )
        .bind(&event.event_id)
        .bind(&event.tenant_id)
        .bind(&event.space_id)
        .bind(&event.text)
        .execute(&mut *tx)
        .await?;
        for entity in entities {
            sqlx::query(
                "INSERT INTO entities (entity_id, tenant_id, space_id, name,
                    normalized_name, type, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT (tenant_id, space_id, normalized_name, type)
                 DO UPDATE SET name = excluded.name",
            )
            .bind(&entity.entity_id)
            .bind(&entity.tenant_id)
            .bind(&entity.space_id)
            .bind(&entity.name)
            .bind(&entity.normalized_name)
            .bind(&entity.entity_type)
            .bind(&entity.created_at)
            .execute(&mut *tx)
            .await?;
        }
        for link in links {
            sqlx::query(
                "INSERT INTO event_entities (event_id, entity_id, weight, tenant_id, space_id)
                 VALUES (?, ?, ?, ?, ?)
                 ON CONFLICT (event_id, entity_id)
                 DO UPDATE SET weight = excluded.weight",
            )
            .bind(&link.event_id)
            .bind(&link.entity_id)
            .bind(link.weight)
            .bind(&link.tenant_id)
            .bind(&link.space_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(EventUpsertOutcome::Inserted)
    }

    async fn search_events(
        &self,
        policy: &Policy,
        params: EventSearchParams<'_>,
    ) -> Result<Vec<SearchHit>, StoreError> {
        let EventSearchParams {
            query,
            depth,
            max_seeds,
            max_events,
            text_weight,
            limit,
            timeout_ms: _,
        } = params;
        let seeds = self
            .event_seed_entities(policy, query, max_seeds.max(1))
            .await?;
        if seeds.is_empty() {
            return Ok(Vec::new());
        }
        let fts_query = crate::util::fts5_query(query);
        let mut rows = self
            .event_expand(policy, &seeds, &fts_query, text_weight, max_events.max(1))
            .await?;
        if depth >= 2 && !rows.is_empty() {
            let event_ids: Vec<String> = rows.iter().map(|row| row.event_id.clone()).collect();
            let second = self
                .event_entities_for_events(policy, &event_ids, max_events.max(2) / 2)
                .await?;
            if !second.is_empty() {
                let mut second_rows = self
                    .event_expand(
                        policy,
                        &second,
                        &fts_query,
                        text_weight * 0.5,
                        max_events.max(1),
                    )
                    .await?;
                for row in &mut second_rows {
                    row.score *= 0.5;
                }
                rows.extend(second_rows);
            }
        }

        let mut by_event: HashMap<String, EventRow> = HashMap::new();
        for row in rows {
            match by_event.entry(row.event_id.clone()) {
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    if row.score > entry.get().score {
                        entry.get_mut().score = row.score;
                    }
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(row);
                }
            }
        }
        let mut by_chunk: HashMap<String, SearchHit> = HashMap::new();
        for row in by_event.into_values() {
            let hit = row.into_hit();
            by_chunk
                .entry(hit.chunk_id.clone())
                .and_modify(|existing| existing.score += hit.score)
                .or_insert(hit);
        }
        let mut hits: Vec<SearchHit> = by_chunk.into_values().collect();
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.chunk_id.cmp(&b.chunk_id))
        });
        hits.truncate(limit.max(1) as usize);
        Ok(hits)
    }

    async fn get_seed_meta(
        &self,
        tenant_id: &str,
        space_id: &str,
    ) -> Result<Option<SeedMeta>, StoreError> {
        let row = sqlx::query(
            "SELECT tenant_id, space_id, seed_version, source_manifest_hash,
                    applied_at, page_count, revision_count
             FROM seed_meta WHERE tenant_id = ? AND space_id = ?",
        )
        .bind(tenant_id)
        .bind(space_id)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            None => Ok(None),
            Some(row) => Ok(Some(SeedMeta {
                tenant_id: row.try_get("tenant_id")?,
                space_id: row.try_get("space_id")?,
                seed_version: row.try_get("seed_version")?,
                source_manifest_hash: row.try_get("source_manifest_hash")?,
                applied_at: row.try_get("applied_at")?,
                page_count: row.try_get::<i64, _>("page_count")? as u32,
                revision_count: row.try_get::<i64, _>("revision_count")? as u32,
            })),
        }
    }

    async fn upsert_seed_meta(&self, meta: &SeedMeta) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO seed_meta (tenant_id, space_id, seed_version,
                source_manifest_hash, applied_at, page_count, revision_count)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (tenant_id, space_id) DO UPDATE SET
               seed_version = excluded.seed_version,
               source_manifest_hash = excluded.source_manifest_hash,
               applied_at = excluded.applied_at,
               page_count = excluded.page_count,
               revision_count = excluded.revision_count",
        )
        .bind(&meta.tenant_id)
        .bind(&meta.space_id)
        .bind(&meta.seed_version)
        .bind(&meta.source_manifest_hash)
        .bind(&meta.applied_at)
        .bind(meta.page_count as i64)
        .bind(meta.revision_count as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn is_database_empty(&self, tenant_id: &str) -> Result<bool, StoreError> {
        let empty: i64 = sqlx::query_scalar(
            "SELECT CASE WHEN EXISTS(SELECT 1 FROM pages WHERE tenant_id = ?)
                           OR EXISTS(SELECT 1 FROM documents WHERE tenant_id = ?)
                         THEN 0 ELSE 1 END",
        )
        .bind(tenant_id)
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(empty != 0)
    }

    async fn add_user(
        &self,
        tenant_id: &str,
        username: &str,
        password_hash: &str,
    ) -> Result<UserRecord, StoreError> {
        self.add_user_with_role(tenant_id, username, password_hash, UserRole::User)
            .await
    }

    async fn add_user_with_role(
        &self,
        tenant_id: &str,
        username: &str,
        password_hash: &str,
        role: UserRole,
    ) -> Result<UserRecord, StoreError> {
        let record = UserRecord {
            user_id: format!("usr_{}", crate::util::random_hex(8)),
            tenant_id: tenant_id.to_string(),
            username: username.to_string(),
            password_hash: password_hash.to_string(),
            display_name: username.to_string(),
            created_at: now_rfc3339(),
            disabled: false,
            role,
            email: None,
            external_id: None,
            source: "local".to_string(),
            last_sso_at: None,
            last_login_at: None,
        };
        sqlx::query(
            "INSERT INTO users (tenant_id, user_id, username, password_hash, display_name,
                created_at, disabled, role)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&record.tenant_id)
        .bind(&record.user_id)
        .bind(&record.username)
        .bind(&record.password_hash)
        .bind(&record.display_name)
        .bind(&record.created_at)
        .bind(0i64)
        .bind(record.role.as_str())
        .execute(&self.pool)
        .await
        .map_err(|err| match &err {
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                StoreError::Conflict(format!("user {username:?} already exists"))
            }
            _ => StoreError::Database(err),
        })?;
        Ok(record)
    }

    async fn set_user_role(
        &self,
        tenant_id: &str,
        username: &str,
        role: UserRole,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query("UPDATE users SET role = ? WHERE tenant_id = ? AND username = ?")
            .bind(role.as_str())
            .bind(tenant_id)
            .bind(username)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_users(&self, tenant_id: &str) -> Result<Vec<UserRecord>, StoreError> {
        let rows = sqlx::query("SELECT * FROM users WHERE tenant_id = ? ORDER BY username")
            .bind(tenant_id)
            .fetch_all(&self.pool)
            .await?;
        rows.iter()
            .map(user_from_row)
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(StoreError::Database)
    }

    async fn get_user(
        &self,
        tenant_id: &str,
        username: &str,
    ) -> Result<Option<UserRecord>, StoreError> {
        let row = sqlx::query("SELECT * FROM users WHERE tenant_id = ? AND username = ?")
            .bind(tenant_id)
            .bind(username)
            .fetch_optional(&self.pool)
            .await?;
        row.map(|row| user_from_row(&row))
            .transpose()
            .map_err(StoreError::Database)
    }

    async fn get_user_by_id(
        &self,
        tenant_id: &str,
        user_id: &str,
    ) -> Result<Option<UserRecord>, StoreError> {
        let row = sqlx::query("SELECT * FROM users WHERE tenant_id = ? AND user_id = ?")
            .bind(tenant_id)
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(|row| user_from_row(&row))
            .transpose()
            .map_err(StoreError::Database)
    }

    async fn create_session(&self, session: &SessionRecord) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO sessions (session_hash, tenant_id, user_id, created_at, expires_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&session.session_hash)
        .bind(&session.tenant_id)
        .bind(&session.user_id)
        .bind(&session.created_at)
        .bind(&session.expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_session(&self, session_hash: &str) -> Result<Option<SessionRecord>, StoreError> {
        let row = sqlx::query(
            "SELECT session_hash, tenant_id, user_id, created_at, expires_at
             FROM sessions WHERE session_hash = ?",
        )
        .bind(session_hash)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            None => Ok(None),
            Some(row) => Ok(Some(SessionRecord {
                session_hash: row.try_get("session_hash")?,
                tenant_id: row.try_get("tenant_id")?,
                user_id: row.try_get("user_id")?,
                created_at: row.try_get("created_at")?,
                expires_at: row.try_get("expires_at")?,
            })),
        }
    }

    async fn delete_session(&self, session_hash: &str) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM sessions WHERE session_hash = ?")
            .bind(session_hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn create_token(&self, token: &TokenRecord) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO api_tokens (token_hash, token_id, tenant_id, user_id, scope,
                kind, label, created_at, expires_at, revoked_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&token.token_hash)
        .bind(&token.token_id)
        .bind(&token.tenant_id)
        .bind(&token.user_id)
        .bind(token.scope.as_str())
        .bind(token_kind_str(token.kind))
        .bind(&token.label)
        .bind(&token.created_at)
        .bind(&token.expires_at)
        .bind(&token.revoked_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_token(&self, token_hash: &str) -> Result<Option<TokenRecord>, StoreError> {
        let row = sqlx::query(
            "SELECT token_hash, token_id, tenant_id, user_id, scope, kind, label,
                    created_at, expires_at, revoked_at
             FROM api_tokens WHERE token_hash = ?",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| token_from_row(&row))
            .transpose()
            .map_err(StoreError::Database)
    }

    async fn list_tokens(
        &self,
        tenant_id: &str,
        user_id: &str,
    ) -> Result<Vec<TokenRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT token_hash, token_id, tenant_id, user_id, scope, kind, label,
                    created_at, expires_at, revoked_at
             FROM api_tokens WHERE tenant_id = ? AND user_id = ?
             ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(token_from_row)
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(StoreError::Database)
    }

    async fn revoke_token(&self, token_id: &str) -> Result<bool, StoreError> {
        let result = sqlx::query(
            "UPDATE api_tokens SET revoked_at = ? WHERE token_id = ? AND revoked_at IS NULL",
        )
        .bind(now_rfc3339())
        .bind(token_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn create_group(&self, group: &GroupRecord) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO groups (group_id, tenant_id, name, description, created_by,
                status, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&group.group_id)
        .bind(&group.tenant_id)
        .bind(&group.name)
        .bind(&group.description)
        .bind(&group.created_by)
        .bind(&group.status)
        .bind(&group.created_at)
        .bind(&group.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|err| match &err {
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                StoreError::Conflict(format!("group {:?} already exists", group.name))
            }
            _ => StoreError::Database(err),
        })?;
        Ok(())
    }

    async fn get_group(
        &self,
        tenant_id: &str,
        group_id: &str,
    ) -> Result<Option<GroupRecord>, StoreError> {
        let row = sqlx::query(
            "SELECT group_id, tenant_id, name, description, created_by, status,
                    created_at, updated_at
             FROM groups WHERE tenant_id = ? AND group_id = ?",
        )
        .bind(tenant_id)
        .bind(group_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| group_from_row(&row))
            .transpose()
            .map_err(StoreError::Database)
    }

    async fn list_groups(&self, tenant_id: &str) -> Result<Vec<GroupRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT group_id, tenant_id, name, description, created_by, status,
                    created_at, updated_at
             FROM groups WHERE tenant_id = ? ORDER BY name",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(group_from_row)
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(StoreError::Database)
    }

    async fn update_group(&self, group: &GroupRecord) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE groups SET name = ?, description = ?, status = ?, updated_at = ?
             WHERE tenant_id = ? AND group_id = ?",
        )
        .bind(&group.name)
        .bind(&group.description)
        .bind(&group.status)
        .bind(&group.updated_at)
        .bind(&group.tenant_id)
        .bind(&group.group_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn add_group_member(&self, member: &GroupMemberRecord) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO group_members (group_id, principal_id, role, added_by, created_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (group_id, principal_id) DO UPDATE SET
               role = excluded.role, added_by = excluded.added_by, created_at = excluded.created_at",
        )
        .bind(&member.group_id)
        .bind(&member.principal_id)
        .bind(member.role.as_str())
        .bind(&member.added_by)
        .bind(&member.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_group_member(
        &self,
        group_id: &str,
        principal_id: &str,
    ) -> Result<Option<GroupMemberRecord>, StoreError> {
        let row = sqlx::query(
            "SELECT group_id, principal_id, role, added_by, created_at
             FROM group_members WHERE group_id = ? AND principal_id = ?",
        )
        .bind(group_id)
        .bind(principal_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| group_member_from_row(&row))
            .transpose()
            .map_err(StoreError::Database)
    }

    async fn list_group_members(
        &self,
        group_id: &str,
    ) -> Result<Vec<GroupMemberRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT group_id, principal_id, role, added_by, created_at
             FROM group_members WHERE group_id = ? ORDER BY principal_id",
        )
        .bind(group_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(group_member_from_row)
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(StoreError::Database)
    }

    async fn list_group_memberships(
        &self,
        tenant_id: &str,
        principal_id: &str,
    ) -> Result<Vec<GroupMembership>, StoreError> {
        let rows = sqlx::query(
            "SELECT g.group_id, g.tenant_id, g.name, g.description, g.created_by,
                    g.status, g.created_at AS group_created_at, g.updated_at,
                    gm.principal_id, gm.role, gm.added_by, gm.created_at AS member_created_at
             FROM groups g JOIN group_members gm ON gm.group_id = g.group_id
             WHERE g.tenant_id = ? AND gm.principal_id = ? AND g.status = 'active'
             ORDER BY g.name",
        )
        .bind(tenant_id)
        .bind(principal_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(GroupMembership {
                    group: group_from_row(row)?,
                    member: GroupMemberRecord {
                        group_id: row.try_get("group_id")?,
                        principal_id: row.try_get("principal_id")?,
                        role: GroupRole::parse(&row.try_get::<String, _>("role")?)
                            .map_err(decode_error)?,
                        added_by: row.try_get("added_by")?,
                        created_at: row.try_get("member_created_at")?,
                    },
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(StoreError::Database)
    }

    async fn update_group_member_role(
        &self,
        group_id: &str,
        principal_id: &str,
        role: GroupRole,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE group_members SET role = ? WHERE group_id = ? AND principal_id = ?")
            .bind(role.as_str())
            .bind(group_id)
            .bind(principal_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn remove_group_member(
        &self,
        group_id: &str,
        principal_id: &str,
    ) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM group_members WHERE group_id = ? AND principal_id = ?")
            .bind(group_id)
            .bind(principal_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn count_group_admins(&self, group_id: &str) -> Result<i64, StoreError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM group_members WHERE group_id = ? AND role = 'group_admin'",
        )
        .bind(group_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    async fn get_space_member(
        &self,
        space_id: &str,
        principal_type: PrincipalType,
        principal_id: &str,
    ) -> Result<Option<SpaceMemberRecord>, StoreError> {
        let row = sqlx::query(
            "SELECT space_id, principal_type, principal_id, role, added_by, created_at
             FROM space_members
             WHERE space_id = ? AND principal_type = ? AND principal_id = ?",
        )
        .bind(space_id)
        .bind(principal_type.as_str())
        .bind(principal_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| space_member_from_row(&row))
            .transpose()
            .map_err(StoreError::Database)
    }

    async fn list_space_members(
        &self,
        space_id: &str,
    ) -> Result<Vec<SpaceMemberRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT space_id, principal_type, principal_id, role, added_by, created_at
             FROM space_members WHERE space_id = ? ORDER BY principal_type, principal_id",
        )
        .bind(space_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(space_member_from_row)
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(StoreError::Database)
    }

    async fn upsert_space_member(&self, member: &SpaceMemberRecord) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO space_members (space_id, principal_type, principal_id, role,
                added_by, created_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT (space_id, principal_type, principal_id) DO UPDATE SET
               role = excluded.role, added_by = excluded.added_by, created_at = excluded.created_at",
        )
        .bind(&member.space_id)
        .bind(member.principal_type.as_str())
        .bind(&member.principal_id)
        .bind(member.role.as_str())
        .bind(&member.added_by)
        .bind(&member.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn remove_space_member(
        &self,
        space_id: &str,
        principal_type: PrincipalType,
        principal_id: &str,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "DELETE FROM space_members
             WHERE space_id = ? AND principal_type = ? AND principal_id = ?",
        )
        .bind(space_id)
        .bind(principal_type.as_str())
        .bind(principal_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn count_space_owners(&self, space_id: &str) -> Result<i64, StoreError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM space_members
             WHERE space_id = ? AND role = 'space_owner'",
        )
        .bind(space_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    async fn get_quota(
        &self,
        tenant_id: &str,
        scope_type: QuotaScope,
        scope_id: &str,
    ) -> Result<Option<QuotaRecord>, StoreError> {
        let row = sqlx::query(
            "SELECT tenant_id, scope_type, scope_id, storage_bytes, documents, chunks,
                    members, spaces, enforcement, updated_by, updated_at
             FROM space_quotas WHERE tenant_id = ? AND scope_type = ? AND scope_id = ?",
        )
        .bind(tenant_id)
        .bind(scope_type.as_str())
        .bind(scope_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| quota_from_row(&row))
            .transpose()
            .map_err(StoreError::Database)
    }

    async fn list_quotas(&self, tenant_id: &str) -> Result<Vec<QuotaRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT tenant_id, scope_type, scope_id, storage_bytes, documents, chunks,
                    members, spaces, enforcement, updated_by, updated_at
             FROM space_quotas WHERE tenant_id = ? ORDER BY scope_type, scope_id",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(quota_from_row)
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(StoreError::Database)
    }

    async fn upsert_quota(&self, quota: &QuotaRecord) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO space_quotas (tenant_id, scope_type, scope_id, storage_bytes,
                documents, chunks, members, spaces, enforcement, updated_by, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (tenant_id, scope_type, scope_id) DO UPDATE SET
               storage_bytes = excluded.storage_bytes,
               documents = excluded.documents,
               chunks = excluded.chunks,
               members = excluded.members,
               spaces = excluded.spaces,
               enforcement = excluded.enforcement,
               updated_by = excluded.updated_by,
               updated_at = excluded.updated_at",
        )
        .bind(&quota.tenant_id)
        .bind(quota.scope_type.as_str())
        .bind(&quota.scope_id)
        .bind(quota.storage_bytes)
        .bind(quota.documents)
        .bind(quota.chunks)
        .bind(quota.members)
        .bind(quota.spaces)
        .bind(&quota.enforcement)
        .bind(&quota.updated_by)
        .bind(&quota.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete_quota(
        &self,
        tenant_id: &str,
        scope_type: QuotaScope,
        scope_id: &str,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query(
            "DELETE FROM space_quotas WHERE tenant_id = ? AND scope_type = ? AND scope_id = ?",
        )
        .bind(tenant_id)
        .bind(scope_type.as_str())
        .bind(scope_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_quota_overrides(
        &self,
        tenant_id: &str,
        scope_type: QuotaScope,
        scope_id: &str,
        dimension: &str,
        now: &str,
    ) -> Result<Vec<QuotaOverrideRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT override_id, tenant_id, scope_type, scope_id, dimension, limit_value,
                    reason, actor, expires_at, created_at
             FROM quota_overrides
             WHERE tenant_id = ? AND scope_type = ? AND scope_id = ? AND dimension = ?
               AND (expires_at IS NULL OR expires_at > ?)
             ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .bind(scope_type.as_str())
        .bind(scope_id)
        .bind(dimension)
        .bind(now)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(quota_override_from_row)
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(StoreError::Database)
    }

    async fn upsert_quota_override(&self, item: &QuotaOverrideRecord) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO quota_overrides (override_id, tenant_id, scope_type, scope_id,
                dimension, limit_value, reason, actor, expires_at, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (override_id) DO UPDATE SET
               limit_value = excluded.limit_value, reason = excluded.reason,
               actor = excluded.actor, expires_at = excluded.expires_at",
        )
        .bind(&item.override_id)
        .bind(&item.tenant_id)
        .bind(item.scope_type.as_str())
        .bind(&item.scope_id)
        .bind(&item.dimension)
        .bind(item.limit_value)
        .bind(&item.reason)
        .bind(&item.actor)
        .bind(&item.expires_at)
        .bind(&item.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_storage_usage(
        &self,
        tenant_id: &str,
        space_id: &str,
    ) -> Result<Option<StorageUsage>, StoreError> {
        let row = sqlx::query(
            "SELECT tenant_id, space_id, source_bytes, index_bytes, history_bytes,
                    total_bytes, reserved_bytes, documents, chunks, members, updated_at
             FROM storage_usage WHERE tenant_id = ? AND space_id = ?",
        )
        .bind(tenant_id)
        .bind(space_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| usage_from_row(&row))
            .transpose()
            .map_err(StoreError::Database)
    }

    async fn list_storage_usage(&self, tenant_id: &str) -> Result<Vec<StorageUsage>, StoreError> {
        let rows = sqlx::query(
            "SELECT tenant_id, space_id, source_bytes, index_bytes, history_bytes,
                    total_bytes, reserved_bytes, documents, chunks, members, updated_at
             FROM storage_usage WHERE tenant_id = ? ORDER BY space_id",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(usage_from_row)
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(StoreError::Database)
    }

    async fn upsert_storage_usage(&self, usage: &StorageUsage) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO storage_usage (tenant_id, space_id, source_bytes, index_bytes,
                history_bytes, total_bytes, reserved_bytes, documents, chunks, members, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (tenant_id, space_id) DO UPDATE SET
               source_bytes = excluded.source_bytes,
               index_bytes = excluded.index_bytes,
               history_bytes = excluded.history_bytes,
               total_bytes = excluded.total_bytes,
               reserved_bytes = excluded.reserved_bytes,
               documents = excluded.documents,
               chunks = excluded.chunks,
               members = excluded.members,
               updated_at = excluded.updated_at",
        )
        .bind(&usage.tenant_id)
        .bind(&usage.space_id)
        .bind(usage.source_bytes)
        .bind(usage.index_bytes)
        .bind(usage.history_bytes)
        .bind(usage.total_bytes)
        .bind(usage.reserved_bytes)
        .bind(usage.documents)
        .bind(usage.chunks)
        .bind(usage.members)
        .bind(&usage.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn recompute_storage_usage(
        &self,
        tenant_id: &str,
        space_id: &str,
    ) -> Result<StorageUsage, StoreError> {
        let row = sqlx::query(
            "SELECT
               COALESCE((SELECT SUM(LENGTH(CAST(content AS BLOB))) FROM documents
                         WHERE tenant_id = ? AND space_id = ? AND status = 'active'), 0) AS source_bytes,
               (COALESCE((SELECT SUM(LENGTH(CAST(c.body AS BLOB))) FROM chunks c
                         JOIN documents d ON d.tenant_id = c.tenant_id AND d.doc_id = c.doc_id
                         WHERE c.tenant_id = ? AND c.space_id = ? AND d.status = 'active'), 0)
                + COALESCE((SELECT SUM(LENGTH(CAST(e.text AS BLOB))) FROM events e
                         JOIN documents d ON d.tenant_id = e.tenant_id AND d.doc_id = e.doc_id
                         WHERE e.tenant_id = ? AND e.space_id = ? AND d.status = 'active'), 0)) AS index_bytes,
               COALESCE((SELECT SUM(LENGTH(CAST(content AS BLOB))) FROM documents
                         WHERE tenant_id = ? AND space_id = ? AND status <> 'active'), 0) AS history_source,
               COALESCE((SELECT SUM(LENGTH(CAST(c.body AS BLOB))) FROM chunks c
                         JOIN documents d ON d.tenant_id = c.tenant_id AND d.doc_id = c.doc_id
                         WHERE c.tenant_id = ? AND c.space_id = ? AND d.status <> 'active'), 0) AS history_index,
               COALESCE((SELECT COUNT(*) FROM documents
                         WHERE tenant_id = ? AND space_id = ? AND status = 'active'), 0) AS documents,
               COALESCE((SELECT COUNT(*) FROM chunks c
                         JOIN documents d ON d.tenant_id = c.tenant_id AND d.doc_id = c.doc_id
                         WHERE c.tenant_id = ? AND c.space_id = ? AND d.status = 'active'), 0) AS chunks,
               COALESCE((SELECT COUNT(*) FROM (
                   SELECT principal_id FROM space_members
                   WHERE space_id = ? AND principal_type IN ('user', 'agent')
                   UNION
                   SELECT gm.principal_id FROM space_members sm
                   JOIN group_members gm ON gm.group_id = sm.principal_id
                   WHERE sm.space_id = ? AND sm.principal_type = 'group'
                   UNION
                   SELECT gm.principal_id FROM spaces s
                   JOIN group_members gm ON gm.group_id = s.owner_id
                   WHERE s.tenant_id = ? AND s.space_id = ?
                     AND s.owner_type = 'group'
               )), 0) AS members",
        )
        .bind(tenant_id)
        .bind(space_id)
        .bind(tenant_id)
        .bind(space_id)
        .bind(tenant_id)
        .bind(space_id)
        .bind(tenant_id)
        .bind(space_id)
        .bind(tenant_id)
        .bind(space_id)
        .bind(tenant_id)
        .bind(space_id)
        .bind(tenant_id)
        .bind(space_id)
        .bind(space_id)
        .bind(space_id)
        .bind(tenant_id)
        .bind(space_id)
        .fetch_one(&self.pool)
        .await?;
        let source: i64 = row.try_get("source_bytes")?;
        let index: i64 = row.try_get("index_bytes")?;
        let history_source: i64 = row.try_get("history_source")?;
        let history_index: i64 = row.try_get("history_index")?;
        let history: i64 = history_source + history_index;
        let usage = StorageUsage {
            tenant_id: tenant_id.to_string(),
            space_id: space_id.to_string(),
            source_bytes: source,
            index_bytes: index,
            history_bytes: history,
            total_bytes: source + index + history,
            reserved_bytes: 0,
            documents: row.try_get("documents")?,
            chunks: row.try_get("chunks")?,
            members: row.try_get("members")?,
            updated_at: now_rfc3339(),
        };
        self.upsert_storage_usage(&usage).await?;
        Ok(usage)
    }

    async fn add_reserved_bytes(
        &self,
        tenant_id: &str,
        space_id: &str,
        delta: i64,
        at: &str,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO storage_usage (tenant_id, space_id, reserved_bytes, updated_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT (tenant_id, space_id) DO UPDATE SET
               reserved_bytes = storage_usage.reserved_bytes + excluded.reserved_bytes,
               updated_at = excluded.updated_at",
        )
        .bind(tenant_id)
        .bind(space_id)
        .bind(delta)
        .bind(at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn insert_audit(&self, event: &AuditEventRecord) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO audit_events (event_id, tenant_id, space_id, actor_id, actor_type,
                action, target_type, target_id, result, status_code, request_id, ip,
                user_agent, detail_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&event.event_id)
        .bind(&event.tenant_id)
        .bind(&event.space_id)
        .bind(&event.actor_id)
        .bind(&event.actor_type)
        .bind(&event.action)
        .bind(&event.target_type)
        .bind(&event.target_id)
        .bind(&event.result)
        .bind(event.status_code)
        .bind(&event.request_id)
        .bind(&event.ip)
        .bind(&event.user_agent)
        .bind(&event.detail_json)
        .bind(&event.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_audit(&self, filter: &AuditFilter) -> Result<Vec<AuditEventRecord>, StoreError> {
        let mut sql = String::from(
            "SELECT event_id, tenant_id, space_id, actor_id, actor_type, action,
                    target_type, target_id, result, status_code, request_id, ip,
                    user_agent, detail_json, created_at
             FROM audit_events WHERE tenant_id = ?",
        );
        let mut binds: Vec<String> = vec![filter.tenant_id.clone()];
        if let Some(space) = &filter.space_id {
            sql.push_str(" AND space_id = ?");
            binds.push(space.clone());
        }
        if let Some(actor) = &filter.actor_id {
            sql.push_str(" AND actor_id = ?");
            binds.push(actor.clone());
        }
        if let Some(action) = &filter.action {
            sql.push_str(" AND action = ?");
            binds.push(action.clone());
        }
        if let Some(from) = &filter.from {
            sql.push_str(" AND created_at >= ?");
            binds.push(from.clone());
        }
        if let Some(to) = &filter.to {
            sql.push_str(" AND created_at <= ?");
            binds.push(to.clone());
        }
        sql.push_str(" ORDER BY created_at DESC LIMIT ?");
        let mut query = sqlx::query(&sql);
        for value in binds {
            query = query.bind(value);
        }
        let rows = query
            .bind(if filter.limit > 0 { filter.limit } else { 100 })
            .fetch_all(&self.pool)
            .await?;
        rows.iter()
            .map(audit_from_row)
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(StoreError::Database)
    }
    async fn list_providers(&self, _tenant_id: &str) -> Result<Vec<ProviderRecord>, StoreError> {
        let rows = sqlx::query("SELECT * FROM providers ORDER BY priority, provider_id")
            .fetch_all(&self.pool)
            .await?;
        rows.iter()
            .map(provider_from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }
    async fn get_provider(
        &self,
        _tenant_id: &str,
        provider_id: &str,
    ) -> Result<Option<ProviderRecord>, StoreError> {
        let row = sqlx::query("SELECT * FROM providers WHERE provider_id = ?")
            .bind(provider_id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(|row| provider_from_row(&row))
            .transpose()
            .map_err(StoreError::Database)
    }
    async fn upsert_provider(
        &self,
        _tenant_id: &str,
        p: &ProviderRecord,
    ) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO providers (provider_id, kind, base_url, api_key_env, status, priority, weight, limits_json, timeouts_json, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT (provider_id) DO UPDATE SET kind=excluded.kind, base_url=excluded.base_url, api_key_env=excluded.api_key_env, status=excluded.status, priority=excluded.priority, weight=excluded.weight, limits_json=excluded.limits_json, timeouts_json=excluded.timeouts_json, updated_at=excluded.updated_at")
            .bind(&p.provider_id).bind(&p.kind).bind(&p.base_url).bind(&p.api_key_env).bind(&p.status)
            .bind(p.priority as i64).bind(p.weight as i64).bind(&p.limits_json).bind(&p.timeouts_json)
            .bind(&p.created_at).bind(&p.updated_at).execute(&self.pool).await?;
        Ok(())
    }
    async fn delete_provider(
        &self,
        _tenant_id: &str,
        provider_id: &str,
    ) -> Result<bool, StoreError> {
        Ok(sqlx::query("DELETE FROM providers WHERE provider_id = ?")
            .bind(provider_id)
            .execute(&self.pool)
            .await?
            .rows_affected()
            > 0)
    }

    async fn list_provider_models(
        &self,
        _tenant_id: &str,
    ) -> Result<Vec<ProviderModelRecord>, StoreError> {
        let rows =
            sqlx::query("SELECT * FROM provider_models ORDER BY provider_id, provider_model_id")
                .fetch_all(&self.pool)
                .await?;
        rows.iter()
            .map(provider_model_from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }
    async fn get_provider_model(
        &self,
        _tenant_id: &str,
        id: &str,
    ) -> Result<Option<ProviderModelRecord>, StoreError> {
        let row = sqlx::query("SELECT * FROM provider_models WHERE provider_model_id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(|row| provider_model_from_row(&row))
            .transpose()
            .map_err(StoreError::Database)
    }
    async fn upsert_provider_model(
        &self,
        _tenant_id: &str,
        m: &ProviderModelRecord,
    ) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO provider_models (provider_model_id, provider_id, upstream_model, context_window, pricing_json, capabilities_json, status, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?) ON CONFLICT (provider_model_id) DO UPDATE SET provider_id=excluded.provider_id, upstream_model=excluded.upstream_model, context_window=excluded.context_window, pricing_json=excluded.pricing_json, capabilities_json=excluded.capabilities_json, status=excluded.status, updated_at=excluded.updated_at")
            .bind(&m.provider_model_id).bind(&m.provider_id).bind(&m.upstream_model).bind(m.context_window)
            .bind(&m.pricing_json).bind(&m.capabilities_json).bind(&m.status).bind(&m.created_at).bind(&m.updated_at)
            .execute(&self.pool).await?;
        Ok(())
    }

    async fn list_model_aliases(
        &self,
        _tenant_id: &str,
    ) -> Result<Vec<ModelAliasRecord>, StoreError> {
        let rows = sqlx::query("SELECT * FROM model_aliases ORDER BY public_id")
            .fetch_all(&self.pool)
            .await?;
        rows.iter()
            .map(alias_from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }
    async fn get_model_alias(
        &self,
        _tenant_id: &str,
        public_id: &str,
    ) -> Result<Option<ModelAliasRecord>, StoreError> {
        let row = sqlx::query("SELECT * FROM model_aliases WHERE public_id = ?")
            .bind(public_id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(|row| alias_from_row(&row))
            .transpose()
            .map_err(StoreError::Database)
    }
    async fn upsert_model_alias(
        &self,
        _tenant_id: &str,
        a: &ModelAliasRecord,
    ) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO model_aliases (public_id, display_name, capabilities_json, status, access_policy_json, price_json, version, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?) ON CONFLICT (public_id) DO UPDATE SET display_name=excluded.display_name, capabilities_json=excluded.capabilities_json, status=excluded.status, access_policy_json=excluded.access_policy_json, price_json=excluded.price_json, version=excluded.version, updated_at=excluded.updated_at")
            .bind(&a.public_id).bind(&a.display_name).bind(&a.capabilities_json).bind(&a.status).bind(&a.access_policy_json)
            .bind(&a.price_json).bind(a.version as i64).bind(&a.created_at).bind(&a.updated_at).execute(&self.pool).await?;
        Ok(())
    }
    async fn delete_model_alias(
        &self,
        _tenant_id: &str,
        public_id: &str,
    ) -> Result<bool, StoreError> {
        Ok(sqlx::query("DELETE FROM model_aliases WHERE public_id = ?")
            .bind(public_id)
            .execute(&self.pool)
            .await?
            .rows_affected()
            > 0)
    }

    async fn list_routes(
        &self,
        _tenant_id: &str,
        alias: Option<&str>,
    ) -> Result<Vec<RouteRecord>, StoreError> {
        let rows = if let Some(alias) = alias {
            sqlx::query(
                "SELECT * FROM routes WHERE alias_public_id = ? ORDER BY priority, route_id",
            )
            .bind(alias)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query("SELECT * FROM routes ORDER BY alias_public_id, priority, route_id")
                .fetch_all(&self.pool)
                .await?
        };
        rows.iter()
            .map(route_from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }
    async fn upsert_route(&self, _tenant_id: &str, r: &RouteRecord) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO routes (route_id, alias_public_id, provider_model_id, weight, priority, health, circuit_state, last_error_at, circuit_open_until, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT (route_id) DO UPDATE SET alias_public_id=excluded.alias_public_id, provider_model_id=excluded.provider_model_id, weight=excluded.weight, priority=excluded.priority, health=excluded.health, circuit_state=excluded.circuit_state, last_error_at=excluded.last_error_at, circuit_open_until=excluded.circuit_open_until, updated_at=excluded.updated_at")
            .bind(&r.route_id).bind(&r.alias_public_id).bind(&r.provider_model_id).bind(r.weight as i64).bind(r.priority as i64)
            .bind(&r.health).bind(&r.circuit_state).bind(&r.last_error_at).bind(&r.circuit_open_until)
            .bind(&r.created_at).bind(&r.updated_at).execute(&self.pool).await?;
        Ok(())
    }
    async fn delete_route(&self, _tenant_id: &str, route_id: &str) -> Result<bool, StoreError> {
        Ok(sqlx::query("DELETE FROM routes WHERE route_id = ?")
            .bind(route_id)
            .execute(&self.pool)
            .await?
            .rows_affected()
            > 0)
    }
    async fn update_route_health(
        &self,
        _tenant_id: &str,
        route_id: &str,
        health: &str,
        circuit: &str,
        last_error_at: Option<&str>,
        open_until: Option<&str>,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE routes SET health=?, circuit_state=?, last_error_at=?, circuit_open_until=?, updated_at=? WHERE route_id=?")
            .bind(health).bind(circuit).bind(last_error_at).bind(open_until).bind(now_rfc3339()).bind(route_id)
            .execute(&self.pool).await?;
        Ok(())
    }

    async fn insert_usage_event(&self, e: &UsageEventRecord) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO usage_events (usage_event_id, request_id, tenant_id, space_id, user_id, role, source, external_usage_id, alias, provider_model_id, input_tokens, output_tokens, cached_tokens, token_source, cost_micros, currency, provider_cost_micros, latency_ms, status, cache_hit, rewritten_query, at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT (usage_event_id) DO NOTHING")
            .bind(&e.usage_event_id).bind(&e.request_id).bind(&e.tenant_id).bind(&e.space_id).bind(&e.user_id).bind(&e.role).bind(&e.source).bind(&e.external_usage_id)
            .bind(&e.alias).bind(&e.provider_model_id).bind(e.input_tokens).bind(e.output_tokens).bind(e.cached_tokens).bind(&e.token_source)
            .bind(e.cost_micros).bind(&e.currency).bind(e.provider_cost_micros).bind(e.latency_ms).bind(&e.status).bind(if e.cache_hit {1i64} else {0i64}).bind(&e.rewritten_query).bind(&e.at)
            .execute(&self.pool).await?;
        Ok(())
    }
    async fn list_usage_events(
        &self,
        f: &UsageFilter,
    ) -> Result<Vec<UsageEventRecord>, StoreError> {
        let mut sql = String::from("SELECT * FROM usage_events WHERE tenant_id = ?");
        let mut binds: Vec<String> = vec![f.tenant_id.clone()];
        if let Some(v) = &f.space_id {
            sql.push_str(" AND space_id = ?");
            binds.push(v.clone());
        }
        if let Some(v) = &f.user_id {
            sql.push_str(" AND user_id = ?");
            binds.push(v.clone());
        }
        if let Some(v) = &f.alias {
            sql.push_str(" AND alias = ?");
            binds.push(v.clone());
        }
        if let Some(v) = &f.from {
            sql.push_str(" AND at >= ?");
            binds.push(v.clone());
        }
        if let Some(v) = &f.to {
            sql.push_str(" AND at <= ?");
            binds.push(v.clone());
        }
        sql.push_str(" ORDER BY at DESC LIMIT ?");
        let mut q = sqlx::query(&sql);
        for v in binds {
            q = q.bind(v);
        }
        let rows = q
            .bind(if f.limit > 0 { f.limit } else { 1000 })
            .fetch_all(&self.pool)
            .await?;
        rows.iter()
            .map(usage_from_row_gateway)
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }

    async fn list_token_quotas(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<TokenQuotaRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT * FROM token_quotas WHERE tenant_id = ? ORDER BY scope_type, scope_id",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(token_quota_from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }
    async fn upsert_token_quota(&self, q: &TokenQuotaRecord) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO token_quotas (quota_id, tenant_id, scope_type, scope_id, period, limit_tokens, limit_requests, limit_cost_micros, currency, enforcement, action, enabled, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT (quota_id) DO UPDATE SET scope_type=excluded.scope_type, scope_id=excluded.scope_id, period=excluded.period, limit_tokens=excluded.limit_tokens, limit_requests=excluded.limit_requests, limit_cost_micros=excluded.limit_cost_micros, currency=excluded.currency, enforcement=excluded.enforcement, action=excluded.action, enabled=excluded.enabled, updated_at=excluded.updated_at")
            .bind(&q.quota_id).bind(&q.tenant_id).bind(&q.scope_type).bind(&q.scope_id).bind(&q.period).bind(q.limit_tokens).bind(q.limit_requests).bind(q.limit_cost_micros)
            .bind(&q.currency).bind(&q.enforcement).bind(&q.action).bind(if q.enabled {1i64} else {0i64}).bind(&q.created_at).bind(&q.updated_at)
            .execute(&self.pool).await?;
        Ok(())
    }
    async fn delete_token_quota(
        &self,
        tenant_id: &str,
        quota_id: &str,
    ) -> Result<bool, StoreError> {
        Ok(
            sqlx::query("DELETE FROM token_quotas WHERE tenant_id = ? AND quota_id = ?")
                .bind(tenant_id)
                .bind(quota_id)
                .execute(&self.pool)
                .await?
                .rows_affected()
                > 0,
        )
    }

    async fn get_quota_counter(
        &self,
        tenant_id: &str,
        scope_type: &str,
        scope_id: &str,
        period_start: &str,
    ) -> Result<Option<QuotaCounter>, StoreError> {
        let row = sqlx::query("SELECT * FROM quota_counters WHERE tenant_id=? AND scope_type=? AND scope_id=? AND period_start=?")
            .bind(tenant_id).bind(scope_type).bind(scope_id).bind(period_start).fetch_optional(&self.pool).await?;
        row.map(|row| quota_counter_from_row(&row))
            .transpose()
            .map_err(StoreError::Database)
    }
    async fn list_quota_counters(&self, tenant_id: &str) -> Result<Vec<QuotaCounter>, StoreError> {
        let rows = sqlx::query(
            "SELECT * FROM quota_counters WHERE tenant_id=? ORDER BY scope_type, scope_id",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(quota_counter_from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }
    async fn add_quota_reservation(&self, r: &QuotaReservation) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO quota_counters (tenant_id, scope_type, scope_id, period_start, reserved_tokens, used_tokens, reserved_requests, used_requests, reserved_cost_micros, used_cost_micros, updated_at) VALUES (?,?,?,?,?,0,?,0,?,0,?) ON CONFLICT (tenant_id, scope_type, scope_id, period_start) DO UPDATE SET reserved_tokens=quota_counters.reserved_tokens+excluded.reserved_tokens, reserved_requests=quota_counters.reserved_requests+excluded.reserved_requests, reserved_cost_micros=quota_counters.reserved_cost_micros+excluded.reserved_cost_micros, updated_at=excluded.updated_at")
            .bind(&r.tenant_id).bind(&r.scope_type).bind(&r.scope_id).bind(&r.period_start).bind(r.reserved_tokens).bind(r.reserved_requests).bind(r.reserved_cost_micros).bind(&r.created_at)
            .execute(&mut *tx).await?;
        sqlx::query("INSERT INTO quota_reservations (reservation_id, request_id, tenant_id, scope_type, scope_id, period_start, reserved_tokens, reserved_requests, reserved_cost_micros, status, expires_at, created_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&r.reservation_id).bind(&r.request_id).bind(&r.tenant_id).bind(&r.scope_type).bind(&r.scope_id).bind(&r.period_start)
            .bind(r.reserved_tokens).bind(r.reserved_requests).bind(r.reserved_cost_micros).bind(&r.status).bind(&r.expires_at).bind(&r.created_at)
            .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    async fn get_quota_reservation(
        &self,
        tenant_id: &str,
        request_id: &str,
    ) -> Result<Option<QuotaReservation>, StoreError> {
        let row = sqlx::query("SELECT * FROM quota_reservations WHERE tenant_id=? AND request_id=? ORDER BY created_at DESC LIMIT 1").bind(tenant_id).bind(request_id).fetch_optional(&self.pool).await?;
        row.map(|row| quota_reservation_from_row(&row))
            .transpose()
            .map_err(StoreError::Database)
    }
    async fn update_quota_reservation(
        &self,
        tenant_id: &str,
        request_id: &str,
        status: &str,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        let rows =
            sqlx::query("SELECT * FROM quota_reservations WHERE tenant_id=? AND request_id=?")
                .bind(tenant_id)
                .bind(request_id)
                .fetch_all(&mut *tx)
                .await?;
        for row in rows {
            let r = quota_reservation_from_row(&row)?;
            if r.status == "reserved" && (status == "committed" || status == "released") {
                if status == "committed" {
                    sqlx::query("UPDATE quota_counters SET reserved_tokens=reserved_tokens-?, used_tokens=used_tokens+?, reserved_requests=reserved_requests-?, used_requests=used_requests+?, reserved_cost_micros=reserved_cost_micros-?, used_cost_micros=used_cost_micros+?, updated_at=? WHERE tenant_id=? AND scope_type=? AND scope_id=? AND period_start=?")
                        .bind(r.reserved_tokens).bind(r.reserved_tokens).bind(r.reserved_requests).bind(r.reserved_requests).bind(r.reserved_cost_micros).bind(r.reserved_cost_micros).bind(now_rfc3339())
                        .bind(&r.tenant_id).bind(&r.scope_type).bind(&r.scope_id).bind(&r.period_start).execute(&mut *tx).await?;
                } else {
                    sqlx::query("UPDATE quota_counters SET reserved_tokens=reserved_tokens-?, reserved_requests=reserved_requests-?, reserved_cost_micros=reserved_cost_micros-?, updated_at=? WHERE tenant_id=? AND scope_type=? AND scope_id=? AND period_start=?")
                        .bind(r.reserved_tokens).bind(r.reserved_requests).bind(r.reserved_cost_micros).bind(now_rfc3339())
                        .bind(&r.tenant_id).bind(&r.scope_type).bind(&r.scope_id).bind(&r.period_start).execute(&mut *tx).await?;
                }
                sqlx::query("UPDATE quota_reservations SET status=? WHERE reservation_id=?")
                    .bind(status)
                    .bind(&r.reservation_id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }
    async fn list_quota_reservations(
        &self,
        tenant_id: &str,
        request_id: &str,
    ) -> Result<Vec<QuotaReservation>, StoreError> {
        let rows =
            sqlx::query("SELECT * FROM quota_reservations WHERE tenant_id=? AND request_id=?")
                .bind(tenant_id)
                .bind(request_id)
                .fetch_all(&self.pool)
                .await?;
        rows.iter()
            .map(quota_reservation_from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }
    async fn adjust_quota_counter(
        &self,
        tenant_id: &str,
        scope_type: &str,
        scope_id: &str,
        period_start: &str,
        token_delta: i64,
        cost_delta: i64,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE quota_counters SET used_tokens=MAX(0, used_tokens+?), used_cost_micros=MAX(0, used_cost_micros+?), updated_at=? WHERE tenant_id=? AND scope_type=? AND scope_id=? AND period_start=?")
            .bind(token_delta).bind(cost_delta).bind(now_rfc3339()).bind(tenant_id).bind(scope_type).bind(scope_id).bind(period_start)
            .execute(&self.pool).await?;
        Ok(())
    }

    async fn cleanup_expired_quota_reservations(
        &self,
        tenant_id: &str,
        now: &str,
    ) -> Result<usize, StoreError> {
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query("SELECT * FROM quota_reservations WHERE tenant_id=? AND status='reserved' AND expires_at < ?").bind(tenant_id).bind(now).fetch_all(&mut *tx).await?;
        let mut count = 0usize;
        for row in rows {
            let r = quota_reservation_from_row(&row)?;
            sqlx::query("UPDATE quota_counters SET reserved_tokens=MAX(0,reserved_tokens-?), reserved_requests=MAX(0,reserved_requests-?), reserved_cost_micros=MAX(0,reserved_cost_micros-?), updated_at=? WHERE tenant_id=? AND scope_type=? AND scope_id=? AND period_start=?")
                .bind(r.reserved_tokens).bind(r.reserved_requests).bind(r.reserved_cost_micros).bind(now)
                .bind(&r.tenant_id).bind(&r.scope_type).bind(&r.scope_id).bind(&r.period_start).execute(&mut *tx).await?;
            sqlx::query("UPDATE quota_reservations SET status='expired' WHERE reservation_id=?")
                .bind(&r.reservation_id)
                .execute(&mut *tx)
                .await?;
            count += 1;
        }
        tx.commit().await?;
        Ok(count)
    }

    async fn aggregate_usage_events(&self, f: &UsageFilter) -> Result<UsageAggregate, StoreError> {
        let mut sql = String::from("SELECT COUNT(*) AS requests, COALESCE(SUM(input_tokens),0) AS input_tokens, COALESCE(SUM(output_tokens),0) AS output_tokens, COALESCE(SUM(cached_tokens),0) AS cached_tokens, COALESCE(SUM(cost_micros),0) AS cost_micros, COALESCE(SUM(provider_cost_micros),0) AS provider_cost_micros, COALESCE(SUM(CASE WHEN status <> 'success' THEN 1 ELSE 0 END),0) AS errors FROM usage_events WHERE tenant_id = ?");
        let mut binds: Vec<String> = vec![f.tenant_id.clone()];
        if let Some(v) = &f.space_id {
            sql.push_str(" AND space_id = ?");
            binds.push(v.clone());
        }
        if let Some(v) = &f.user_id {
            sql.push_str(" AND user_id = ?");
            binds.push(v.clone());
        }
        if let Some(v) = &f.alias {
            sql.push_str(" AND alias = ?");
            binds.push(v.clone());
        }
        if let Some(v) = &f.from {
            sql.push_str(" AND at >= ?");
            binds.push(v.clone());
        }
        if let Some(v) = &f.to {
            sql.push_str(" AND at <= ?");
            binds.push(v.clone());
        }
        let mut q = sqlx::query(&sql);
        for v in binds {
            q = q.bind(v);
        }
        let row = q.fetch_one(&self.pool).await?;
        Ok(UsageAggregate {
            requests: row.try_get("requests")?,
            input_tokens: row.try_get("input_tokens")?,
            output_tokens: row.try_get("output_tokens")?,
            cached_tokens: row.try_get("cached_tokens")?,
            cost_micros: row.try_get("cost_micros")?,
            provider_cost_micros: row.try_get("provider_cost_micros")?,
            errors: row.try_get("errors")?,
        })
    }

    async fn upsert_document_summary(
        &self,
        summary: &DocumentSummaryRecord,
    ) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO document_summaries (tenant_id, space_id, doc_id, revision_id, lang, model, prompt_version, content_hash, status, summary_json, usage_event_id, generated_at, error) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT (tenant_id, space_id, doc_id, revision_id, model, prompt_version) DO UPDATE SET content_hash=excluded.content_hash, status=excluded.status, summary_json=excluded.summary_json, usage_event_id=excluded.usage_event_id, generated_at=excluded.generated_at, error=excluded.error")
            .bind(&summary.tenant_id).bind(&summary.space_id).bind(&summary.doc_id).bind(&summary.revision_id).bind(&summary.lang).bind(&summary.model).bind(&summary.prompt_version).bind(&summary.content_hash).bind(&summary.status).bind(&summary.summary_json).bind(&summary.usage_event_id).bind(&summary.generated_at).bind(&summary.error)
            .execute(&self.pool).await?;
        Ok(())
    }
    async fn get_document_summary(
        &self,
        tenant_id: &str,
        space_id: &str,
        doc_id: &str,
        revision_id: &str,
        model: &str,
        prompt_version: &str,
    ) -> Result<Option<DocumentSummaryRecord>, StoreError> {
        let row = sqlx::query("SELECT * FROM document_summaries WHERE tenant_id=? AND space_id=? AND doc_id=? AND revision_id=? AND model=? AND prompt_version=?")
            .bind(tenant_id).bind(space_id).bind(doc_id).bind(revision_id).bind(model).bind(prompt_version).fetch_optional(&self.pool).await?;
        row.map(|row| summary_from_row(&row))
            .transpose()
            .map_err(StoreError::Database)
    }
    async fn create_summary_job(&self, job: &SummaryJobRecord) -> Result<bool, StoreError> {
        let result = sqlx::query("INSERT INTO summary_jobs (job_id, tenant_id, space_id, doc_id, revision_id, status, attempts, scheduled_at, started_at, finished_at, error) VALUES (?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT (tenant_id, space_id, doc_id, revision_id) DO NOTHING")
            .bind(&job.job_id).bind(&job.tenant_id).bind(&job.space_id).bind(&job.doc_id).bind(&job.revision_id).bind(&job.status).bind(job.attempts as i64).bind(&job.scheduled_at).bind(&job.started_at).bind(&job.finished_at).bind(&job.error)
            .execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }
    async fn list_summary_jobs(
        &self,
        status: &str,
        limit: i64,
    ) -> Result<Vec<SummaryJobRecord>, StoreError> {
        let rows =
            sqlx::query("SELECT * FROM summary_jobs WHERE status=? ORDER BY scheduled_at LIMIT ?")
                .bind(status)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?;
        rows.iter()
            .map(summary_job_from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }
    async fn update_summary_job(
        &self,
        job_id: &str,
        status: &str,
        attempts: u32,
        started_at: Option<&str>,
        finished_at: Option<&str>,
        error: Option<&str>,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE summary_jobs SET status=?, attempts=?, started_at=COALESCE(?, started_at), finished_at=?, error=? WHERE job_id=?")
            .bind(status).bind(attempts as i64).bind(started_at).bind(finished_at).bind(error).bind(job_id).execute(&self.pool).await?;
        Ok(())
    }
    async fn create_conversation(&self, c: &ConversationRecord) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO conversations (conversation_id, tenant_id, space_id, owner_id, title, external_session_id, status, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?)")
            .bind(&c.conversation_id).bind(&c.tenant_id).bind(&c.space_id).bind(&c.owner_id).bind(&c.title).bind(&c.external_session_id).bind(&c.status).bind(&c.created_at).bind(&c.updated_at).execute(&self.pool).await?;
        Ok(())
    }
    async fn get_conversation(
        &self,
        tenant_id: &str,
        conversation_id: &str,
    ) -> Result<Option<ConversationRecord>, StoreError> {
        let row =
            sqlx::query("SELECT * FROM conversations WHERE tenant_id=? AND conversation_id=?")
                .bind(tenant_id)
                .bind(conversation_id)
                .fetch_optional(&self.pool)
                .await?;
        row.map(|row| conversation_from_row(&row))
            .transpose()
            .map_err(StoreError::Database)
    }
    async fn list_conversations(
        &self,
        tenant_id: &str,
        owner_id: &str,
        limit: i64,
    ) -> Result<Vec<ConversationRecord>, StoreError> {
        let rows = sqlx::query("SELECT * FROM conversations WHERE tenant_id=? AND owner_id=? AND status <> 'deleted' ORDER BY updated_at DESC LIMIT ?").bind(tenant_id).bind(owner_id).bind(limit).fetch_all(&self.pool).await?;
        rows.iter()
            .map(conversation_from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }
    async fn update_conversation(
        &self,
        tenant_id: &str,
        conversation_id: &str,
        title: Option<&str>,
        status: Option<&str>,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE conversations SET title=COALESCE(?, title), status=COALESCE(?, status), updated_at=? WHERE tenant_id=? AND conversation_id=?")
            .bind(title).bind(status).bind(now_rfc3339()).bind(tenant_id).bind(conversation_id).execute(&self.pool).await?;
        Ok(())
    }
    async fn insert_message(&self, m: &MessageRecord) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO messages (message_id, conversation_id, role, content, citations_json, model, usage_json, degraded_json, request_id, mode, rewritten_query, conversation_context_hash, created_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&m.message_id).bind(&m.conversation_id).bind(&m.role).bind(&m.content).bind(&m.citations_json).bind(&m.model).bind(&m.usage_json).bind(&m.degraded_json).bind(&m.request_id).bind(&m.mode).bind(&m.rewritten_query).bind(&m.conversation_context_hash).bind(&m.created_at).execute(&self.pool).await?;
        Ok(())
    }
    async fn list_messages(
        &self,
        conversation_id: &str,
        limit: i64,
    ) -> Result<Vec<MessageRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT * FROM messages WHERE conversation_id=? ORDER BY created_at ASC LIMIT ?",
        )
        .bind(conversation_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(message_from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }

    async fn bind_conversation_external_session(
        &self,
        tenant_id: &str,
        conversation_id: &str,
        external_session_id: &str,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE conversations SET external_session_id=?, updated_at=? WHERE tenant_id=? AND conversation_id=?")
            .bind(external_session_id).bind(now_rfc3339()).bind(tenant_id).bind(conversation_id).execute(&self.pool).await?;
        Ok(())
    }

    async fn set_usage_rewritten_query(
        &self,
        usage_event_id: &str,
        rewritten_query: &str,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE usage_events SET rewritten_query=? WHERE usage_event_id=?")
            .bind(rewritten_query)
            .bind(usage_event_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn set_user_disabled(
        &self,
        tenant_id: &str,
        user_id: &str,
        disabled: bool,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE users SET disabled=? WHERE tenant_id=? AND user_id=?")
            .bind(if disabled { 1i64 } else { 0i64 })
            .bind(tenant_id)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn archive_page(
        &self,
        tenant_id: &str,
        space_id: &str,
        slug: &str,
        lang: &str,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query("UPDATE pages SET valid_to=? WHERE tenant_id=? AND space_id=? AND slug=? AND lang=? AND valid_to IS NULL")
            .bind(now_rfc3339()).bind(tenant_id).bind(space_id).bind(slug).bind(lang).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }
    async fn restore_page(
        &self,
        tenant_id: &str,
        space_id: &str,
        slug: &str,
        lang: &str,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query("UPDATE pages SET valid_to=NULL WHERE tenant_id=? AND space_id=? AND slug=? AND lang=? AND valid_to IS NOT NULL")
            .bind(tenant_id).bind(space_id).bind(slug).bind(lang).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    async fn archive_document_path(
        &self,
        tenant_id: &str,
        space_id: &str,
        logical_path: &str,
    ) -> Result<usize, StoreError> {
        let now = now_rfc3339();
        let docs = sqlx::query("UPDATE documents SET status='archived', valid_to=? WHERE tenant_id=? AND space_id=? AND logical_path=? AND status='active' AND valid_to IS NULL")
            .bind(&now).bind(tenant_id).bind(space_id).bind(logical_path).execute(&self.pool).await?;
        sqlx::query("UPDATE chunks SET valid_to=? WHERE tenant_id=? AND space_id=? AND valid_to IS NULL AND doc_id IN (SELECT doc_id FROM documents WHERE tenant_id=? AND space_id=? AND logical_path=?)")
            .bind(&now).bind(tenant_id).bind(space_id).bind(tenant_id).bind(space_id).bind(logical_path).execute(&self.pool).await?;
        Ok(docs.rows_affected() as usize)
    }
    async fn restore_document_path(
        &self,
        tenant_id: &str,
        space_id: &str,
        logical_path: &str,
    ) -> Result<usize, StoreError> {
        let docs = sqlx::query("UPDATE documents SET status='active', valid_to=NULL WHERE tenant_id=? AND space_id=? AND logical_path=? AND status='archived'")
            .bind(tenant_id).bind(space_id).bind(logical_path).execute(&self.pool).await?;
        sqlx::query("UPDATE chunks SET valid_to=NULL WHERE tenant_id=? AND space_id=? AND valid_to IS NOT NULL AND doc_id IN (SELECT doc_id FROM documents WHERE tenant_id=? AND space_id=? AND logical_path=?)")
            .bind(tenant_id).bind(space_id).bind(tenant_id).bind(space_id).bind(logical_path).execute(&self.pool).await?;
        Ok(docs.rows_affected() as usize)
    }

    async fn list_page_records(
        &self,
        policy: &Policy,
        limit: i64,
    ) -> Result<Vec<PageRecord>, StoreError> {
        let visibility = policy.visibility("p", self.dialect())?;
        let sql = format!("SELECT * FROM pages p WHERE {vis} ORDER BY p.space_id, p.lang, p.order_index, p.slug LIMIT ?", vis = visibility.sql);
        let mut query = sqlx::query(&sql);
        for value in &visibility.binds {
            query = query.bind(value.as_str());
        }
        let rows = query.bind(limit).fetch_all(&self.pool).await?;
        rows.iter()
            .map(|row| -> Result<PageRecord, sqlx::Error> {
                Ok(PageRecord {
                    tenant_id: row.try_get("tenant_id")?,
                    space_id: row.try_get("space_id")?,
                    slug: row.try_get("slug")?,
                    lang: row.try_get("lang")?,
                    title: row.try_get("title")?,
                    summary: row.try_get("summary")?,
                    order_index: row.try_get::<i64, _>("order_index")? as i32,
                    current_revision_id: row.try_get("current_revision_id")?,
                    content_hash: row.try_get("content_hash")?,
                    acl_json: row.try_get("acl_json")?,
                    acl_principals: row.try_get("acl_principals")?,
                    valid_from: row.try_get("valid_from")?,
                    valid_to: row.try_get("valid_to")?,
                    version: row.try_get::<i64, _>("version")? as u32,
                    superseded_by: row.try_get("superseded_by")?,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }
    async fn get_revision(
        &self,
        tenant_id: &str,
        revision_id: &str,
    ) -> Result<Option<RevisionRecord>, StoreError> {
        let row = sqlx::query("SELECT * FROM revisions WHERE tenant_id=? AND revision_id=?")
            .bind(tenant_id)
            .bind(revision_id)
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some(row) => Ok(Some(RevisionRecord {
                tenant_id: row.try_get("tenant_id")?,
                revision_id: row.try_get("revision_id")?,
                space_id: row.try_get("space_id")?,
                slug: row.try_get("slug")?,
                lang: row.try_get("lang")?,
                title: row.try_get("title")?,
                markdown: row.try_get("markdown")?,
                content_hash: row.try_get("content_hash")?,
                author: row.try_get("author")?,
                created_at: row.try_get("created_at")?,
                version: row.try_get::<i64, _>("version")? as u32,
                acl_json: row.try_get("acl_json")?,
                acl_principals: row.try_get("acl_principals")?,
                valid_from: row.try_get("valid_from")?,
                valid_to: row.try_get("valid_to")?,
                superseded_by: row.try_get("superseded_by")?,
                source_url: row.try_get("source_url")?,
                source_revision: row.try_get("source_revision")?,
            })),
            None => Ok(None),
        }
    }
    async fn restore_conversation(
        &self,
        tenant_id: &str,
        conversation_id: &str,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query("UPDATE conversations SET status='active', updated_at=? WHERE tenant_id=? AND conversation_id=? AND status='deleted'")
            .bind(now_rfc3339()).bind(tenant_id).bind(conversation_id).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    async fn billing_get_wallet(
        &self,
        tenant_id: &str,
        currency: &str,
    ) -> Result<Option<WalletRecord>, StoreError> {
        let row = sqlx::query("SELECT * FROM wallets WHERE tenant_id=? AND currency=?")
            .bind(tenant_id)
            .bind(currency)
            .fetch_optional(&self.pool)
            .await
            .map_err(StoreError::Database)?;
        row.map(|row| -> Result<WalletRecord, sqlx::Error> {
            Ok(WalletRecord {
                wallet_id: row.try_get("wallet_id")?,
                tenant_id: row.try_get("tenant_id")?,
                currency: row.try_get("currency")?,
                balance_micros: row.try_get("balance_micros")?,
                status: row.try_get("status")?,
                updated_at: row.try_get("updated_at")?,
            })
        })
        .transpose()
        .map_err(StoreError::Database)
    }
    async fn billing_list_wallets(&self, tenant_id: &str) -> Result<Vec<WalletRecord>, StoreError> {
        let rows = sqlx::query("SELECT * FROM wallets WHERE tenant_id=? ORDER BY currency")
            .bind(tenant_id)
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::Database)?;
        rows.iter()
            .map(|row| -> Result<WalletRecord, sqlx::Error> {
                Ok(WalletRecord {
                    wallet_id: row.try_get("wallet_id")?,
                    tenant_id: row.try_get("tenant_id")?,
                    currency: row.try_get("currency")?,
                    balance_micros: row.try_get("balance_micros")?,
                    status: row.try_get("status")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }
    async fn billing_ensure_wallet(&self, wallet: &WalletRecord) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO wallets (wallet_id, tenant_id, currency, balance_micros, status, updated_at) VALUES (?,?,?,?,?,?) ON CONFLICT (tenant_id, currency) DO NOTHING")
            .bind(&wallet.wallet_id).bind(&wallet.tenant_id).bind(&wallet.currency).bind(wallet.balance_micros).bind(&wallet.status).bind(&wallet.updated_at)
            .execute(&self.pool).await.map_err(StoreError::Database)?;
        Ok(())
    }
    async fn billing_post_ledger(
        &self,
        entries: &[LedgerEntryRecord],
        balance_delta_micros: i64,
        allow_negative: bool,
        currency: &str,
    ) -> Result<LedgerTxResult, StoreError> {
        let Some(first) = entries.first() else {
            return Ok(LedgerTxResult::Duplicate);
        };
        let mut tx = self.pool.begin().await.map_err(StoreError::Database)?;
        for entry in entries {
            let result = sqlx::query("INSERT INTO ledger_entries (entry_id, group_id, wallet_id, tenant_id, account, direction, amount_micros, currency, idempotency_key, request_id, metadata_json, created_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(&entry.entry_id).bind(&entry.group_id).bind(&entry.wallet_id).bind(&entry.tenant_id).bind(&entry.account).bind(&entry.direction)
                .bind(entry.amount_micros).bind(&entry.currency).bind(&entry.idempotency_key).bind(&entry.request_id).bind(&entry.metadata_json).bind(&entry.created_at)
                .execute(&mut *tx).await;
            if let Err(err) = result {
                if let sqlx::Error::Database(db) = &err {
                    if db.is_unique_violation() {
                        let _ = tx.rollback().await;
                        return Ok(LedgerTxResult::Duplicate);
                    }
                }
                return Err(StoreError::Database(err));
            }
        }
        if balance_delta_micros != 0 {
            let now = crate::util::now_rfc3339();
            let result = if allow_negative {
                sqlx::query("UPDATE wallets SET balance_micros = balance_micros + ?, updated_at=? WHERE tenant_id=? AND currency=?")
                    .bind(balance_delta_micros).bind(&now).bind(&first.tenant_id).bind(currency).execute(&mut *tx).await
            } else {
                sqlx::query("UPDATE wallets SET balance_micros = balance_micros + ?, updated_at=? WHERE tenant_id=? AND currency=? AND balance_micros + ? >= 0")
                    .bind(balance_delta_micros).bind(&now).bind(&first.tenant_id).bind(currency).bind(balance_delta_micros).execute(&mut *tx).await
            }.map_err(StoreError::Database)?;
            if !allow_negative && balance_delta_micros < 0 && result.rows_affected() == 0 {
                let _ = tx.rollback().await;
                return Ok(LedgerTxResult::Insufficient);
            }
        }
        let balance: i64 = sqlx::query_scalar(
            "SELECT balance_micros FROM wallets WHERE tenant_id=? AND currency=?",
        )
        .bind(&first.tenant_id)
        .bind(currency)
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Database)?;
        tx.commit().await.map_err(StoreError::Database)?;
        Ok(LedgerTxResult::Applied {
            balance_micros: balance,
        })
    }
    async fn billing_list_ledger(
        &self,
        tenant_id: &str,
        limit: i64,
    ) -> Result<Vec<LedgerEntryRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT * FROM ledger_entries WHERE tenant_id=? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Database)?;
        rows.iter()
            .map(|row| -> Result<LedgerEntryRecord, sqlx::Error> {
                Ok(LedgerEntryRecord {
                    entry_id: row.try_get("entry_id")?,
                    group_id: row.try_get("group_id")?,
                    wallet_id: row.try_get("wallet_id")?,
                    tenant_id: row.try_get("tenant_id")?,
                    account: row.try_get("account")?,
                    direction: row.try_get("direction")?,
                    amount_micros: row.try_get("amount_micros")?,
                    currency: row.try_get("currency")?,
                    idempotency_key: row.try_get("idempotency_key")?,
                    request_id: row.try_get("request_id")?,
                    metadata_json: row.try_get("metadata_json")?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }
    async fn billing_upsert_topup(&self, topup: &TopupRecord) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO topups (topup_id, wallet_id, tenant_id, amount_micros, currency, channel, provider_ref, status, idempotency_key, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT (topup_id) DO UPDATE SET status=excluded.status, provider_ref=excluded.provider_ref, updated_at=excluded.updated_at")
            .bind(&topup.topup_id).bind(&topup.wallet_id).bind(&topup.tenant_id).bind(topup.amount_micros).bind(&topup.currency).bind(&topup.channel).bind(&topup.provider_ref).bind(&topup.status).bind(&topup.idempotency_key).bind(&topup.created_at).bind(&topup.updated_at)
            .execute(&self.pool).await.map_err(StoreError::Database)?;
        Ok(())
    }
    async fn billing_get_topup_by_key(
        &self,
        tenant_id: &str,
        idempotency_key: &str,
    ) -> Result<Option<TopupRecord>, StoreError> {
        let row = sqlx::query("SELECT * FROM topups WHERE tenant_id=? AND idempotency_key=?")
            .bind(tenant_id)
            .bind(idempotency_key)
            .fetch_optional(&self.pool)
            .await
            .map_err(StoreError::Database)?;
        row.map(|row| -> Result<TopupRecord, sqlx::Error> {
            Ok(TopupRecord {
                topup_id: row.try_get("topup_id")?,
                wallet_id: row.try_get("wallet_id")?,
                tenant_id: row.try_get("tenant_id")?,
                amount_micros: row.try_get("amount_micros")?,
                currency: row.try_get("currency")?,
                channel: row.try_get("channel")?,
                provider_ref: row.try_get("provider_ref")?,
                status: row.try_get("status")?,
                idempotency_key: row.try_get("idempotency_key")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            })
        })
        .transpose()
        .map_err(StoreError::Database)
    }
    async fn billing_set_topup_status(
        &self,
        topup_id: &str,
        status: &str,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE topups SET status=?, updated_at=? WHERE topup_id=?")
            .bind(status)
            .bind(crate::util::now_rfc3339())
            .bind(topup_id)
            .execute(&self.pool)
            .await
            .map_err(StoreError::Database)?;
        Ok(())
    }
    async fn billing_upsert_plan(&self, plan: &PlanRecord) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO plans (plan_id, name, price_micros, included_tokens, currency, period, status, features_json, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?,?) ON CONFLICT (plan_id) DO UPDATE SET name=excluded.name, price_micros=excluded.price_micros, included_tokens=excluded.included_tokens, currency=excluded.currency, period=excluded.period, status=excluded.status, features_json=excluded.features_json, updated_at=excluded.updated_at")
            .bind(&plan.plan_id).bind(&plan.name).bind(plan.price_micros).bind(plan.included_tokens).bind(&plan.currency).bind(&plan.period).bind(&plan.status).bind(&plan.features_json).bind(&plan.created_at).bind(&plan.updated_at)
            .execute(&self.pool).await.map_err(StoreError::Database)?;
        Ok(())
    }
    async fn billing_get_plan(&self, plan_id: &str) -> Result<Option<PlanRecord>, StoreError> {
        let row = sqlx::query("SELECT * FROM plans WHERE plan_id=?")
            .bind(plan_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(StoreError::Database)?;
        row.map(|row| -> Result<PlanRecord, sqlx::Error> {
            Ok(PlanRecord {
                plan_id: row.try_get("plan_id")?,
                name: row.try_get("name")?,
                price_micros: row.try_get("price_micros")?,
                included_tokens: row.try_get("included_tokens")?,
                currency: row.try_get("currency")?,
                period: row.try_get("period")?,
                status: row.try_get("status")?,
                features_json: row.try_get("features_json")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            })
        })
        .transpose()
        .map_err(StoreError::Database)
    }
    async fn billing_list_plans(&self) -> Result<Vec<PlanRecord>, StoreError> {
        let rows = sqlx::query("SELECT * FROM plans ORDER BY price_micros")
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::Database)?;
        rows.iter()
            .map(|row| -> Result<PlanRecord, sqlx::Error> {
                Ok(PlanRecord {
                    plan_id: row.try_get("plan_id")?,
                    name: row.try_get("name")?,
                    price_micros: row.try_get("price_micros")?,
                    included_tokens: row.try_get("included_tokens")?,
                    currency: row.try_get("currency")?,
                    period: row.try_get("period")?,
                    status: row.try_get("status")?,
                    features_json: row.try_get("features_json")?,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }
    async fn billing_upsert_subscription(
        &self,
        subscription: &SubscriptionRecord,
    ) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO subscriptions (subscription_id, tenant_id, plan_id, status, trial_ends_at, current_period_start, current_period_end, past_due_at, suspended_at, cancelled_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT (subscription_id) DO UPDATE SET plan_id=excluded.plan_id, status=excluded.status, trial_ends_at=excluded.trial_ends_at, current_period_start=excluded.current_period_start, current_period_end=excluded.current_period_end, past_due_at=excluded.past_due_at, suspended_at=excluded.suspended_at, cancelled_at=excluded.cancelled_at, updated_at=excluded.updated_at")
            .bind(&subscription.subscription_id).bind(&subscription.tenant_id).bind(&subscription.plan_id).bind(&subscription.status).bind(&subscription.trial_ends_at).bind(&subscription.current_period_start).bind(&subscription.current_period_end).bind(&subscription.past_due_at).bind(&subscription.suspended_at).bind(&subscription.cancelled_at).bind(&subscription.updated_at)
            .execute(&self.pool).await.map_err(StoreError::Database)?;
        Ok(())
    }
    async fn billing_get_subscription(
        &self,
        tenant_id: &str,
    ) -> Result<Option<SubscriptionRecord>, StoreError> {
        let row = sqlx::query(
            "SELECT * FROM subscriptions WHERE tenant_id=? ORDER BY updated_at DESC LIMIT 1",
        )
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Database)?;
        row.map(|row| -> Result<SubscriptionRecord, sqlx::Error> {
            Ok(SubscriptionRecord {
                subscription_id: row.try_get("subscription_id")?,
                tenant_id: row.try_get("tenant_id")?,
                plan_id: row.try_get("plan_id")?,
                status: row.try_get("status")?,
                trial_ends_at: row.try_get("trial_ends_at")?,
                current_period_start: row.try_get("current_period_start")?,
                current_period_end: row.try_get("current_period_end")?,
                past_due_at: row.try_get("past_due_at")?,
                suspended_at: row.try_get("suspended_at")?,
                cancelled_at: row.try_get("cancelled_at")?,
                updated_at: row.try_get("updated_at")?,
            })
        })
        .transpose()
        .map_err(StoreError::Database)
    }
    async fn billing_upsert_entitlement(
        &self,
        entitlement: &EntitlementRecord,
    ) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO entitlements (tenant_id, plan_id, status, balance_micros, included_tokens, used_tokens, currency, period_start, period_end, updated_at) VALUES (?,?,?,?,?,?,?,?,?,?) ON CONFLICT (tenant_id) DO UPDATE SET plan_id=excluded.plan_id, status=excluded.status, balance_micros=excluded.balance_micros, included_tokens=excluded.included_tokens, used_tokens=excluded.used_tokens, currency=excluded.currency, period_start=excluded.period_start, period_end=excluded.period_end, updated_at=excluded.updated_at")
            .bind(&entitlement.tenant_id).bind(&entitlement.plan_id).bind(&entitlement.status).bind(entitlement.balance_micros).bind(entitlement.included_tokens).bind(entitlement.used_tokens).bind(&entitlement.currency).bind(&entitlement.period_start).bind(&entitlement.period_end).bind(&entitlement.updated_at)
            .execute(&self.pool).await.map_err(StoreError::Database)?;
        Ok(())
    }
    async fn billing_get_entitlement(
        &self,
        tenant_id: &str,
    ) -> Result<Option<EntitlementRecord>, StoreError> {
        let row = sqlx::query("SELECT * FROM entitlements WHERE tenant_id=?")
            .bind(tenant_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(StoreError::Database)?;
        row.map(|row| -> Result<EntitlementRecord, sqlx::Error> {
            Ok(EntitlementRecord {
                tenant_id: row.try_get("tenant_id")?,
                plan_id: row.try_get("plan_id")?,
                status: row.try_get("status")?,
                balance_micros: row.try_get("balance_micros")?,
                included_tokens: row.try_get("included_tokens")?,
                used_tokens: row.try_get("used_tokens")?,
                currency: row.try_get("currency")?,
                period_start: row.try_get("period_start")?,
                period_end: row.try_get("period_end")?,
                updated_at: row.try_get("updated_at")?,
            })
        })
        .transpose()
        .map_err(StoreError::Database)
    }
    async fn billing_insert_payment_event(
        &self,
        event: &PaymentEventRecord,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query("INSERT INTO payment_events (provider_event_id, provider, event_type, payload_json, status, received_at, processed_at) VALUES (?,?,?,?,?,?,?) ON CONFLICT (provider_event_id) DO NOTHING")
            .bind(&event.provider_event_id).bind(&event.provider).bind(&event.event_type).bind(&event.payload_json).bind(&event.status).bind(&event.received_at).bind(&event.processed_at)
            .execute(&self.pool).await.map_err(StoreError::Database)?;
        Ok(result.rows_affected() > 0)
    }
    async fn billing_mark_payment_event(
        &self,
        provider_event_id: &str,
        status: &str,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE payment_events SET status=?, processed_at=? WHERE provider_event_id=?")
            .bind(status)
            .bind(crate::util::now_rfc3339())
            .bind(provider_event_id)
            .execute(&self.pool)
            .await
            .map_err(StoreError::Database)?;
        Ok(())
    }
    async fn billing_upsert_invoice(&self, invoice: &InvoiceRecord) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO invoices (invoice_id, tenant_id, period_start, period_end, amount_micros, currency, status, created_at) VALUES (?,?,?,?,?,?,?,?) ON CONFLICT (invoice_id) DO NOTHING")
            .bind(&invoice.invoice_id).bind(&invoice.tenant_id).bind(&invoice.period_start).bind(&invoice.period_end).bind(invoice.amount_micros).bind(&invoice.currency).bind(&invoice.status).bind(&invoice.created_at)
            .execute(&self.pool).await.map_err(StoreError::Database)?;
        Ok(())
    }
    async fn billing_list_invoices(
        &self,
        tenant_id: &str,
        limit: i64,
    ) -> Result<Vec<InvoiceRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT * FROM invoices WHERE tenant_id=? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Database)?;
        rows.iter()
            .map(|row| -> Result<InvoiceRecord, sqlx::Error> {
                Ok(InvoiceRecord {
                    invoice_id: row.try_get("invoice_id")?,
                    tenant_id: row.try_get("tenant_id")?,
                    period_start: row.try_get("period_start")?,
                    period_end: row.try_get("period_end")?,
                    amount_micros: row.try_get("amount_micros")?,
                    currency: row.try_get("currency")?,
                    status: row.try_get("status")?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }

    async fn connector_list(&self, tenant_id: &str) -> Result<Vec<ConnectorRecord>, StoreError> {
        let rows = sqlx::query("SELECT * FROM connectors WHERE tenant_id=? ORDER BY name")
            .bind(tenant_id)
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::Database)?;
        rows.iter()
            .map(|row| -> Result<ConnectorRecord, sqlx::Error> {
                Ok(ConnectorRecord {
                    connector_id: row.try_get("connector_id")?,
                    tenant_id: row.try_get("tenant_id")?,
                    space_id: row.try_get("space_id")?,
                    kind: row.try_get("kind")?,
                    name: row.try_get("name")?,
                    config_json: row.try_get("config_json")?,
                    secret_env: row.try_get("secret_env")?,
                    status: row.try_get("status")?,
                    last_sync_at: row.try_get("last_sync_at")?,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }
    async fn connector_get(
        &self,
        tenant_id: &str,
        connector_id: &str,
    ) -> Result<Option<ConnectorRecord>, StoreError> {
        let row = sqlx::query("SELECT * FROM connectors WHERE tenant_id=? AND connector_id=?")
            .bind(tenant_id)
            .bind(connector_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(StoreError::Database)?;
        row.map(|row| -> Result<ConnectorRecord, sqlx::Error> {
            Ok(ConnectorRecord {
                connector_id: row.try_get("connector_id")?,
                tenant_id: row.try_get("tenant_id")?,
                space_id: row.try_get("space_id")?,
                kind: row.try_get("kind")?,
                name: row.try_get("name")?,
                config_json: row.try_get("config_json")?,
                secret_env: row.try_get("secret_env")?,
                status: row.try_get("status")?,
                last_sync_at: row.try_get("last_sync_at")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            })
        })
        .transpose()
        .map_err(StoreError::Database)
    }
    async fn connector_upsert(&self, connector: &ConnectorRecord) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO connectors (connector_id, tenant_id, space_id, kind, name, config_json, secret_env, status, last_sync_at, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT (connector_id) DO UPDATE SET space_id=excluded.space_id, kind=excluded.kind, name=excluded.name, config_json=excluded.config_json, secret_env=excluded.secret_env, status=excluded.status, updated_at=excluded.updated_at")
            .bind(&connector.connector_id).bind(&connector.tenant_id).bind(&connector.space_id).bind(&connector.kind).bind(&connector.name).bind(&connector.config_json).bind(&connector.secret_env).bind(&connector.status).bind(&connector.last_sync_at).bind(&connector.created_at).bind(&connector.updated_at)
            .execute(&self.pool).await.map_err(StoreError::Database)?;
        Ok(())
    }
    async fn connector_delete(
        &self,
        tenant_id: &str,
        connector_id: &str,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query("DELETE FROM connectors WHERE tenant_id=? AND connector_id=?")
            .bind(tenant_id)
            .bind(connector_id)
            .execute(&self.pool)
            .await
            .map_err(StoreError::Database)?;
        Ok(result.rows_affected() > 0)
    }
    async fn connector_mark_synced(
        &self,
        tenant_id: &str,
        connector_id: &str,
        at: &str,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE connectors SET last_sync_at=?, updated_at=? WHERE tenant_id=? AND connector_id=?").bind(at).bind(crate::util::now_rfc3339()).bind(tenant_id).bind(connector_id).execute(&self.pool).await.map_err(StoreError::Database)?;
        Ok(())
    }
    async fn external_ref_upsert(&self, reference: &ExternalRefRecord) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO external_refs (tenant_id, space_id, doc_id, provider, external_id, url, revision, acl_json, fetched_at, status) VALUES (?,?,?,?,?,?,?,?,?,?) ON CONFLICT (tenant_id, provider, external_id) DO UPDATE SET space_id=excluded.space_id, doc_id=excluded.doc_id, url=excluded.url, revision=excluded.revision, acl_json=excluded.acl_json, fetched_at=excluded.fetched_at, status=excluded.status")
            .bind(&reference.tenant_id).bind(&reference.space_id).bind(&reference.doc_id).bind(&reference.provider).bind(&reference.external_id).bind(&reference.url).bind(&reference.revision).bind(&reference.acl_json).bind(&reference.fetched_at).bind(&reference.status)
            .execute(&self.pool).await.map_err(StoreError::Database)?;
        Ok(())
    }
    async fn external_ref_list(
        &self,
        tenant_id: &str,
        doc_id: Option<&str>,
        provider: Option<&str>,
        limit: i64,
    ) -> Result<Vec<ExternalRefRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT * FROM external_refs WHERE tenant_id=? ORDER BY fetched_at DESC LIMIT 500",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Database)?;
        let mut refs = rows
            .iter()
            .map(|row| -> Result<ExternalRefRecord, sqlx::Error> {
                Ok(ExternalRefRecord {
                    tenant_id: row.try_get("tenant_id")?,
                    space_id: row.try_get("space_id")?,
                    doc_id: row.try_get("doc_id")?,
                    provider: row.try_get("provider")?,
                    external_id: row.try_get("external_id")?,
                    url: row.try_get("url")?,
                    revision: row.try_get("revision")?,
                    acl_json: row.try_get("acl_json")?,
                    fetched_at: row.try_get("fetched_at")?,
                    status: row.try_get("status")?,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)?;
        if let Some(doc_id) = doc_id {
            refs.retain(|reference| reference.doc_id == doc_id);
        }
        if let Some(provider) = provider {
            refs.retain(|reference| reference.provider == provider);
        }
        refs.truncate(limit.max(1) as usize);
        Ok(refs)
    }
    async fn connector_job_create(&self, job: &ConnectorJobRecord) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO connector_jobs (job_id, connector_id, status, started_at, finished_at, stats_json, error) VALUES (?,?,?,?,?,?,?) ON CONFLICT (job_id) DO NOTHING")
            .bind(&job.job_id).bind(&job.connector_id).bind(&job.status).bind(&job.started_at).bind(&job.finished_at).bind(&job.stats_json).bind(&job.error)
            .execute(&self.pool).await.map_err(StoreError::Database)?;
        Ok(())
    }
    async fn connector_job_update(
        &self,
        job_id: &str,
        status: &str,
        finished_at: Option<&str>,
        stats_json: &str,
        error: Option<&str>,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE connector_jobs SET status=?, finished_at=?, stats_json=?, error=? WHERE job_id=?").bind(status).bind(finished_at).bind(stats_json).bind(error).bind(job_id).execute(&self.pool).await.map_err(StoreError::Database)?;
        Ok(())
    }
    async fn connector_job_list(
        &self,
        connector_id: &str,
        limit: i64,
    ) -> Result<Vec<ConnectorJobRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT * FROM connector_jobs WHERE connector_id=? ORDER BY started_at DESC LIMIT ?",
        )
        .bind(connector_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Database)?;
        rows.iter()
            .map(|row| -> Result<ConnectorJobRecord, sqlx::Error> {
                Ok(ConnectorJobRecord {
                    job_id: row.try_get("job_id")?,
                    connector_id: row.try_get("connector_id")?,
                    status: row.try_get("status")?,
                    started_at: row.try_get("started_at")?,
                    finished_at: row.try_get("finished_at")?,
                    stats_json: row.try_get("stats_json")?,
                    error: row.try_get("error")?,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }

    async fn get_user_by_email(
        &self,
        tenant_id: &str,
        email: &str,
    ) -> Result<Option<UserRecord>, StoreError> {
        let row = sqlx::query("SELECT * FROM users WHERE tenant_id=? AND email=?")
            .bind(tenant_id)
            .bind(email)
            .fetch_optional(&self.pool)
            .await
            .map_err(StoreError::Database)?;
        row.map(|row| user_from_row(&row))
            .transpose()
            .map_err(StoreError::Database)
    }
    async fn get_user_by_external_id(
        &self,
        tenant_id: &str,
        source: &str,
        external_id: &str,
    ) -> Result<Option<UserRecord>, StoreError> {
        let row =
            sqlx::query("SELECT * FROM users WHERE tenant_id=? AND source=? AND external_id=?")
                .bind(tenant_id)
                .bind(source)
                .bind(external_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(StoreError::Database)?;
        row.map(|row| user_from_row(&row))
            .transpose()
            .map_err(StoreError::Database)
    }
    async fn create_sso_user(&self, user: &UserRecord) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO users (tenant_id, user_id, username, password_hash, display_name, created_at, disabled, role, email, external_id, source, last_sso_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&user.tenant_id).bind(&user.user_id).bind(&user.username).bind(&user.password_hash).bind(&user.display_name).bind(&user.created_at).bind(if user.disabled {1i64} else {0i64}).bind(user.role.as_str()).bind(&user.email).bind(&user.external_id).bind(&user.source).bind(&user.last_sso_at)
            .execute(&self.pool).await.map_err(StoreError::Database)?;
        Ok(())
    }
    async fn link_sso_identity(
        &self,
        tenant_id: &str,
        user_id: &str,
        email: &str,
        external_id: &str,
        last_sso_at: &str,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE users SET email=COALESCE(email, ?), external_id=COALESCE(external_id, ?), source=CASE WHEN source='local' THEN 'sso' ELSE source END, last_sso_at=? WHERE tenant_id=? AND user_id=?")
            .bind(email).bind(external_id).bind(last_sso_at).bind(tenant_id).bind(user_id).execute(&self.pool).await.map_err(StoreError::Database)?;
        Ok(())
    }
    async fn touch_sso_login(
        &self,
        tenant_id: &str,
        user_id: &str,
        last_sso_at: &str,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE users SET last_sso_at=? WHERE tenant_id=? AND user_id=?")
            .bind(last_sso_at)
            .bind(tenant_id)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(StoreError::Database)?;
        Ok(())
    }
    async fn touch_user_login(
        &self,
        tenant_id: &str,
        user_id: &str,
        at: &str,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE users SET last_login_at=? WHERE tenant_id=? AND user_id=?")
            .bind(at)
            .bind(tenant_id)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(StoreError::Database)?;
        Ok(())
    }
    async fn set_user_password(
        &self,
        tenant_id: &str,
        user_id: &str,
        password_hash: &str,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE users SET password_hash=? WHERE tenant_id=? AND user_id=?")
            .bind(password_hash)
            .bind(tenant_id)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(StoreError::Database)?;
        Ok(())
    }
    async fn get_audit_event(
        &self,
        tenant_id: &str,
        event_id: &str,
    ) -> Result<Option<AuditEventRecord>, StoreError> {
        let row = sqlx::query("SELECT * FROM audit_events WHERE tenant_id=? AND event_id=?")
            .bind(tenant_id)
            .bind(event_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(StoreError::Database)?;
        row.map(|row| -> Result<AuditEventRecord, sqlx::Error> {
            Ok(AuditEventRecord {
                event_id: row.try_get("event_id")?,
                tenant_id: row.try_get("tenant_id")?,
                space_id: row.try_get("space_id")?,
                actor_id: row.try_get("actor_id")?,
                actor_type: row.try_get("actor_type")?,
                action: row.try_get("action")?,
                target_type: row.try_get("target_type")?,
                target_id: row.try_get("target_id")?,
                result: row.try_get("result")?,
                status_code: row.try_get("status_code")?,
                request_id: row.try_get("request_id")?,
                ip: row.try_get("ip")?,
                user_agent: row.try_get("user_agent")?,
                detail_json: row.try_get("detail_json")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .transpose()
        .map_err(StoreError::Database)
    }
    async fn billing_list_subscriptions(
        &self,
        tenant_id: &str,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<SubscriptionRecord>, StoreError> {
        let rows = if let Some(status) = status {
            sqlx::query("SELECT * FROM subscriptions WHERE tenant_id=? AND status=? ORDER BY updated_at DESC LIMIT ? OFFSET ?").bind(tenant_id).bind(status).bind(limit).bind(offset).fetch_all(&self.pool).await
        } else {
            sqlx::query("SELECT * FROM subscriptions WHERE tenant_id=? ORDER BY updated_at DESC LIMIT ? OFFSET ?").bind(tenant_id).bind(limit).bind(offset).fetch_all(&self.pool).await
        }.map_err(StoreError::Database)?;
        rows.iter()
            .map(|row| -> Result<SubscriptionRecord, sqlx::Error> {
                Ok(SubscriptionRecord {
                    subscription_id: row.try_get("subscription_id")?,
                    tenant_id: row.try_get("tenant_id")?,
                    plan_id: row.try_get("plan_id")?,
                    status: row.try_get("status")?,
                    trial_ends_at: row.try_get("trial_ends_at")?,
                    current_period_start: row.try_get("current_period_start")?,
                    current_period_end: row.try_get("current_period_end")?,
                    past_due_at: row.try_get("past_due_at")?,
                    suspended_at: row.try_get("suspended_at")?,
                    cancelled_at: row.try_get("cancelled_at")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }
    async fn billing_count_active_subscriptions(&self, plan_id: &str) -> Result<i64, StoreError> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM subscriptions WHERE plan_id=? AND status IN ('trial','active','past_due')").bind(plan_id).fetch_one(&self.pool).await.map_err(StoreError::Database)?;
        Ok(count)
    }
    async fn billing_delete_plan(&self, plan_id: &str) -> Result<bool, StoreError> {
        let result = sqlx::query("DELETE FROM plans WHERE plan_id=?")
            .bind(plan_id)
            .execute(&self.pool)
            .await
            .map_err(StoreError::Database)?;
        Ok(result.rows_affected() > 0)
    }
    async fn update_provider_model_record(
        &self,
        _tenant_id: &str,
        model: &ProviderModelRecord,
    ) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO provider_models (provider_model_id, provider_id, upstream_model, context_window, pricing_json, capabilities_json, status, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?) ON CONFLICT (provider_model_id) DO UPDATE SET provider_id=excluded.provider_id, upstream_model=excluded.upstream_model, context_window=excluded.context_window, pricing_json=excluded.pricing_json, capabilities_json=excluded.capabilities_json, status=excluded.status, updated_at=excluded.updated_at")
            .bind(&model.provider_model_id).bind(&model.provider_id).bind(&model.upstream_model).bind(model.context_window).bind(&model.pricing_json).bind(&model.capabilities_json).bind(&model.status).bind(&model.created_at).bind(&model.updated_at)
            .execute(&self.pool).await.map_err(StoreError::Database)?;
        Ok(())
    }
    async fn delete_provider_model(
        &self,
        _tenant_id: &str,
        provider_model_id: &str,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query("DELETE FROM provider_models WHERE provider_model_id=?")
            .bind(provider_model_id)
            .execute(&self.pool)
            .await
            .map_err(StoreError::Database)?;
        Ok(result.rows_affected() > 0)
    }
    async fn admin_overview(
        &self,
        tenant_id: &str,
        since: &str,
    ) -> Result<AdminOverview, StoreError> {
        let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE tenant_id=?")
            .bind(tenant_id)
            .fetch_one(&self.pool)
            .await
            .map_err(StoreError::Database)?;
        let spaces: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM spaces WHERE tenant_id=? AND status='active'")
                .bind(tenant_id)
                .fetch_one(&self.pool)
                .await
                .map_err(StoreError::Database)?;
        let documents: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM documents WHERE tenant_id=? AND valid_to IS NULL",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Database)?;
        let chunks: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM chunks WHERE tenant_id=? AND valid_to IS NULL",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Database)?;
        let requests: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM usage_events WHERE tenant_id=? AND at>=?")
                .bind(tenant_id)
                .bind(since)
                .fetch_one(&self.pool)
                .await
                .map_err(StoreError::Database)?;
        let errors: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM usage_events WHERE tenant_id=? AND at>=? AND status<>'success'",
        )
        .bind(tenant_id)
        .bind(since)
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Database)?;
        let cost: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(cost_micros),0) FROM usage_events WHERE tenant_id=? AND at>=?",
        )
        .bind(tenant_id)
        .bind(since)
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Database)?;
        let hits: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(CASE WHEN cache_hit THEN 1 ELSE 0 END),0) FROM usage_events WHERE tenant_id=? AND at>=?").bind(tenant_id).bind(since).fetch_one(&self.pool).await.map_err(StoreError::Database)?;
        Ok(AdminOverview {
            users,
            spaces,
            documents,
            chunks,
            requests_24h: requests,
            errors_24h: errors,
            cost_micros_24h: cost,
            cache_hits_24h: hits,
        })
    }

    async fn set_user_display_name(
        &self,
        tenant_id: &str,
        user_id: &str,
        display_name: &str,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE users SET display_name=? WHERE tenant_id=? AND user_id=?")
            .bind(display_name)
            .bind(tenant_id)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(StoreError::Database)?;
        Ok(())
    }
}
