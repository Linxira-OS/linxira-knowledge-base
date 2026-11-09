//! Token/request/cost quota reservation for model gateway calls.

use crate::error::AppError;
use crate::governance::Principal;
use crate::store::{QuotaReservation, Store, TokenQuotaRecord};
use axum::http::StatusCode;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Estimate {
    pub input_tokens: i64,
    pub max_output_tokens: i64,
    pub cost_micros: i64,
    pub currency: String,
}

#[derive(Debug, Clone)]
pub struct TokenQuotaError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub retry_after: Option<u64>,
}

impl TokenQuotaError {
    pub fn to_app_error(&self) -> AppError {
        let mut error = AppError::new(self.status, self.code, self.message.clone());
        if let Some(retry) = self.retry_after {
            error = error.with_retry_after(retry);
        }
        error
    }
}

fn internal(err: impl ToString) -> TokenQuotaError {
    TokenQuotaError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        code: "quota_check_failed",
        message: err.to_string(),
        retry_after: None,
    }
}

fn period_start(period: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    match period {
        "day" => now.format("%Y-%m-%d").to_string(),
        "month" => now.format("%Y-%m-01").to_string(),
        _ => "total".to_string(),
    }
}

fn applies(
    quota: &TokenQuotaRecord,
    principal: &Principal,
    space: Option<&str>,
    role: &str,
) -> bool {
    match quota.scope_type.as_str() {
        "tenant" => quota.scope_id == principal.tenant_id,
        "space" => space.map(|space| quota.scope_id == space).unwrap_or(false),
        "user" => quota.scope_id == principal.principal_id,
        "role" => quota.scope_id == role,
        _ => false,
    }
}

fn exceeded(
    quota: &TokenQuotaRecord,
    counter: &crate::store::QuotaCounter,
    estimate: &Estimate,
) -> Option<&'static str> {
    if let Some(limit) = quota.limit_tokens {
        if counter.used_tokens
            + counter.reserved_tokens
            + estimate.input_tokens
            + estimate.max_output_tokens
            > limit
        {
            return Some("tokens");
        }
    }
    if let Some(limit) = quota.limit_requests {
        if counter.used_requests + counter.reserved_requests + 1 > limit {
            return Some("requests");
        }
    }
    if let Some(limit) = quota.limit_cost_micros {
        if counter.used_cost_micros + counter.reserved_cost_micros + estimate.cost_micros > limit {
            return Some("cost");
        }
    }
    None
}

