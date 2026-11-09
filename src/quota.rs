//! Storage quota checks, reservations and usage refresh.
//!
//! Storage accounting is per space. Tenant/group/user checks aggregate the
//! space rows, which keeps SQLite and PostgreSQL on the same semantics and
//! makes archived/deleted spaces remain counted until a future purge job.

use crate::config::SpacesConfig;
use crate::error::{AppError, StoreError};
use crate::governance::SpaceRole;
use crate::store::{QuotaRecord, QuotaScope, SpaceRecord, StorageUsage, Store};
use axum::http::StatusCode;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::{Mutex, MutexGuard};

static WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Serializes quota check + reserve + write + refresh within one process.
/// Multi-instance deployments need a shared lock/advisory lock (documented
/// P1 item); this keeps SQLite and a single PG instance from overselling.
pub async fn write_guard() -> MutexGuard<'static, ()> {
    WRITE_LOCK.get_or_init(|| Mutex::new(())).lock().await
}

#[derive(Debug, Clone, Copy, Default)]
pub struct QuotaDelta {
    pub source_bytes: i64,
    pub index_bytes: i64,
    pub history_bytes: i64,
    pub documents: i64,
    pub chunks: i64,
}

impl QuotaDelta {
    pub fn total_bytes(&self) -> i64 {
        self.source_bytes + self.index_bytes + self.history_bytes
    }
}

#[derive(Debug, Clone)]
pub struct QuotaError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

impl QuotaError {
    pub fn to_app_error(&self) -> AppError {
        AppError::new(self.status, self.code, self.message.clone())
    }
}

fn bytes_error(is_tenant: bool, dimension: &str, limit: i64, used: i64) -> QuotaError {
    QuotaError {
        status: StatusCode::PAYLOAD_TOO_LARGE,
        code: if is_tenant {
            "tenant_quota_exceeded"
        } else {
            "space_quota_exceeded"
        },
        message: format!("{dimension} quota exceeded: limit={limit}, used={used}"),
    }
}

fn count_error(is_tenant: bool, dimension: &str, limit: i64, used: i64) -> QuotaError {
    QuotaError {
        status: StatusCode::TOO_MANY_REQUESTS,
        code: if is_tenant {
            "tenant_quota_exceeded"
        } else {
            "space_quota_exceeded"
        },
        message: format!("{dimension} quota exceeded: limit={limit}, used={used}"),
    }
}

async fn usage_map(
    store: &Arc<dyn Store>,
    tenant_id: &str,
) -> Result<HashMap<String, StorageUsage>, StoreError> {
    Ok(store
        .list_storage_usage(tenant_id)
        .await?
        .into_iter()
        .map(|usage| (usage.space_id.clone(), usage))
        .collect())
}

fn aggregate_all(map: &HashMap<String, StorageUsage>) -> StorageUsage {
    let mut total = StorageUsage::zero("", "", "");
    for usage in map.values() {
        total.source_bytes += usage.source_bytes;
        total.index_bytes += usage.index_bytes;
        total.history_bytes += usage.history_bytes;
        total.total_bytes += usage.total_bytes;
        total.reserved_bytes += usage.reserved_bytes;
        total.documents += usage.documents;
        total.chunks += usage.chunks;
        total.members += usage.members;
    }
    total
}

fn aggregate(
    map: &HashMap<String, StorageUsage>,
    spaces: &[SpaceRecord],
    owner_type: &str,
    owner_id: &str,
) -> StorageUsage {
    let mut total = StorageUsage::zero("", "", "");
    for space in spaces {
        if space.owner_type != owner_type || space.owner_id != owner_id {
            continue;
        }
        if let Some(usage) = map.get(&space.space_id) {
            total.source_bytes += usage.source_bytes;
            total.index_bytes += usage.index_bytes;
            total.history_bytes += usage.history_bytes;
            total.total_bytes += usage.total_bytes;
            total.reserved_bytes += usage.reserved_bytes;
            total.documents += usage.documents;
            total.chunks += usage.chunks;
            total.members += usage.members;
        }
    }
    total
}

