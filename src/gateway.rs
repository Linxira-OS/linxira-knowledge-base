// SPDX-License-Identifier: AGPL-3.0-only
//! Rust thin model gateway: public aliases, provider routes, failover,
//! usage events and pre-call token quota reservations.

use crate::config::Config;
use crate::error::AppError;
use crate::governance::Principal;
use crate::store::{
    ModelAliasRecord, ProviderModelRecord, ProviderRecord, RouteRecord, Store, UsageEventRecord,
};
use axum::http::StatusCode;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct GatewayRequest {
    pub request_id: String,
    pub principal: Principal,
    pub space_id: Option<String>,
    pub role: String,
    pub alias: String,
    pub messages: Vec<ChatMessage>,
    pub max_output_tokens: i64,
    pub temperature: f64,
    pub source: String,
    /// Standalone rewrite that produced this request, when the caller rewrote a
    /// follow-up question from conversation context.
    pub rewritten_query: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayUsage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_tokens: i64,
    pub token_source: String,
    pub cost_micros: i64,
    pub currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayResponse {
    pub model: String,
    pub content: String,
    pub usage: GatewayUsage,
    pub provider_cost_micros: i64,
    pub latency_ms: i64,
    pub usage_event_id: Option<String>,
    /// Non-empty when the request degraded, e.g. a stream request answered by
    /// a non-streaming upstream.
    pub degraded_codes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GatewayError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub retry_after: Option<u64>,
}

impl GatewayError {
    pub fn to_app_error(&self) -> AppError {
        let mut error = AppError::new(self.status, self.code, self.message.clone());
        if let Some(retry) = self.retry_after {
            error = error.with_retry_after(retry);
        }
        error
    }
}

fn unavailable(message: impl Into<String>) -> GatewayError {
    GatewayError {
        status: StatusCode::SERVICE_UNAVAILABLE,
        code: "model_unavailable",
        message: message.into(),
        retry_after: None,
    }
}

fn payment_required() -> GatewayError {
    GatewayError {
        status: StatusCode::PAYMENT_REQUIRED,
        code: "payment_required",
        message: "insufficient balance; top up or activate a plan".to_string(),
        retry_after: None,
    }
}

fn forbidden() -> GatewayError {
    GatewayError {
        status: StatusCode::FORBIDDEN,
        code: "model_forbidden",
        message: "model is forbidden for this principal".to_string(),
        retry_after: None,
    }
}

fn price_micros(json_text: &str) -> (i64, i64, String) {
    let value: Value = serde_json::from_str(json_text).unwrap_or(Value::Null);
    let input = value
        .get("input_per_million_micros")
        .or_else(|| value.get("input"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let output = value
        .get("output_per_million_micros")
        .or_else(|| value.get("output"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let currency = value
        .get("currency")
        .and_then(Value::as_str)
        .unwrap_or("USD")
        .to_string();
    (input, output, currency)
}

fn cost(tokens: i64, per_million: i64) -> i64 {
    if tokens <= 0 || per_million <= 0 {
        0
    } else {
        ((tokens as i128 * per_million as i128) / 1_000_000) as i64
    }
}

fn access_allowed(
    alias: &ModelAliasRecord,
    principal: &Principal,
    space: Option<&str>,
    role: &str,
) -> bool {
    let value: Value = serde_json::from_str(&alias.access_policy_json).unwrap_or(Value::Null);
    if value
        .as_object()
        .map(|object| object.is_empty())
        .unwrap_or(true)
    {
        return true;
    }
    if value
        .get("allow_all")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return true;
    }
    let contains = |key: &str, needle: &str| {
        value
            .get(key)
            .and_then(Value::as_array)
            .map(|items| items.iter().any(|item| item.as_str() == Some(needle)))
            .unwrap_or(false)
    };
    if contains("tenants", &principal.tenant_id) {
        return true;
    }
    if let Some(space) = space {
        if contains("spaces", space) {
            return true;
        }
    }
    contains("roles", role)
}

fn estimate_input(messages: &[ChatMessage]) -> i64 {
    let chars: usize = messages
        .iter()
        .map(|message| message.content.chars().count())
        .sum();
    (chars as i64 / 4).max(1)
}

fn capabilities_match(alias_json: &str, model_json: &str) -> bool {
    let alias: Vec<String> = serde_json::from_str(alias_json).unwrap_or_default();
    if alias.is_empty() {
        return true;
    }
    let model: Vec<String> = serde_json::from_str(model_json).unwrap_or_default();
    alias.iter().any(|capability| model.contains(capability))
}

fn weighted_pick(
    candidates: Vec<(RouteRecord, ProviderModelRecord, ProviderRecord)>,
) -> Vec<(RouteRecord, ProviderModelRecord, ProviderRecord)> {
    if candidates.len() <= 1 {
        return candidates;
    }
    let first_key = (candidates[0].0.priority, candidates[0].2.priority);
    let split = candidates
        .iter()
        .position(|(route, _, provider)| (route.priority, provider.priority) != first_key)
        .unwrap_or(candidates.len());
    let mut group = candidates[..split].to_vec();
    let rest = candidates[split..].to_vec();
    let total: u32 = group
        .iter()
        .map(|(route, _, _)| route.weight.max(1) as u32)
        .sum();
    let mut pick = if total == 0 {
        0
    } else {
        rand::thread_rng().gen_range(0..total)
    };
    let mut index = 0;
    for (position, (route, _, _)) in group.iter().enumerate() {
        let weight = route.weight.max(1) as u32;
        if pick < weight {
            index = position;
            break;
        }
        pick -= weight;
    }
    if index != 0 {
        group.swap(0, index);
    }
    group.extend(rest);
    group
}

pub struct Gateway {
    store: Arc<dyn Store>,
    client: reqwest::Client,
    local_ledger: bool,
}

impl Gateway {
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self {
            store,
            client: reqwest::Client::new(),
            local_ledger: true,
        }
    }

    /// External billing engines only emit usage; they never post local
    /// ledger entries, so the gateway must not debit the local wallet.
    pub fn with_local_ledger(mut self, local_ledger: bool) -> Self {
        self.local_ledger = local_ledger;
        self
    }

    async fn call_provider(
        &self,
        provider: &ProviderRecord,
        model: &ProviderModelRecord,
        request: &GatewayRequest,
    ) -> Result<(String, Value), (StatusCode, String)> {
        let base = provider.base_url.trim_end_matches('/');
        let url = format!("{base}/chat/completions");
        let timeout = serde_json::from_str::<Value>(&provider.timeouts_json)
            .ok()
            .and_then(|value| value.get("total_ms").and_then(Value::as_u64))
            .unwrap_or(30_000);
        let body = json!({
            "model": model.upstream_model,
            "messages": request.messages,
            "max_tokens": request.max_output_tokens,
            "temperature": request.temperature,
        });
        let mut builder = self
            .client
            .post(url)
            .timeout(Duration::from_millis(timeout.max(1)))
            .json(&body);
        if let Some(env_name) = &provider.api_key_env {
            if let Ok(key) = std::env::var(env_name) {
                if !key.trim().is_empty() {
                    builder = builder.bearer_auth(key);
                }
            }
        }
        let response = builder.send().await.map_err(|err| {
            (
                StatusCode::GATEWAY_TIMEOUT,
                format!("provider request failed: {err}"),
            )
        })?;
        let status = response.status();
        let value: Value = response.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            return Err((status, "provider returned an error".to_string()));
        }
        let content = value
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        Ok((content, value))
    }

    async fn call_provider_stream(
        &self,
        provider: &ProviderRecord,
        model: &ProviderModelRecord,
        request: &GatewayRequest,
        tx: &mpsc::Sender<String>,
    ) -> Result<(String, Value, bool), (StatusCode, String)> {
        let base = provider.base_url.trim_end_matches('/');
        let url = format!("{base}/chat/completions");
        let timeout = serde_json::from_str::<Value>(&provider.timeouts_json)
            .ok()
            .and_then(|value| value.get("total_ms").and_then(Value::as_u64))
            .unwrap_or(30_000);
        let body = json!({
            "model": model.upstream_model,
            "messages": request.messages,
            "max_tokens": request.max_output_tokens,
            "temperature": request.temperature,
            "stream": true,
        });
        let mut builder = self
            .client
            .post(url)
            .timeout(Duration::from_millis(timeout.max(1)))
            .json(&body);
        if let Some(env_name) = &provider.api_key_env {
            if let Ok(key) = std::env::var(env_name) {
                if !key.trim().is_empty() {
                    builder = builder.bearer_auth(key);
                }
            }
        }
        let mut response = builder.send().await.map_err(|err| {
            (
                StatusCode::GATEWAY_TIMEOUT,
                format!("provider request failed: {err}"),
            )
        })?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err((
                status,
                format!(
                    "provider returned an error: {}",
                    crate::util::truncate_chars(&text, 200)
                ),
            ));
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_string();
        let mut buffer = String::new();
        let mut content = String::new();
        let mut usage = Value::Null;
        let mut streamed = false;
        if content_type.contains("text/event-stream") {
            streamed = true;
            while let Some(chunk) = response.chunk().await.map_err(|err| {
                (
                    StatusCode::GATEWAY_TIMEOUT,
                    format!("provider stream failed: {err}"),
                )
            })? {
                buffer.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(index) = buffer.find('\n') {
                    let line = buffer[..index].trim().to_string();
                    buffer.drain(..=index);
                    let Some(data) = line.strip_prefix("data:") else {
                        continue;
                    };
                    let data = data.trim();
                    if data == "[DONE]" || data.is_empty() {
                        continue;
                    }
                    let Ok(value) = serde_json::from_str::<Value>(data) else {
                        continue;
                    };
                    if let Some(delta) = value
                        .pointer("/choices/0/delta/content")
                        .and_then(Value::as_str)
                    {
                        if !delta.is_empty() {
                            content.push_str(delta);
                            if tx.send(delta.to_string()).await.is_err() {
                                return Err((StatusCode::GONE, "client_disconnected".to_string()));
                            }
                        }
                    }
                    if value.get("usage").is_some() {
                        usage = value["usage"].clone();
                    }
                }
            }
        } else {
            let text = response.text().await.unwrap_or_default();
            let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            if value.get("choices").is_none() {
                return Err((
                    StatusCode::BAD_GATEWAY,
                    "provider returned an unsupported stream".to_string(),
                ));
            }
            content = value
                .pointer("/choices/0/message/content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            usage = value.get("usage").cloned().unwrap_or(Value::Null);
            if !content.is_empty() {
                let _ = tx.send(content.clone()).await;
            }
        }
        let mut value = json!({});
        if !usage.is_null() {
            value["usage"] = usage;
        }
        Ok((content, value, streamed))
    }

    /// Streams a chat completion. Token deltas are sent on `tx`; the caller
    /// maps `GatewayError { code: "client_disconnected" }` to cancellation.
    pub async fn chat_stream(
        &self,
        request: &GatewayRequest,
        tx: mpsc::Sender<String>,
    ) -> Result<GatewayResponse, GatewayError> {
        let started = std::time::Instant::now();
        let alias = self
            .store
            .get_model_alias(&request.principal.tenant_id, &request.alias)
            .await
            .map_err(|err| unavailable(err.to_string()))?
            .ok_or_else(|| unavailable("model alias not found"))?;
        if alias.status != "active" {
            return Err(unavailable("model alias is disabled"));
        }
        if !access_allowed(
            &alias,
            &request.principal,
            request.space_id.as_deref(),
            &request.role,
        ) {
            return Err(forbidden());
        }
        let routes = self
            .store
            .list_routes(&request.principal.tenant_id, Some(&alias.public_id))
            .await
            .map_err(|err| unavailable(err.to_string()))?;
        let models = self
            .store
            .list_provider_models(&request.principal.tenant_id)
            .await
            .map_err(|err| unavailable(err.to_string()))?;
        let providers = self
            .store
            .list_providers(&request.principal.tenant_id)
            .await
            .map_err(|err| unavailable(err.to_string()))?;
        let now = crate::util::now_rfc3339();
        let mut candidates: Vec<(RouteRecord, ProviderModelRecord, ProviderRecord)> = Vec::new();
        for route in routes {
            if (route.health == "open" || route.circuit_state == "open")
                && route
                    .circuit_open_until
                    .as_deref()
                    .map(|until| until > now.as_str())
                    .unwrap_or(false)
            {
                continue;
            }
            let Some(model) = models.iter().find(|model| {
                model.provider_model_id == route.provider_model_id && model.status == "active"
            }) else {
                continue;
            };
            if !capabilities_match(&alias.capabilities_json, &model.capabilities_json) {
                continue;
            }
            let Some(provider) = providers.iter().find(|provider| {
                provider.provider_id == model.provider_id && provider.status == "active"
            }) else {
                continue;
            };
            candidates.push((route, model.clone(), provider.clone()));
        }
        candidates.sort_by_key(|(route, _, provider)| (route.priority, provider.priority));
        candidates = weighted_pick(candidates);
        if candidates.is_empty() {
            return Err(unavailable("no healthy model route"));
        }
        let input_estimate = estimate_input(&request.messages);
        let (input_price, output_price, currency) = price_micros(&alias.price_json);
        let estimated_cost = cost(
            input_estimate + request.max_output_tokens,
            input_price.max(output_price),
        );
        let billing_wallets = self
            .store
            .billing_list_wallets(&request.principal.tenant_id)
            .await
            .unwrap_or_default();
        let billing_entitlement = self
            .store
            .billing_get_entitlement(&request.principal.tenant_id)
            .await
            .ok()
            .flatten();
        let billing_active = !billing_wallets.is_empty() || billing_entitlement.is_some();
        if billing_active {
            if let Some(entitlement) = &billing_entitlement {
                if matches!(entitlement.status.as_str(), "suspended" | "cancelled") {
                    return Err(payment_required());
                }
            }
            let included_remaining = billing_entitlement
                .as_ref()
                .map(|entitlement| (entitlement.included_tokens - entitlement.used_tokens).max(0))
                .unwrap_or(0);
            let wallet_balance = billing_wallets
                .first()
                .map(|wallet| wallet.balance_micros)
                .unwrap_or(0);
            if included_remaining <= 0 && wallet_balance < estimated_cost {
                return Err(payment_required());
            }
        }
        crate::token_quota::reserve(
            &self.store,
            &request.principal,
            request.space_id.as_deref(),
            &request.role,
            &request.request_id,
            &crate::token_quota::Estimate {
                input_tokens: input_estimate,
                max_output_tokens: request.max_output_tokens,
                cost_micros: estimated_cost,
                currency: currency.clone(),
            },
        )
        .await
        .map_err(|err| GatewayError {
            status: err.status,
            code: err.code,
            message: err.message,
            retry_after: err.retry_after,
        })?;

        let mut last_error = unavailable("all model routes failed");
        for (route, model, provider) in candidates {
            match self
                .call_provider_stream(&provider, &model, request, &tx)
                .await
            {
                Ok((content, value, streamed)) => {
                    let _ = self
                        .store
                        .update_route_health(
                            &request.principal.tenant_id,
                            &route.route_id,
                            "healthy",
                            "closed",
                            None,
                            None,
                        )
                        .await;
                    let prompt = value
                        .pointer("/usage/prompt_tokens")
                        .and_then(Value::as_i64);
                    let completion = value
                        .pointer("/usage/completion_tokens")
                        .and_then(Value::as_i64);
                    let cached = value
                        .pointer("/usage/prompt_tokens_details/cached_tokens")
                        .and_then(Value::as_i64)
                        .unwrap_or(0);
                    let (input_tokens, output_tokens, token_source) = match (prompt, completion) {
                        (Some(input), Some(output)) => (input, output, "provider"),
                        _ => (
                            input_estimate,
                            (content.chars().count() as i64 / 4).max(1),
                            "estimated",
                        ),
                    };
                    let (provider_input, provider_output, _) = price_micros(&model.pricing_json);
                    let usage = GatewayUsage {
                        input_tokens,
                        output_tokens,
                        cached_tokens: cached,
                        token_source: token_source.to_string(),
                        cost_micros: cost(input_tokens, input_price)
                            + cost(output_tokens, output_price),
                        currency: currency.clone(),
                    };
                    let provider_cost =
                        cost(input_tokens, provider_input) + cost(output_tokens, provider_output);
                    let usage_event_id = format!("ue_{}", crate::util::random_hex(12));
                    let _ = self
                        .store
                        .insert_usage_event(&UsageEventRecord {
                            usage_event_id: usage_event_id.clone(),
                            request_id: request.request_id.clone(),
                            tenant_id: request.principal.tenant_id.clone(),
                            space_id: request.space_id.clone(),
                            user_id: request.principal.principal_id.clone(),
                            role: request.role.clone(),
                            source: request.source.clone(),
                            rewritten_query: request.rewritten_query.clone(),
                            external_usage_id: None,
                            alias: alias.public_id.clone(),
                            provider_model_id: model.provider_model_id.clone(),
                            input_tokens,
                            output_tokens,
                            cached_tokens: cached,
                            token_source: token_source.to_string(),
                            cost_micros: usage.cost_micros,
                            currency: currency.clone(),
                            provider_cost_micros: provider_cost,
                            latency_ms: started.elapsed().as_millis() as i64,
                            status: "success".to_string(),
                            cache_hit: false,
                            at: crate::util::now_rfc3339(),
                        })
                        .await;
                    if billing_active && self.local_ledger {
                        let billing = crate::billing::Billing::new(
                            self.store.clone(),
                            crate::config::BillingConfig::default(),
                        );
                        let billed_tokens = usage.input_tokens + usage.output_tokens;
                        if let Err(err) = billing
                            .charge_usage(
                                &request.principal.tenant_id,
                                usage.cost_micros,
                                billed_tokens,
                                &usage_event_id,
                            )
                            .await
                        {
                            tracing::warn!(error = %err, "usage charge failed");
                        }
                    }
                    crate::token_quota::commit(
                        &self.store,
                        &request.principal.tenant_id,
                        &request.request_id,
                        input_tokens + output_tokens,
                        usage.cost_micros,
                    )
                    .await;
                    return Ok(GatewayResponse {
                        model: alias.public_id,
                        content,
                        usage,
                        provider_cost_micros: provider_cost,
                        latency_ms: started.elapsed().as_millis() as i64,
                        usage_event_id: Some(usage_event_id),
                        degraded_codes: if streamed {
                            Vec::new()
                        } else {
                            vec!["stream_fallback".to_string()]
                        },
                    });
                }
                Err((status, message)) => {
                    if message == "client_disconnected" {
                        crate::token_quota::release(
                            &self.store,
                            &request.principal.tenant_id,
                            &request.request_id,
                        )
                        .await;
                        let _ = self
                            .store
                            .insert_usage_event(&UsageEventRecord {
                                usage_event_id: format!("ue_{}", crate::util::random_hex(12)),
                                request_id: request.request_id.clone(),
                                tenant_id: request.principal.tenant_id.clone(),
                                space_id: request.space_id.clone(),
                                user_id: request.principal.principal_id.clone(),
                                role: request.role.clone(),
                                source: request.source.clone(),
                                rewritten_query: request.rewritten_query.clone(),
                                external_usage_id: None,
                                alias: alias.public_id.clone(),
                                provider_model_id: model.provider_model_id.clone(),
                                input_tokens: 0,
                                output_tokens: 0,
                                cached_tokens: 0,
                                token_source: "estimated".to_string(),
                                cost_micros: 0,
                                currency: currency.clone(),
                                provider_cost_micros: 0,
                                latency_ms: started.elapsed().as_millis() as i64,
                                status: "cancelled".to_string(),
                                cache_hit: false,
                                at: crate::util::now_rfc3339(),
                            })
                            .await;
                        return Err(GatewayError {
                            status: StatusCode::GONE,
                            code: "client_disconnected",
                            message,
                            retry_after: None,
                        });
                    }
                    let retryable = status == StatusCode::TOO_MANY_REQUESTS
                        || status.is_server_error()
                        || status == StatusCode::GATEWAY_TIMEOUT;
                    let open_until =
                        (chrono::Utc::now() + chrono::Duration::seconds(30)).to_rfc3339();
                    let _ = self
                        .store
                        .update_route_health(
                            &request.principal.tenant_id,
                            &route.route_id,
                            if retryable { "open" } else { "degraded" },
                            if retryable { "open" } else { "closed" },
                            Some(&now),
                            if retryable {
                                Some(open_until.as_str())
                            } else {
                                None
                            },
                        )
                        .await;
                    if retryable {
                        last_error = GatewayError {
                            status: if status == StatusCode::TOO_MANY_REQUESTS {
                                StatusCode::TOO_MANY_REQUESTS
                            } else {
                                StatusCode::SERVICE_UNAVAILABLE
                            },
                            code: if status == StatusCode::TOO_MANY_REQUESTS {
                                "model_rate_limited"
                            } else {
                                "model_timeout"
                            },
                            message,
                            retry_after: Some(30),
                        };
                        continue;
                    }
                    crate::token_quota::release(
                        &self.store,
                        &request.principal.tenant_id,
                        &request.request_id,
                    )
                    .await;
                    return Err(GatewayError {
                        status: StatusCode::BAD_REQUEST,
                        code: "model_invalid_request",
                        message,
                        retry_after: None,
                    });
                }
            }
        }
        crate::token_quota::release(
            &self.store,
            &request.principal.tenant_id,
            &request.request_id,
        )
        .await;
        Err(last_error)
    }

    pub async fn chat(&self, request: &GatewayRequest) -> Result<GatewayResponse, GatewayError> {
        let started = std::time::Instant::now();
        let alias = self
            .store
            .get_model_alias(&request.principal.tenant_id, &request.alias)
            .await
            .map_err(|err| unavailable(err.to_string()))?
            .ok_or_else(|| unavailable("model alias not found"))?;
        if alias.status != "active" {
            return Err(unavailable("model alias is disabled"));
        }
        if !access_allowed(
            &alias,
            &request.principal,
            request.space_id.as_deref(),
            &request.role,
        ) {
            return Err(forbidden());
        }

        let routes = self
            .store
            .list_routes(&request.principal.tenant_id, Some(&alias.public_id))
            .await
            .map_err(|err| unavailable(err.to_string()))?;
        let models = self
            .store
            .list_provider_models(&request.principal.tenant_id)
            .await
            .map_err(|err| unavailable(err.to_string()))?;
        let providers = self
            .store
            .list_providers(&request.principal.tenant_id)
            .await
            .map_err(|err| unavailable(err.to_string()))?;
        let now = crate::util::now_rfc3339();
        let mut candidates: Vec<(RouteRecord, ProviderModelRecord, ProviderRecord)> = Vec::new();
        for route in routes {
            if (route.health == "open" || route.circuit_state == "open")
                && route
                    .circuit_open_until
                    .as_deref()
                    .map(|until| until > now.as_str())
                    .unwrap_or(false)
            {
                continue;
            }
            let Some(model) = models.iter().find(|model| {
                model.provider_model_id == route.provider_model_id && model.status == "active"
            }) else {
                continue;
            };
            if !capabilities_match(&alias.capabilities_json, &model.capabilities_json) {
                continue;
            }
            let Some(provider) = providers.iter().find(|provider| {
                provider.provider_id == model.provider_id && provider.status == "active"
            }) else {
                continue;
            };
            candidates.push((route, model.clone(), provider.clone()));
        }
        candidates.sort_by_key(|(route, _, provider)| (route.priority, provider.priority));
        candidates = weighted_pick(candidates);
        if candidates.is_empty() {
            return Err(unavailable("no healthy model route"));
        }

        let input_estimate = estimate_input(&request.messages);
        let (input_price, output_price, currency) = price_micros(&alias.price_json);
        let estimated_cost = cost(
            input_estimate + request.max_output_tokens,
            input_price.max(output_price),
        );
        let billing_wallets = self
            .store
            .billing_list_wallets(&request.principal.tenant_id)
            .await
            .unwrap_or_default();
        let billing_entitlement = self
            .store
            .billing_get_entitlement(&request.principal.tenant_id)
            .await
            .ok()
            .flatten();
        let billing_active = !billing_wallets.is_empty() || billing_entitlement.is_some();
        if billing_active {
            if let Some(entitlement) = &billing_entitlement {
                if matches!(entitlement.status.as_str(), "suspended" | "cancelled") {
                    return Err(payment_required());
                }
            }
            let included_remaining = billing_entitlement
                .as_ref()
                .map(|entitlement| (entitlement.included_tokens - entitlement.used_tokens).max(0))
                .unwrap_or(0);
            let wallet_balance = billing_wallets
                .first()
                .map(|wallet| wallet.balance_micros)
                .unwrap_or(0);
            if included_remaining <= 0 && wallet_balance < estimated_cost {
                return Err(payment_required());
            }
        }
        crate::token_quota::reserve(
            &self.store,
            &request.principal,
            request.space_id.as_deref(),
            &request.role,
            &request.request_id,
            &crate::token_quota::Estimate {
                input_tokens: input_estimate,
                max_output_tokens: request.max_output_tokens,
                cost_micros: estimated_cost,
                currency: currency.clone(),
            },
        )
        .await
        .map_err(|err| GatewayError {
            status: err.status,
            code: err.code,
            message: err.message,
            retry_after: err.retry_after,
        })?;

        let mut last_error = unavailable("all model routes failed");
        for (route, model, provider) in candidates {
            let (content, value) = match self.call_provider(&provider, &model, request).await {
                Ok(result) => result,
                Err((status, message)) => {
                    let retryable = status == StatusCode::TOO_MANY_REQUESTS
                        || status.is_server_error()
                        || status == StatusCode::GATEWAY_TIMEOUT;
                    let open_until =
                        (chrono::Utc::now() + chrono::Duration::seconds(30)).to_rfc3339();
                    let _ = self
                        .store
                        .update_route_health(
                            &request.principal.tenant_id,
                            &route.route_id,
                            if retryable { "open" } else { "degraded" },
                            if retryable { "open" } else { "closed" },
                            Some(&now),
                            if retryable {
                                Some(open_until.as_str())
                            } else {
                                None
                            },
                        )
                        .await;
                    if retryable {
                        last_error = GatewayError {
                            status: if status == StatusCode::TOO_MANY_REQUESTS {
                                StatusCode::TOO_MANY_REQUESTS
                            } else {
                                StatusCode::SERVICE_UNAVAILABLE
                            },
                            code: if status == StatusCode::TOO_MANY_REQUESTS {
                                "model_rate_limited"
                            } else {
                                "model_timeout"
                            },
                            message,
                            retry_after: Some(30),
                        };
                        continue;
                    }
                    crate::token_quota::release(
                        &self.store,
                        &request.principal.tenant_id,
                        &request.request_id,
                    )
                    .await;
                    return Err(
                        if status == StatusCode::FORBIDDEN || status == StatusCode::UNAUTHORIZED {
                            forbidden()
                        } else {
                            GatewayError {
                                status: StatusCode::BAD_REQUEST,
                                code: "model_invalid_request",
                                message,
                                retry_after: None,
                            }
                        },
                    );
                }
            };
            let _ = self
                .store
                .update_route_health(
                    &request.principal.tenant_id,
                    &route.route_id,
                    "healthy",
                    "closed",
                    None,
                    None,
                )
                .await;

            let prompt = value
                .pointer("/usage/prompt_tokens")
                .and_then(Value::as_i64);
            let completion = value
                .pointer("/usage/completion_tokens")
                .and_then(Value::as_i64);
            let cached = value
                .pointer("/usage/prompt_tokens_details/cached_tokens")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let (input_tokens, output_tokens, token_source) = match (prompt, completion) {
                (Some(input), Some(output)) => (input, output, "provider"),
                _ => (
                    input_estimate,
                    (content.chars().count() as i64 / 4).max(1),
                    "estimated",
                ),
            };
            let (provider_input, provider_output, _) = price_micros(&model.pricing_json);
            let usage = GatewayUsage {
                input_tokens,
                output_tokens,
                cached_tokens: cached,
                token_source: token_source.to_string(),
                cost_micros: cost(input_tokens, input_price) + cost(output_tokens, output_price),
                currency: currency.clone(),
            };
            let provider_cost =
                cost(input_tokens, provider_input) + cost(output_tokens, provider_output);
            let usage_event_id = format!("ue_{}", crate::util::random_hex(12));
            let _ = self
                .store
                .insert_usage_event(&UsageEventRecord {
                    usage_event_id: usage_event_id.clone(),
                    request_id: request.request_id.clone(),
                    tenant_id: request.principal.tenant_id.clone(),
                    space_id: request.space_id.clone(),
                    user_id: request.principal.principal_id.clone(),
                    role: request.role.clone(),
                    source: request.source.clone(),
                    rewritten_query: request.rewritten_query.clone(),
                    external_usage_id: None,
                    alias: alias.public_id.clone(),
                    provider_model_id: model.provider_model_id.clone(),
                    input_tokens,
                    output_tokens,
                    cached_tokens: cached,
                    token_source: token_source.to_string(),
                    cost_micros: usage.cost_micros,
                    currency: currency.clone(),
                    provider_cost_micros: provider_cost,
                    latency_ms: started.elapsed().as_millis() as i64,
                    status: "success".to_string(),
                    cache_hit: false,
                    at: crate::util::now_rfc3339(),
                })
                .await;
            if billing_active && self.local_ledger {
                let billing = crate::billing::Billing::new(
                    self.store.clone(),
                    crate::config::BillingConfig::default(),
                );
                let billed_tokens = usage.input_tokens + usage.output_tokens;
                if let Err(err) = billing
                    .charge_usage(
                        &request.principal.tenant_id,
                        usage.cost_micros,
                        billed_tokens,
                        &usage_event_id,
                    )
                    .await
                {
                    tracing::warn!(error = %err, "usage charge failed");
                }
            }
            crate::token_quota::commit(
                &self.store,
                &request.principal.tenant_id,
                &request.request_id,
                input_tokens + output_tokens,
                usage.cost_micros,
            )
            .await;
            return Ok(GatewayResponse {
                model: alias.public_id,
                content,
                usage,
                provider_cost_micros: provider_cost,
                latency_ms: started.elapsed().as_millis() as i64,
                usage_event_id: Some(usage_event_id),
                degraded_codes: Vec::new(),
            });
        }
        crate::token_quota::release(
            &self.store,
            &request.principal.tenant_id,
            &request.request_id,
        )
        .await;
        let _ = self
            .store
            .insert_usage_event(&UsageEventRecord {
                usage_event_id: format!("ue_{}", crate::util::random_hex(12)),
                request_id: request.request_id.clone(),
                tenant_id: request.principal.tenant_id.clone(),
                space_id: request.space_id.clone(),
                user_id: request.principal.principal_id.clone(),
                role: request.role.clone(),
                source: "kb".to_string(),
                external_usage_id: None,
                alias: alias.public_id.clone(),
                provider_model_id: "unavailable".to_string(),
                input_tokens: 0,
                output_tokens: 0,
                cached_tokens: 0,
                token_source: "estimated".to_string(),
                cost_micros: 0,
                currency,
                provider_cost_micros: 0,
                latency_ms: started.elapsed().as_millis() as i64,
                status: "error".to_string(),
                cache_hit: false,
                rewritten_query: request.rewritten_query.clone(),
                at: crate::util::now_rfc3339(),
            })
            .await;
        Err(last_error)
    }
}

/// Materialize a legacy `[models.llm]` into the alias/provider/route control
/// plane without changing the existing answer path.
pub async fn materialize_legacy(store: &Arc<dyn Store>, config: &Config) -> anyhow::Result<()> {
    if !config.models.llm.enabled() {
        return Ok(());
    }
    let tenant = &config.storage.tenant_id;
    if store.get_model_alias(tenant, "default").await?.is_some() {
        return Ok(());
    }
    let Some(base_url) = config.models.llm.base_url_or_default() else {
        return Ok(());
    };
    let Some(upstream) = config.models.llm.model.clone() else {
        return Ok(());
    };
    let now = crate::util::now_rfc3339();
    let provider = ProviderRecord {
        provider_id: "legacy".to_string(),
        kind: "openai_compatible".to_string(),
        base_url,
        api_key_env: config.models.llm.api_key_env.clone(),
        status: "active".to_string(),
        priority: 100,
        weight: 1,
        limits_json: "{}".to_string(),
        timeouts_json: json!({"total_ms": config.models.llm.timeout_ms}).to_string(),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let model = ProviderModelRecord {
        provider_model_id: "legacy:default".to_string(),
        provider_id: "legacy".to_string(),
        upstream_model: upstream,
        context_window: 0,
        pricing_json: "{}".to_string(),
        capabilities_json: "[\"chat\"]".to_string(),
        status: "active".to_string(),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let alias = ModelAliasRecord {
        public_id: "default".to_string(),
        display_name: "Default chat model".to_string(),
        capabilities_json: "[\"chat\"]".to_string(),
        status: "active".to_string(),
        access_policy_json: "{\"allow_all\":true}".to_string(),
        price_json: "{}".to_string(),
        version: 1,
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let route = RouteRecord {
        route_id: "route_legacy_default".to_string(),
        alias_public_id: "default".to_string(),
        provider_model_id: "legacy:default".to_string(),
        weight: 1,
        priority: 100,
        health: "healthy".to_string(),
        circuit_state: "closed".to_string(),
        last_error_at: None,
        circuit_open_until: None,
        created_at: now.clone(),
        updated_at: now,
    };
    store.upsert_provider(tenant, &provider).await?;
    store.upsert_provider_model(tenant, &model).await?;
    store.upsert_model_alias(tenant, &alias).await?;
    store.upsert_route(tenant, &route).await?;
    Ok(())
}