/// Reserve estimated tokens/requests/cost on every applicable hard quota.
pub async fn reserve(
    store: &Arc<dyn Store>,
    principal: &Principal,
    space: Option<&str>,
    role: &str,
    request_id: &str,
    estimate: &Estimate,
) -> Result<(), TokenQuotaError> {
    let _guard = crate::quota::write_guard().await;
    let _ = store
        .cleanup_expired_quota_reservations(&principal.tenant_id, &crate::util::now_rfc3339())
        .await;
    let quotas = store
        .list_token_quotas(&principal.tenant_id)
        .await
        .map_err(internal)?;
    let now = chrono::Utc::now();
    let applicable: Vec<&TokenQuotaRecord> = quotas
        .iter()
        .filter(|quota| quota.enabled && applies(quota, principal, space, role))
        .collect();
    if applicable.is_empty() {
        return Ok(());
    }

    // Check all hard limits before writing any reservation.
    for quota in &applicable {
        let start = period_start(&quota.period, now);
        let counter = store
            .get_quota_counter(
                &principal.tenant_id,
                &quota.scope_type,
                &quota.scope_id,
                &start,
            )
            .await
            .map_err(internal)?
            .unwrap_or_else(|| crate::store::QuotaCounter {
                tenant_id: principal.tenant_id.clone(),
                scope_type: quota.scope_type.clone(),
                scope_id: quota.scope_id.clone(),
                period_start: start.clone(),
                reserved_tokens: 0,
                used_tokens: 0,
                reserved_requests: 0,
                used_requests: 0,
                reserved_cost_micros: 0,
                used_cost_micros: 0,
                updated_at: String::new(),
            });
        if quota.enforcement == "soft" {
            if let Some(dimension) = exceeded(quota, &counter, estimate) {
                crate::audit::record(
                    store,
                    &principal.tenant_id,
                    space,
                    &principal.principal_id,
                    "user",
                    "quota.soft_exceeded",
                    "token_quota",
                    &quota.quota_id,
                    "alert",
                    200,
                    Some(request_id),
                    None,
                    None,
                    serde_json::json!({"dimension": dimension, "scope_type": quota.scope_type, "scope_id": quota.scope_id}),
                )
                .await;
                tracing::warn!(quota_id = %quota.quota_id, dimension, "token quota soft limit exceeded");
            }
            continue;
        }
        if quota.enforcement != "hard" || quota.action != "block" {
            continue;
        }
        if let Some(limit) = quota.limit_tokens {
            if counter.used_tokens
                + counter.reserved_tokens
                + estimate.input_tokens
                + estimate.max_output_tokens
                > limit
            {
                return Err(TokenQuotaError {
                    status: StatusCode::TOO_MANY_REQUESTS,
                    code: "quota_exceeded",
                    message: format!(
                        "token quota exceeded for {}/{}",
                        quota.scope_type, quota.scope_id
                    ),
                    retry_after: Some(60),
                });
            }
        }
        if let Some(limit) = quota.limit_requests {
            if counter.used_requests + counter.reserved_requests + 1 > limit {
                return Err(TokenQuotaError {
                    status: StatusCode::TOO_MANY_REQUESTS,
                    code: "quota_exceeded",
                    message: format!(
                        "request quota exceeded for {}/{}",
                        quota.scope_type, quota.scope_id
                    ),
                    retry_after: Some(60),
                });
            }
        }
        if let Some(limit) = quota.limit_cost_micros {
            if counter.used_cost_micros + counter.reserved_cost_micros + estimate.cost_micros
                > limit
            {
                return Err(TokenQuotaError {
                    status: StatusCode::TOO_MANY_REQUESTS,
                    code: "quota_exceeded",
                    message: format!(
                        "cost quota exceeded for {}/{}",
                        quota.scope_type, quota.scope_id
                    ),
                    retry_after: Some(60),
                });
            }
        }
    }

    let mut added: Vec<String> = Vec::new();
    for quota in applicable {
        let start = period_start(&quota.period, now);
        let reservation = QuotaReservation {
            reservation_id: format!(
                "qres_{}_{}_{}",
                request_id, quota.scope_type, quota.scope_id
            ),
            request_id: request_id.to_string(),
            tenant_id: principal.tenant_id.clone(),
            scope_type: quota.scope_type.clone(),
            scope_id: quota.scope_id.clone(),
            period_start: start,
            reserved_tokens: estimate.input_tokens + estimate.max_output_tokens,
            reserved_requests: 1,
            reserved_cost_micros: estimate.cost_micros,
            status: "reserved".to_string(),
            expires_at: (now + chrono::Duration::minutes(10)).to_rfc3339(),
            created_at: crate::util::now_rfc3339(),
        };
        if let Err(err) = store.add_quota_reservation(&reservation).await {
            for previous in added {
                let _ = store
                    .update_quota_reservation(&principal.tenant_id, &previous, "released")
                    .await;
            }
            return Err(internal(err));
        }
        added.push(reservation.request_id);
    }
    Ok(())
}

pub async fn commit(
    store: &Arc<dyn Store>,
    tenant_id: &str,
    request_id: &str,
    actual_tokens: i64,
    actual_cost_micros: i64,
) {
    let reservations = store
        .list_quota_reservations(tenant_id, request_id)
        .await
        .unwrap_or_default();
    let _ = store
        .update_quota_reservation(tenant_id, request_id, "committed")
        .await;
    for reservation in reservations {
        if reservation.status == "reserved" {
            let _ = store
                .adjust_quota_counter(
                    tenant_id,
                    &reservation.scope_type,
                    &reservation.scope_id,
                    &reservation.period_start,
                    actual_tokens - reservation.reserved_tokens,
                    actual_cost_micros - reservation.reserved_cost_micros,
                )
                .await;
        }
    }
}

pub async fn release(store: &Arc<dyn Store>, tenant_id: &str, request_id: &str) {
    let _ = store
        .update_quota_reservation(tenant_id, request_id, "released")
        .await;
}

pub fn estimate_tokens(text: &str) -> i64 {
    (text.chars().count() as i64 / 4).max(1)
}