async fn limit_for(
    store: &Arc<dyn Store>,
    tenant_id: &str,
    scope: QuotaScope,
    scope_id: &str,
    dimension: &str,
    now: &str,
) -> Result<Option<(i64, String)>, StoreError> {
    let overrides = store
        .list_quota_overrides(tenant_id, scope, scope_id, dimension, now)
        .await?;
    if let Some(over) = overrides.first() {
        if over.limit_value >= 0 {
            return Ok(Some((over.limit_value, "hard".to_string())));
        }
    }
    let Some(quota) = store.get_quota(tenant_id, scope, scope_id).await? else {
        return Ok(None);
    };
    let value = match dimension {
        "storage_bytes" => quota.storage_bytes,
        "documents" => quota.documents,
        "chunks" => quota.chunks,
        "members" => quota.members,
        "spaces" => quota.spaces,
        _ => None,
    };
    Ok(value.map(|limit| (limit, quota.enforcement.clone())))
}

fn check(
    scope: QuotaScope,
    dimension: &str,
    usage: &StorageUsage,
    delta: i64,
    limit: Option<(i64, String)>,
) -> Result<(), QuotaError> {
    let Some((limit, enforcement)) = limit else {
        return Ok(());
    };
    if enforcement != "hard" {
        return Ok(());
    }
    let is_tenant = scope == QuotaScope::Tenant;
    let used = match dimension {
        "storage_bytes" => usage.total_bytes + usage.reserved_bytes,
        "documents" => usage.documents,
        "chunks" => usage.chunks,
        "members" => usage.members,
        _ => 0,
    };
    if used + delta <= limit {
        return Ok(());
    }
    if dimension == "members" || dimension == "spaces" {
        Err(count_error(is_tenant, dimension, limit, used + delta))
    } else {
        Err(bytes_error(is_tenant, dimension, limit, used + delta))
    }
}

/// Check all levels applicable to a space write. Does not reserve.
pub async fn check_space_quota(
    store: &Arc<dyn Store>,
    space: &SpaceRecord,
    delta: &QuotaDelta,
) -> Result<(), QuotaError> {
    let tenant = space.tenant_id.as_str();
    let now = crate::util::now_rfc3339();
    let spaces = store.list_spaces(tenant).await.map_err(to_quota_error)?;
    let map = usage_map(store, tenant).await.map_err(to_quota_error)?;
    let space_usage = map
        .get(&space.space_id)
        .cloned()
        .unwrap_or_else(|| StorageUsage::zero(tenant, &space.space_id, &now));

    let mut scopes: Vec<(QuotaScope, String, StorageUsage)> = vec![
        (
            QuotaScope::Tenant,
            tenant.to_string(),
            aggregate_all(&map),
        ),
        (
            QuotaScope::Space,
            space.space_id.clone(),
            space_usage.clone(),
        ),
    ];
    if space.owner_type == "group" {
        scopes.push((
            QuotaScope::Group,
            space.owner_id.clone(),
            aggregate(&map, &spaces, "group", &space.owner_id),
        ));
    } else {
        scopes.push((
            QuotaScope::User,
            space.owner_id.clone(),
            aggregate(&map, &spaces, "user", &space.owner_id),
        ));
    }

    for (scope, scope_id, usage) in scopes {
        for (dimension, delta_value) in [
            ("storage_bytes", delta.total_bytes()),
            ("documents", delta.documents),
            ("chunks", delta.chunks),
        ] {
            let limit = limit_for(store, tenant, scope, &scope_id, dimension, &now)
                .await
                .map_err(to_quota_error)?;
            check(scope, dimension, &usage, delta_value, limit)?;
        }
    }
    Ok(())
}

fn to_quota_error(err: StoreError) -> QuotaError {
    QuotaError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        code: "quota_check_failed",
        message: err.to_string(),
    }
}

/// Reserve space for a pending write after checking every applicable limit.
pub async fn reserve(
    store: &Arc<dyn Store>,
    space: &SpaceRecord,
    delta: &QuotaDelta,
) -> Result<(), QuotaError> {
    check_space_quota(store, space, delta).await?;
    store
        .add_reserved_bytes(
            &space.tenant_id,
            &space.space_id,
            delta.total_bytes(),
            &crate::util::now_rfc3339(),
        )
        .await
        .map_err(to_quota_error)
}

