// SPDX-License-Identifier: MIT OR Apache-2.0
//! Payment provider boundary.
//!
//! This module deliberately contains no gateway implementation. Stripe,
//! PayPal, WeChat Pay, Alipay and billing engines such as Lago/Stripe Billing
//! are future adapters; the kernel only owns checkout/webhook/refund/sync
//! interfaces, feature gates and usage/audit hand-off.

use crate::config::{BillingConfig, PaymentConfig};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutRequest {
    pub tenant_id: String,
    pub space_id: Option<String>,
    pub plan: String,
    pub amount_minor: i64,
    pub currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutSession {
    pub provider: String,
    pub session_id: String,
    pub checkout_url: Option<String>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookRequest {
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentEvent {
    pub provider: String,
    pub event_id: String,
    pub event_type: String,
    pub tenant_id: Option<String>,
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
    /// Provider object id (checkout session / payment intent) used to match
    /// the pending topup created at checkout time.
    #[serde(default)]
    pub provider_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefundRequest {
    pub tenant_id: String,
    pub payment_id: String,
    pub amount_minor: Option<i64>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionSyncRequest {
    pub tenant_id: String,
    pub subscription_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionSnapshot {
    pub tenant_id: String,
    pub plan: String,
    pub status: String,
    pub current_period_end: Option<String>,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum PaymentError {
    #[error("payment provider is not configured")]
    NotConfigured,
    #[error("payment provider adapter is unavailable")]
    Unavailable,
    #[error("payment provider rejected the request: {0}")]
    Invalid(String),
}

impl PaymentError {
    pub fn code(&self) -> &'static str {
        match self {
            PaymentError::NotConfigured => "payment_not_configured",
            PaymentError::Unavailable => "payment_provider_unavailable",
            PaymentError::Invalid(_) => "payment_invalid",
        }
    }
}

#[async_trait]
pub trait PaymentProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn currency(&self) -> &str;
    fn mode(&self) -> &str;
    async fn create_checkout(
        &self,
        request: &CheckoutRequest,
    ) -> Result<CheckoutSession, PaymentError>;
    async fn verify_webhook(&self, request: &WebhookRequest) -> Result<PaymentEvent, PaymentError>;
    async fn refund(&self, request: &RefundRequest) -> Result<PaymentEvent, PaymentError>;
    async fn subscription_sync(
        &self,
        request: &SubscriptionSyncRequest,
    ) -> Result<SubscriptionSnapshot, PaymentError>;
}

/// Resolve a provider configuration into an adapter. WeChat/Alipay keep the
/// adapter boundary but report NotConfigured until certificates are wired.
pub fn provider_from_config(
    config: &PaymentConfig,
) -> Result<Box<dyn PaymentProvider>, PaymentError> {
    provider_for_billing(config, &BillingConfig::default())
}

pub fn provider_for_billing(
    config: &PaymentConfig,
    billing: &BillingConfig,
) -> Result<Box<dyn PaymentProvider>, PaymentError> {
    match config.provider.trim().to_ascii_lowercase().as_str() {
        "" | "none" => Err(PaymentError::NotConfigured),
        "stripe" => Ok(Box::new(StripeProvider::from_env(billing))),
        "manual" => Ok(Box::new(ManualProvider { name: "manual" })),
        "mock" => Ok(Box::new(ManualProvider { name: "mock" })),
        "wechat" | "wechat_pay" | "alipay" => Err(PaymentError::NotConfigured),
        _ => Err(PaymentError::Unavailable),
    }
}

/// Stripe Checkout + webhook signature verification. Card data never reaches
/// this kernel: only session ids and webhook events are handled.
pub struct StripeProvider {
    secret: Option<String>,
    webhook_secret: Option<String>,
    tolerance_seconds: u64,
    client: reqwest::Client,
}

impl StripeProvider {
    pub fn from_env(billing: &BillingConfig) -> Self {
        let secret = billing
            .stripe_secret_env
            .as_ref()
            .and_then(|name| std::env::var(name).ok())
            .filter(|value| !value.trim().is_empty());
        let webhook_secret = billing
            .stripe_webhook_secret_env
            .as_ref()
            .and_then(|name| std::env::var(name).ok())
            .filter(|value| !value.trim().is_empty());
        Self {
            secret,
            webhook_secret,
            tolerance_seconds: billing.webhook_tolerance_seconds.max(1),
            client: reqwest::Client::new(),
        }
    }
}

/// Test/ops helper: compute a Stripe-style `t=...,v1=...` signature header.
pub fn sign_stripe_webhook(secret: &str, body: &[u8], timestamp: i64) -> String {
    let mut signed = timestamp.to_string().into_bytes();
    signed.push(b'.');
    signed.extend_from_slice(body);
    format!(
        "t={timestamp},v1={}",
        hmac_sha256_hex(secret.as_bytes(), &signed)
    )
}

fn hmac_sha256_hex(secret: &[u8], payload: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("hmac key");
    mac.update(payload);
    hex::encode(mac.finalize().into_bytes())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[async_trait]
impl PaymentProvider for StripeProvider {
    fn name(&self) -> &'static str {
        "stripe"
    }

    fn currency(&self) -> &str {
        "USD"
    }

    fn mode(&self) -> &str {
        if self.secret.is_some() {
            "live"
        } else {
            "mock"
        }
    }

    async fn create_checkout(
        &self,
        request: &CheckoutRequest,
    ) -> Result<CheckoutSession, PaymentError> {
        let session_id = format!("cs_{}", crate::util::random_hex(12));
        if let Some(secret) = &self.secret {
            let response = self
                .client
                .post("https://api.stripe.com/v1/checkout/sessions")
                .bearer_auth(secret)
                .form(&[
                    ("mode", "payment".to_string()),
                    (
                        "success_url",
                        "https://example.invalid/billing/success".to_string(),
                    ),
                    (
                        "cancel_url",
                        "https://example.invalid/billing/cancel".to_string(),
                    ),
                    ("client_reference_id", request.tenant_id.clone()),
                    ("line_items[0][quantity]", "1".to_string()),
                    (
                        "line_items[0][price_data][currency]",
                        request.currency.to_lowercase(),
                    ),
                    (
                        "line_items[0][price_data][unit_amount]",
                        request.amount_minor.to_string(),
                    ),
                    (
                        "line_items[0][price_data][product_data][name]",
                        request.plan.clone(),
                    ),
                    ("metadata[tenant_id]", request.tenant_id.clone()),
                    ("metadata[plan]", request.plan.clone()),
                ])
                .send()
                .await
                .map_err(|err| PaymentError::Invalid(format!("stripe request failed: {err}")))?;
            let status = response.status();
            let value: serde_json::Value = response.json().await.unwrap_or(serde_json::Value::Null);
            if !status.is_success() {
                return Err(PaymentError::Invalid(format!("stripe returned {status}")));
            }
            let id = value
                .get("id")
                .and_then(|item| item.as_str())
                .unwrap_or(&session_id)
                .to_string();
            let url = value
                .get("url")
                .and_then(|item| item.as_str())
                .map(str::to_string);
            return Ok(CheckoutSession {
                provider: "stripe".to_string(),
                session_id: id,
                checkout_url: url,
                expires_at: None,
            });
        }
        Ok(CheckoutSession {
            provider: "stripe".to_string(),
            session_id: session_id.clone(),
            checkout_url: Some(format!("lkb-mock://checkout/{session_id}")),
            expires_at: None,
        })
    }

    async fn verify_webhook(&self, request: &WebhookRequest) -> Result<PaymentEvent, PaymentError> {
        let secret = self
            .webhook_secret
            .as_ref()
            .ok_or_else(|| PaymentError::NotConfigured)?;
        let signature = request
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("stripe-signature"))
            .map(|(_, value)| value.clone())
            .ok_or_else(|| PaymentError::Invalid("missing Stripe-Signature header".to_string()))?;
        let mut timestamp = None;
        let mut expected = None;
        for part in signature.split(',') {
            let Some((key, value)) = part.split_once('=') else {
                continue;
            };
            match key.trim() {
                "t" => timestamp = value.trim().parse::<i64>().ok(),
                "v1" => expected = Some(value.trim().to_string()),
                _ => {}
            }
        }
        let timestamp = timestamp
            .ok_or_else(|| PaymentError::Invalid("missing signature timestamp".to_string()))?;
        let expected =
            expected.ok_or_else(|| PaymentError::Invalid("missing v1 signature".to_string()))?;
        let now = chrono::Utc::now().timestamp();
        if (now - timestamp).abs() > self.tolerance_seconds as i64 {
            return Err(PaymentError::Invalid(
                "signature timestamp outside tolerance".to_string(),
            ));
        }
        let mut signed = timestamp.to_string().into_bytes();
        signed.push(b'.');
        signed.extend_from_slice(&request.body);
        let computed = hmac_sha256_hex(secret.as_bytes(), &signed);
        if !constant_time_eq(computed.as_bytes(), expected.as_bytes()) {
            return Err(PaymentError::Invalid("signature mismatch".to_string()));
        }
        let value: serde_json::Value = serde_json::from_slice(&request.body)
            .map_err(|err| PaymentError::Invalid(format!("invalid webhook json: {err}")))?;
        let event_id = value
            .get("id")
            .and_then(|item| item.as_str())
            .ok_or_else(|| PaymentError::Invalid("missing event id".to_string()))?;
        let event_type = value
            .get("type")
            .and_then(|item| item.as_str())
            .unwrap_or("unknown");
        let object = value
            .get("data")
            .and_then(|item| item.get("object"))
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let tenant_id = object
            .get("metadata")
            .and_then(|item| item.get("tenant_id"))
            .and_then(|item| item.as_str())
            .or_else(|| {
                object
                    .get("client_reference_id")
                    .and_then(|item| item.as_str())
            })
            .map(str::to_string);
        let amount_minor = object.get("amount_total").and_then(|item| item.as_i64());
        let currency = object
            .get("currency")
            .and_then(|item| item.as_str())
            .map(str::to_string);
        Ok(PaymentEvent {
            provider: "stripe".to_string(),
            event_id: event_id.to_string(),
            event_type: event_type.to_string(),
            tenant_id,
            amount_minor,
            currency,
            provider_ref: object
                .get("id")
                .and_then(|item| item.as_str())
                .map(str::to_string),
        })
    }

    async fn refund(&self, _request: &RefundRequest) -> Result<PaymentEvent, PaymentError> {
        Err(PaymentError::Unavailable)
    }

    async fn subscription_sync(
        &self,
        _request: &SubscriptionSyncRequest,
    ) -> Result<SubscriptionSnapshot, PaymentError> {
        Err(PaymentError::Unavailable)
    }
}

/// Manual/mock provider used for tests and self-hosted offline topups.
pub struct ManualProvider {
    name: &'static str,
}

#[async_trait]
impl PaymentProvider for ManualProvider {
    fn name(&self) -> &'static str {
        self.name
    }

    fn currency(&self) -> &str {
        "USD"
    }

    fn mode(&self) -> &str {
        "test"
    }

    async fn create_checkout(
        &self,
        _request: &CheckoutRequest,
    ) -> Result<CheckoutSession, PaymentError> {
        let session_id = format!("{}_{}", self.name, crate::util::random_hex(12));
        Ok(CheckoutSession {
            provider: self.name.to_string(),
            session_id: session_id.clone(),
            checkout_url: Some(format!("lkb-mock://checkout/{session_id}")),
            expires_at: None,
        })
    }

    async fn verify_webhook(&self, request: &WebhookRequest) -> Result<PaymentEvent, PaymentError> {
        let value: serde_json::Value = serde_json::from_slice(&request.body)
            .map_err(|err| PaymentError::Invalid(format!("invalid webhook json: {err}")))?;
        let event_id = value
            .get("id")
            .and_then(|item| item.as_str())
            .ok_or_else(|| PaymentError::Invalid("missing event id".to_string()))?;
        Ok(PaymentEvent {
            provider: self.name.to_string(),
            event_id: event_id.to_string(),
            event_type: value
                .get("type")
                .and_then(|item| item.as_str())
                .unwrap_or("unknown")
                .to_string(),
            tenant_id: value
                .pointer("/data/object/metadata/tenant_id")
                .and_then(|item| item.as_str())
                .map(str::to_string),
            amount_minor: value
                .pointer("/data/object/amount")
                .and_then(|item| item.as_i64()),
            currency: value
                .pointer("/data/object/currency")
                .and_then(|item| item.as_str())
                .map(str::to_string),
            provider_ref: value
                .pointer("/data/object/id")
                .and_then(|item| item.as_str())
                .map(str::to_string),
        })
    }

    async fn refund(&self, _request: &RefundRequest) -> Result<PaymentEvent, PaymentError> {
        Err(PaymentError::Unavailable)
    }

    async fn subscription_sync(
        &self,
        _request: &SubscriptionSyncRequest,
    ) -> Result<SubscriptionSnapshot, PaymentError> {
        Err(PaymentError::Unavailable)
    }
}

pub fn is_configured(config: &PaymentConfig) -> bool {
    !config.provider.trim().is_empty() && !config.provider.eq_ignore_ascii_case("none")
}
