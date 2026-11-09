// SPDX-License-Identifier: MIT OR Apache-2.0
//! In-process rate limiting and backpressure.
//!
//! P0 keeps counters in memory (single process). Team/enterprise multi-instance
//! consistency needs shared counters (PG/Redis) and is explicitly documented as
//! a later item; this module never claims cross-instance strictness.

use crate::config::LimitsConfig;
use crate::error::AppError;
use crate::governance::Principal;
use crate::store::Store;
use axum::http::{HeaderMap, StatusCode};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

struct Bucket {
    tokens: f64,
    last: Instant,
}

#[derive(Debug, Default)]
struct Counters {
    inflight: HashMap<String, u32>,
    daily: HashMap<String, (String, u64)>,
}

#[derive(Clone)]
pub struct Limits {
    config: LimitsConfig,
    buckets: Arc<Mutex<HashMap<String, Bucket>>>,
    counters: Arc<Mutex<Counters>>,
}

pub struct LimitGuard {
    limits: Limits,
    keys: Vec<String>,
}

impl Drop for LimitGuard {
    fn drop(&mut self) {
        let mut counters = self.limits.counters.lock().expect("limits poisoned");
        for key in &self.keys {
            if let Some(value) = counters.inflight.get_mut(key) {
                *value = value.saturating_sub(1);
            }
        }
    }
}

impl Limits {
    pub fn new(config: LimitsConfig) -> Self {
        Self {
            config,
            buckets: Arc::new(Mutex::new(HashMap::new())),
            counters: Arc::new(Mutex::new(Counters::default())),
        }
    }

    pub fn config(&self) -> &LimitsConfig {
        &self.config
    }

    pub fn check_query(&self, query: &str, top_k: usize) -> Result<(), AppError> {
        if query.chars().count() > self.config.max_query_chars {
            return Err(AppError::invalid_query("query too long"));
        }
        if top_k > self.config.max_top_k {
            return Err(AppError::invalid_top_k("top_k too large"));
        }
        Ok(())
    }

    fn check_qps(&self, key: &str, qps: u32) -> Result<(), u64> {
        if qps == 0 {
            return Ok(());
        }
        let mut buckets = self.buckets.lock().expect("limits poisoned");
        let now = Instant::now();
        let bucket = buckets.entry(key.to_string()).or_insert(Bucket {
            tokens: qps as f64,
            last: now,
        });
        let elapsed = now.duration_since(bucket.last).as_secs_f64();
        let burst = (qps as f64 * 2.0).max(1.0);
        bucket.tokens = (bucket.tokens + elapsed * qps as f64).min(burst);
        bucket.last = now;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            Ok(())
        } else {
            Err(self.config.retry_after_seconds.max(1))
        }
    }

    fn check_daily(&self, key: &str, limit: u32) -> Result<(), u64> {
        if limit == 0 {
            return Ok(());
        }
        let day = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let mut counters = self.counters.lock().expect("limits poisoned");
        let entry = counters
            .daily
            .entry(key.to_string())
            .or_insert((day.clone(), 0));
        if entry.0 != day {
            *entry = (day, 0);
        }
        if entry.1 >= limit as u64 {
            return Err(86_400);
        }
        entry.1 += 1;
        Ok(())
    }

    fn acquire(&self, key: &str, max: u32) -> Result<(), ()> {
        if max == 0 {
            return Ok(());
        }
        let mut counters = self.counters.lock().expect("limits poisoned");
        let value = counters.inflight.entry(key.to_string()).or_insert(0);
        if *value >= max {
            return Err(());
        }
        *value += 1;
        Ok(())
    }

    fn guard(&self, keys: Vec<String>) -> LimitGuard {
        LimitGuard {
            limits: self.clone(),
            keys,
        }
    }

    pub fn check_ip(&self, ip_hash: &str) -> Result<(), AppError> {
        self.check_qps(&format!("ip:{ip_hash}"), self.config.qps_ip)
            .map_err(rate_error)
    }

    pub fn enforce(
        &self,
        principal: &Principal,
        space: Option<&str>,
    ) -> Result<LimitGuard, AppError> {
        self.check_qps("instance", self.config.qps_instance)
            .map_err(rate_error)?;
        let user_key = format!("user:{}", principal.principal_id);
        let tenant_key = format!("tenant:{}", principal.tenant_id);
        self.check_qps(&user_key, self.config.qps_user)
            .map_err(rate_error)?;
        self.check_qps(&tenant_key, self.config.qps_tenant)
            .map_err(rate_error)?;
        if let Some(token_id) = &principal.token_id {
            let token_key = format!("token:{token_id}");
            self.check_qps(&token_key, self.config.qps_token)
                .map_err(rate_error)?;
            self.check_daily(
                &format!("daily:token:{token_id}"),
                self.config.daily_requests_token,
            )
            .map_err(rate_error)?;
        }
        if let Some(space) = space {
            self.check_qps(&format!("space:{space}"), self.config.qps_space)
                .map_err(rate_error)?;
        }

        let mut acquired = Vec::new();
        for (key, max) in [
            ("instance", self.config.max_concurrency_instance),
            (user_key.as_str(), self.config.max_concurrency_user),
            (tenant_key.as_str(), self.config.max_concurrency_tenant),
        ] {
            if self.acquire(key, max).is_err() {
                let _ = self.guard(acquired);
                return Err(queue_error(self.config.retry_after_seconds));
            }
            acquired.push(key.to_string());
        }
        if let Some(space) = space {
            if self
                .acquire(&format!("space:{space}"), self.config.max_concurrency_space)
                .is_err()
            {
                let _ = self.guard(acquired);
                return Err(queue_error(self.config.retry_after_seconds));
            }
            acquired.push(format!("space:{space}"));
        }
        Ok(self.guard(acquired))
    }
}

fn rate_error(retry: u64) -> AppError {
    crate::metrics::rate_limited();
    AppError::new(
        StatusCode::TOO_MANY_REQUESTS,
        "rate_limited",
        "rate limit exceeded",
    )
    .with_retry_after(retry.max(1))
}

fn queue_error(retry: u64) -> AppError {
    crate::metrics::queue_full();
    AppError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "queue_full",
        "server queue is full",
    )
    .with_retry_after(retry.max(1))
}

/// Enforce and audit one authenticated request. The returned guard must live
/// until the handler finishes so concurrency accounting is accurate.
pub async fn enforce(
    store: &Arc<dyn Store>,
    limits: &Limits,
    principal: &Principal,
    space: Option<&str>,
    action: &str,
    _headers: &HeaderMap,
) -> Result<LimitGuard, AppError> {
    match limits.enforce(principal, space) {
        Ok(guard) => Ok(guard),
        Err(err) => {
            crate::audit::record(
                store,
                &principal.tenant_id,
                space,
                &principal.principal_id,
                "user",
                action,
                "rate_limit",
                space.unwrap_or("instance"),
                "denied",
                err.status().as_u16(),
                None,
                None,
                None,
                serde_json::json!({"error": err.code()}),
            )
            .await;
            Err(err)
        }
    }
}