/// Release a reservation after failed/unchanged writes.
pub async fn release(store: &Arc<dyn Store>, space: &SpaceRecord, delta: &QuotaDelta) {
    let _ = store
        .add_reserved_bytes(
            &space.tenant_id,
            &space.space_id,
            -delta.total_bytes(),
            &crate::util::now_rfc3339(),
        )
        .await;
}

/// Recompute exact usage after a committed write.
pub async fn refresh(
    store: &Arc<dyn Store>,
    tenant_id: &str,
    space_id: &str,
) -> Result<StorageUsage, StoreError> {
    store.recompute_storage_usage(tenant_id, space_id).await
}

pub async fn check_member_quota(
    store: &Arc<dyn Store>,
    space: &SpaceRecord,
    additional: i64,
) -> Result<(), QuotaError> {
    let now = crate::util::now_rfc3339();
    let usage = store
        .get_storage_usage(&space.tenant_id, &space.space_id)
        .await
        .map_err(to_quota_error)?
        .unwrap_or_else(|| StorageUsage::zero(&space.tenant_id, &space.space_id, &now));
    for scope in [QuotaScope::Space, QuotaScope::Tenant] {
        let scope_id = if scope == QuotaScope::Tenant {
            space.tenant_id.clone()
        } else {
            space.space_id.clone()
        };
        let limit = limit_for(store, &space.tenant_id, scope, &scope_id, "members", &now)
            .await
            .map_err(to_quota_error)?;
        check(scope, "members", &usage, additional, limit)?;
    }
    Ok(())
}

pub async fn check_space_creation(
    store: &Arc<dyn Store>,
    tenant_id: &str,
    owner_type: &str,
    owner_id: &str,
    max_per_user: u32,
) -> Result<(), QuotaError> {
    let spaces = store.list_spaces(tenant_id).await.map_err(to_quota_error)?;
    let owned = spaces
        .iter()
        .filter(|space| {
            space.status != "deleted"
                && space.owner_type == owner_type
                && space.owner_id == owner_id
        })
        .count() as i64;
    if max_per_user > 0 && owned >= max_per_user as i64 {
        return Err(count_error(
            false,
            "spaces",
            max_per_user as i64,
            owned + 1,
        ));
    }
    let now = crate::util::now_rfc3339();
    let scope = if owner_type == "group" {
        QuotaScope::Group
    } else {
        QuotaScope::User
    };
    if let Some((limit, enforcement)) =
        limit_for(store, tenant_id, scope, owner_id, "spaces", &now)
            .await
            .map_err(to_quota_error)?
    {
        if enforcement == "hard" && owned + 1 > limit {
            return Err(count_error(false, "spaces", limit, owned + 1));
        }
    }
    if let Some((limit, enforcement)) =
        limit_for(store, tenant_id, QuotaScope::Tenant, tenant_id, "spaces", &now)
            .await
            .map_err(to_quota_error)?
    {
        let used = spaces.iter().filter(|s| s.status != "deleted").count() as i64;
        if enforcement == "hard" && used + 1 > limit {
            return Err(count_error(true, "spaces", limit, used + 1));
        }
    }
    Ok(())
}

pub async fn apply_default_quota(
    store: &Arc<dyn Store>,
    tenant_id: &str,
    space_id: &str,
    actor: &str,
    config: &SpacesConfig,
) -> Result<(), StoreError> {
    if config.default_storage_bytes.is_none()
        && config.default_documents.is_none()
        && config.default_chunks.is_none()
        && config.default_members.is_none()
    {
        return Ok(());
    }
    store
        .upsert_quota(&QuotaRecord {
            tenant_id: tenant_id.to_string(),
            scope_type: QuotaScope::Space,
            scope_id: space_id.to_string(),
            storage_bytes: config.default_storage_bytes,
            documents: config.default_documents,
            chunks: config.default_chunks,
            members: config.default_members,
            spaces: None,
            enforcement: "hard".to_string(),
            updated_by: actor.to_string(),
            updated_at: crate::util::now_rfc3339(),
        })
        .await
}

/// Effective role helper kept here to avoid a second import path.
pub fn role_allows(access: Option<SpaceRole>, minimum: SpaceRole) -> bool {
    access.map(|role| role >= minimum).unwrap_or(false)
}
