//! Audit event writer/query helper.
//!
//! Audit writes are best effort for domain handlers: a storage failure is
//! logged but never masks the actual response. Security failures still call
//! this module and only the durable copy is best effort.

use crate::store::{AuditEventRecord, AuditFilter, Store};
use std::sync::Arc;

#[allow(clippy::too_many_arguments)]
pub async fn record(
    store: &Arc<dyn Store>,
    tenant_id: &str,
    space_id: Option<&str>,
    actor_id: &str,
    actor_type: &str,
    action: &str,
    target_type: &str,
    target_id: &str,
    result: &str,
    status_code: u16,
    request_id: Option<&str>,
    ip: Option<&str>,
    user_agent: Option<&str>,
    detail: serde_json::Value,
) {
    let event = AuditEventRecord {
        event_id: format!("aud_{}", crate::util::random_hex(12)),
        tenant_id: tenant_id.to_string(),
        space_id: space_id.map(|value| value.to_string()),
        actor_id: actor_id.to_string(),
        actor_type: actor_type.to_string(),
        action: action.to_string(),
        target_type: target_type.to_string(),
        target_id: target_id.to_string(),
        result: result.to_string(),
        status_code: status_code as i64,
        request_id: request_id.map(|value| value.to_string()),
        ip: ip.map(|value| value.to_string()),
        user_agent: user_agent.map(|value| value.to_string()),
        detail_json: detail.to_string(),
        created_at: crate::util::now_rfc3339(),
    };
    if let Err(err) = store.insert_audit(&event).await {
        tracing::warn!(error = %err, action = %event.action, "audit write failed");
    }
}

pub async fn query(
    store: &Arc<dyn Store>,
    filter: &AuditFilter,
) -> Result<Vec<AuditEventRecord>, crate::error::StoreError> {
    store.list_audit(filter).await
}
