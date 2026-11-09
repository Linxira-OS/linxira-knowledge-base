//! Self-hosted wallet/ledger/plan core plus the external billing-engine
//! boundary. External engines (Lago/OpenMeter/Stripe Billing) never write the
//! local financial tables; they only emit usage and read entitlement snapshots.

use crate::config::{BillingConfig, Config};
use crate::error::StoreError;
use crate::store::{
    EntitlementRecord, InvoiceRecord, LedgerEntryRecord, LedgerTxResult, PaymentEventRecord,
    PlanRecord, Store, SubscriptionRecord, TopupRecord, WalletRecord,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum BillingError {
    #[error("billing is not enabled")]
    NotEnabled,
    #[error("insufficient balance")]
    Insufficient,
    #[error("billing plan or subscription is not payable in its current state")]
    PaymentRequired,
    #[error("invalid billing request: {0}")]
    Invalid(String),
    #[error("billing store error: {0}")]
    Store(String),
}

impl BillingError {
    pub fn code(&self) -> &'static str {
        match self {
            BillingError::NotEnabled => "billing_not_enabled",
            BillingError::Insufficient | BillingError::PaymentRequired => "payment_required",
            BillingError::Invalid(_) => "invalid_billing_request",
            BillingError::Store(_) => "billing_unavailable",
        }
    }

    pub fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            BillingError::NotEnabled => StatusCode::FORBIDDEN,
            BillingError::Insufficient | BillingError::PaymentRequired => {
                StatusCode::PAYMENT_REQUIRED
            }
            BillingError::Invalid(_) => StatusCode::BAD_REQUEST,
            BillingError::Store(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<StoreError> for BillingError {
    fn from(err: StoreError) -> Self {
        BillingError::Store(err.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionStatus {
    Trial,
    Active,
    PastDue,
    Suspended,
    Cancelled,
}

impl SubscriptionStatus {
    pub fn parse(value: &str) -> Result<Self, BillingError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "trial" => Ok(Self::Trial),
            "active" => Ok(Self::Active),
            "past_due" => Ok(Self::PastDue),
            "suspended" => Ok(Self::Suspended),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(BillingError::Invalid(format!(
                "unknown subscription status {other:?}"
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trial => "trial",
            Self::Active => "active",
            Self::PastDue => "past_due",
            Self::Suspended => "suspended",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn can_transition(self, to: Self) -> bool {
        use SubscriptionStatus::*;
        matches!(
            (self, to),
            (Trial, Active)
                | (Trial, Cancelled)
                | (Active, PastDue)
                | (Active, Cancelled)
                | (PastDue, Active)
                | (PastDue, Suspended)
                | (PastDue, Cancelled)
                | (Suspended, Active)
                | (Suspended, Cancelled)
        )
    }
}

pub fn is_billing_enabled(config: &Config) -> bool {
    config.billing.enabled
}

pub struct Billing {
    store: Arc<dyn Store>,
    config: BillingConfig,
}

struct LedgerEntryInput<'a> {
    account: &'a str,
    direction: &'a str,
    amount_micros: i64,
    idempotency_key: &'a str,
    request_id: Option<&'a str>,
    metadata: serde_json::Value,
}

impl Billing {
    pub fn new(store: Arc<dyn Store>, config: BillingConfig) -> Self {
        Self { store, config }
    }

    pub fn cfg(&self) -> &BillingConfig {
        &self.config
    }

    async fn ensure_wallet(&self, tenant_id: &str) -> Result<WalletRecord, BillingError> {
        if let Some(wallet) = self
            .store
            .billing_get_wallet(tenant_id, &self.config.currency)
            .await?
        {
            return Ok(wallet);
        }
        if let Some(wallet) = self
            .store
            .billing_list_wallets(tenant_id)
            .await?
            .into_iter()
            .next()
        {
            return Ok(wallet);
        }
        let wallet = WalletRecord {
            wallet_id: format!("wal_{}", crate::util::random_hex(12)),
            tenant_id: tenant_id.to_string(),
            currency: self.config.currency.clone(),
            balance_micros: 0,
            status: "active".to_string(),
            updated_at: crate::util::now_rfc3339(),
        };
        self.store.billing_ensure_wallet(&wallet).await?;
        self.store
            .billing_get_wallet(tenant_id, &self.config.currency)
            .await?
            .ok_or_else(|| BillingError::Store("wallet missing after ensure".to_string()))
    }

    pub async fn wallet(&self, tenant_id: &str) -> Result<WalletRecord, BillingError> {
        self.ensure_wallet(tenant_id).await
    }

    fn entries(&self, wallet: &WalletRecord, input: LedgerEntryInput<'_>) -> LedgerEntryRecord {
        LedgerEntryRecord {
            entry_id: format!("leq_{}", crate::util::random_hex(12)),
            group_id: format!("grp_{}", crate::util::random_hex(12)),
            wallet_id: wallet.wallet_id.clone(),
            tenant_id: wallet.tenant_id.clone(),
            account: input.account.to_string(),
            direction: input.direction.to_string(),
            amount_micros: input.amount_micros,
            currency: wallet.currency.clone(),
            idempotency_key: input.idempotency_key.to_string(),
            request_id: input.request_id.map(str::to_string),
            metadata_json: input.metadata.to_string(),
            created_at: crate::util::now_rfc3339(),
        }
    }

    async fn post_balanced(
        &self,
        wallet: &WalletRecord,
        legs: Vec<(&str, &str, i64)>,
        idempotency_key: &str,
        request_id: Option<&str>,
        metadata: serde_json::Value,
    ) -> Result<LedgerTxResult, BillingError> {
        self.post_balanced_forced(wallet, legs, idempotency_key, request_id, metadata, false)
            .await
    }

    async fn post_balanced_forced(
        &self,
        wallet: &WalletRecord,
        legs: Vec<(&str, &str, i64)>,
        idempotency_key: &str,
        request_id: Option<&str>,
        metadata: serde_json::Value,
        force_negative: bool,
    ) -> Result<LedgerTxResult, BillingError> {
        let debit: i64 = legs
            .iter()
            .filter(|(_, direction, _)| *direction == "debit")
            .map(|(_, _, amount)| *amount)
            .sum();
        let credit: i64 = legs
            .iter()
            .filter(|(_, direction, _)| *direction == "credit")
            .map(|(_, _, amount)| *amount)
            .sum();
        if debit != credit {
            return Err(BillingError::Invalid(
                "ledger legs must balance".to_string(),
            ));
        }
        let mut entries = Vec::new();
        for (index, (account, direction, amount)) in legs.iter().enumerate() {
            entries.push(self.entries(
                wallet,
                LedgerEntryInput {
                    account,
                    direction,
                    amount_micros: *amount,
                    idempotency_key: &format!("{idempotency_key}:{index}"),
                    request_id,
                    metadata: metadata.clone(),
                },
            ));
        }
        let balance_delta: i64 = entries
            .iter()
            .filter(|entry| entry.account == "wallet")
            .map(|entry| {
                if entry.direction == "credit" {
                    entry.amount_micros
                } else {
                    -entry.amount_micros
                }
            })
            .sum();
        let allow_negative = force_negative || balance_delta >= 0;
        let result = self
            .store
            .billing_post_ledger(&entries, balance_delta, allow_negative, &wallet.currency)
            .await?;
        if let LedgerTxResult::Applied { .. } = result {
            let _ = self.snapshot_entitlement(&wallet.tenant_id).await;
        }
        Ok(result)
    }

    pub async fn topup(
        &self,
        tenant_id: &str,
        amount_micros: i64,
        channel: &str,
        provider_ref: Option<&str>,
        idempotency_key: &str,
    ) -> Result<TopupRecord, BillingError> {
        if amount_micros <= 0 {
            return Err(BillingError::Invalid(
                "amount_micros must be positive".to_string(),
            ));
        }
        if let Some(existing) = self
            .store
            .billing_get_topup_by_key(tenant_id, idempotency_key)
            .await?
        {
            if existing.status == "paid" {
                return Ok(existing);
            }
        }
        let wallet = self.ensure_wallet(tenant_id).await?;
        let now = crate::util::now_rfc3339();
        let topup = TopupRecord {
            topup_id: format!("top_{}", crate::util::random_hex(12)),
            wallet_id: wallet.wallet_id.clone(),
            tenant_id: tenant_id.to_string(),
            amount_micros,
            currency: wallet.currency.clone(),
            channel: channel.to_string(),
            provider_ref: provider_ref.map(str::to_string),
            status: "pending".to_string(),
            idempotency_key: idempotency_key.to_string(),
            created_at: now.clone(),
            updated_at: now,
        };
        self.store.billing_upsert_topup(&topup).await?;
        Ok(topup)
    }

    pub async fn settle_topup(&self, topup: &TopupRecord) -> Result<TopupRecord, BillingError> {
        let tenant_id = topup.tenant_id.as_str();
        let wallet = self.ensure_wallet(tenant_id).await?;
        let result = self
            .post_balanced(
                &wallet,
                vec![
                    ("wallet", "credit", topup.amount_micros),
                    ("payment_clearing", "debit", topup.amount_micros),
                ],
                &format!("topup:{}", topup.topup_id),
                Some(&topup.topup_id),
                json!({"kind": "topup", "channel": topup.channel}),
            )
            .await?;
        let status = match result {
            LedgerTxResult::Applied { .. } | LedgerTxResult::Duplicate => "paid",
            _ => "pending",
        };
        self.store
            .billing_set_topup_status(&topup.topup_id, status)
            .await?;
        let invoice = InvoiceRecord {
            invoice_id: format!("inv_{}", crate::util::random_hex(12)),
            tenant_id: tenant_id.to_string(),
            period_start: Some(topup.created_at.clone()),
            period_end: Some(crate::util::now_rfc3339()),
            amount_micros: topup.amount_micros,
            currency: topup.currency.clone(),
            status: "paid".to_string(),
            created_at: crate::util::now_rfc3339(),
        };
        self.store.billing_upsert_invoice(&invoice).await?;
        Ok(topup.clone())
    }

    /// Charges usage: plan included tokens cover the first `tokens` of the
    /// period, the remainder is debited from the wallet proportionally to the
    /// micros cost. Refuses (402) when neither source can cover the cost.
    pub async fn charge_usage(
        &self,
        tenant_id: &str,
        cost_micros: i64,
        tokens: i64,
        usage_event_id: &str,
    ) -> Result<(), BillingError> {
        if cost_micros <= 0 {
            return Ok(());
        }
        let mut entitlement = self.entitlement(tenant_id).await;
        let remaining_included = entitlement
            .as_ref()
            .map(|item| (item.included_tokens - item.used_tokens).max(0))
            .unwrap_or(0);
        let covered_tokens = remaining_included.min(tokens.max(0));
        let wallet_micros = if tokens > 0 && covered_tokens < tokens {
            ((cost_micros as i128) * ((tokens - covered_tokens) as i128) / (tokens as i128)) as i64
        } else if tokens <= 0 {
            cost_micros
        } else {
            0
        };
        let wallet = self.ensure_wallet(tenant_id).await?;
        if wallet_micros > 0 {
            match self
                .post_balanced(
                    &wallet,
                    vec![
                        ("wallet", "debit", wallet_micros),
                        ("revenue", "credit", wallet_micros),
                    ],
                    &format!("usage:{usage_event_id}"),
                    Some(usage_event_id),
                    json!({"kind": "usage", "cost_micros": cost_micros, "tokens": tokens}),
                )
                .await?
            {
                LedgerTxResult::Applied { .. } | LedgerTxResult::Duplicate => {}
                LedgerTxResult::Insufficient => return Err(BillingError::Insufficient),
            }
        }
        if let Some(item) = entitlement.as_mut() {
            item.used_tokens += tokens.max(0);
            item.balance_micros = self
                .store
                .billing_get_wallet(tenant_id, &self.config.currency)
                .await?
                .map(|wallet| wallet.balance_micros)
                .unwrap_or(0);
            item.updated_at = crate::util::now_rfc3339();
            self.store.billing_upsert_entitlement(item).await?;
        }
        Ok(())
    }

    /// Reverses a topup (charge.refunded): the wallet is debited and
    /// payment_clearing credited; the wallet may go negative to reflect money
    /// that must be returned to the payer.
    pub async fn reverse_topup_refund(
        &self,
        tenant_id: &str,
        amount_micros: i64,
        reason: &str,
        idempotency_key: &str,
    ) -> Result<(), BillingError> {
        if amount_micros <= 0 {
            return Err(BillingError::Invalid(
                "refund amount must be positive".to_string(),
            ));
        }
        let wallet = self.ensure_wallet(tenant_id).await?;
        match self
            .post_balanced_forced(
                &wallet,
                vec![
                    ("wallet", "debit", amount_micros),
                    ("payment_clearing", "credit", amount_micros),
                ],
                idempotency_key,
                None,
                json!({"kind": "topup_refund", "reason": reason}),
                true,
            )
            .await?
        {
            LedgerTxResult::Applied { .. } | LedgerTxResult::Duplicate => Ok(()),
            LedgerTxResult::Insufficient => Err(BillingError::Insufficient),
        }
    }

    pub async fn refund(
        &self,
        tenant_id: &str,
        amount_micros: i64,
        reason: &str,
        idempotency_key: &str,
    ) -> Result<(), BillingError> {
        if amount_micros <= 0 {
            return Err(BillingError::Invalid(
                "refund amount must be positive".to_string(),
            ));
        }
        let wallet = self.ensure_wallet(tenant_id).await?;
        match self
            .post_balanced(
                &wallet,
                vec![
                    ("wallet", "credit", amount_micros),
                    ("revenue", "debit", amount_micros),
                ],
                idempotency_key,
                None,
                json!({"kind": "refund", "reason": reason}),
            )
            .await?
        {
            LedgerTxResult::Applied { .. } | LedgerTxResult::Duplicate => Ok(()),
            LedgerTxResult::Insufficient => Err(BillingError::Insufficient),
        }
    }

    pub async fn entitlement(&self, tenant_id: &str) -> Option<EntitlementRecord> {
        self.store
            .billing_get_entitlement(tenant_id)
            .await
            .ok()
            .flatten()
    }

    async fn snapshot_entitlement(&self, tenant_id: &str) -> Result<(), BillingError> {
        let wallet = self.ensure_wallet(tenant_id).await?;
        let subscription = self.store.billing_get_subscription(tenant_id).await?;
        let plan = match &subscription {
            Some(subscription) => self.store.billing_get_plan(&subscription.plan_id).await?,
            None => None,
        };
        let existing = self.store.billing_get_entitlement(tenant_id).await?;
        let entitlement = EntitlementRecord {
            tenant_id: tenant_id.to_string(),
            plan_id: plan
                .as_ref()
                .map(|plan| plan.plan_id.clone())
                .or(subscription.as_ref().map(|item| item.plan_id.clone())),
            status: subscription
                .as_ref()
                .map(|item| item.status.clone())
                .unwrap_or_else(|| "active".to_string()),
            balance_micros: wallet.balance_micros,
            included_tokens: plan.as_ref().map(|plan| plan.included_tokens).unwrap_or(0),
            used_tokens: existing.as_ref().map(|item| item.used_tokens).unwrap_or(0),
            currency: wallet.currency.clone(),
            period_start: subscription
                .as_ref()
                .and_then(|item| item.current_period_start.clone()),
            period_end: subscription
                .as_ref()
                .and_then(|item| item.current_period_end.clone()),
            updated_at: crate::util::now_rfc3339(),
        };
        self.store.billing_upsert_entitlement(&entitlement).await?;
        Ok(())
    }

    /// Per-call gate: suspended/cancelled tenants are blocked, active plans
    /// with remaining included tokens pass, otherwise the wallet must cover
    /// the estimate. Returns the source that authorized the call.
    pub async fn authorize_call(
        &self,
        tenant_id: &str,
        estimated_micros: i64,
    ) -> Result<&'static str, BillingError> {
        let entitlement = self.entitlement(tenant_id).await;
        if let Some(item) = &entitlement {
            if matches!(item.status.as_str(), "suspended" | "cancelled") {
                return Err(BillingError::PaymentRequired);
            }
            if item.included_tokens - item.used_tokens > 0 {
                return Ok("plan");
            }
        }
        let wallet = self.ensure_wallet(tenant_id).await?;
        if wallet.status != "active" {
            return Err(BillingError::PaymentRequired);
        }
        if wallet.balance_micros >= estimated_micros {
            Ok("wallet")
        } else {
            Err(BillingError::Insufficient)
        }
    }

    pub async fn subscribe(
        &self,
        tenant_id: &str,
        plan_id: &str,
        trial_days: u32,
    ) -> Result<SubscriptionRecord, BillingError> {
        let plan = self
            .store
            .billing_get_plan(plan_id)
            .await?
            .ok_or_else(|| BillingError::Invalid("plan not found".to_string()))?;
        let now = chrono::Utc::now();
        let status = if trial_days > 0 { "trial" } else { "active" };
        let subscription = SubscriptionRecord {
            subscription_id: format!("sub_{}", crate::util::random_hex(12)),
            tenant_id: tenant_id.to_string(),
            plan_id: plan.plan_id.clone(),
            status: status.to_string(),
            trial_ends_at: if trial_days > 0 {
                Some(
                    (now + chrono::Duration::days(trial_days as i64))
                        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                )
            } else {
                None
            },
            current_period_start: Some(now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
            current_period_end: Some(
                (now + chrono::Duration::days(30))
                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            ),
            past_due_at: None,
            suspended_at: None,
            cancelled_at: None,
            updated_at: crate::util::now_rfc3339(),
        };
        self.store
            .billing_upsert_subscription(&subscription)
            .await?;
        self.snapshot_entitlement(tenant_id).await?;
        Ok(subscription)
    }

    pub async fn transition(
        &self,
        tenant_id: &str,
        to: SubscriptionStatus,
    ) -> Result<SubscriptionRecord, BillingError> {
        let mut subscription = self
            .store
            .billing_get_subscription(tenant_id)
            .await?
            .ok_or_else(|| BillingError::Invalid("subscription not found".to_string()))?;
        let from = SubscriptionStatus::parse(&subscription.status)?;
        if !from.can_transition(to) {
            return Err(BillingError::Invalid(format!(
                "illegal subscription transition {} -> {}",
                from.as_str(),
                to.as_str()
            )));
        }
        subscription.status = to.as_str().to_string();
        let now = crate::util::now_rfc3339();
        match to {
            SubscriptionStatus::PastDue => subscription.past_due_at = Some(now.clone()),
            SubscriptionStatus::Suspended => subscription.suspended_at = Some(now.clone()),
            SubscriptionStatus::Cancelled => subscription.cancelled_at = Some(now.clone()),
            _ => {}
        }
        subscription.updated_at = now;
        self.store
            .billing_upsert_subscription(&subscription)
            .await?;
        self.snapshot_entitlement(tenant_id).await?;
        Ok(subscription)
    }
}

/// External billing engines only emit usage and read entitlement snapshots;
/// they never post local ledger entries.
pub struct ExternalBillingEngine {
    name: String,
    store: Arc<dyn Store>,
}

#[async_trait]
pub trait BillingEngine: Send + Sync {
    fn name(&self) -> &str;
    fn writes_ledger(&self) -> bool;
    async fn on_usage(
        &self,
        tenant_id: &str,
        usage_event_id: &str,
        cost_micros: i64,
    ) -> Result<(), BillingError>;
    async fn entitlement(&self, tenant_id: &str) -> Option<EntitlementRecord>;
}

#[async_trait]
impl BillingEngine for Billing {
    fn name(&self) -> &str {
        "local"
    }

    fn writes_ledger(&self) -> bool {
        true
    }

    async fn on_usage(
        &self,
        tenant_id: &str,
        usage_event_id: &str,
        cost_micros: i64,
    ) -> Result<(), BillingError> {
        self.charge_usage(tenant_id, cost_micros, 0, usage_event_id)
            .await
    }

    async fn entitlement(&self, tenant_id: &str) -> Option<EntitlementRecord> {
        Billing::entitlement(self, tenant_id).await
    }
}

#[async_trait]
impl BillingEngine for ExternalBillingEngine {
    fn name(&self) -> &str {
        &self.name
    }

    fn writes_ledger(&self) -> bool {
        false
    }

    async fn on_usage(
        &self,
        tenant_id: &str,
        usage_event_id: &str,
        cost_micros: i64,
    ) -> Result<(), BillingError> {
        tracing::info!(
            engine = self.name.as_str(),
            tenant = tenant_id,
            usage_event_id,
            cost_micros,
            "forwarded usage event to external billing engine (no local ledger write)"
        );
        Ok(())
    }

    async fn entitlement(&self, tenant_id: &str) -> Option<EntitlementRecord> {
        self.store
            .billing_get_entitlement(tenant_id)
            .await
            .ok()
            .flatten()
    }
}

pub fn engine_from_config(store: Arc<dyn Store>, config: &Config) -> Arc<dyn BillingEngine> {
    let engine = config.billing.engine.trim().to_ascii_lowercase();
    if engine == "local" || engine.is_empty() {
        Arc::new(Billing::new(store, config.billing.clone()))
    } else {
        Arc::new(ExternalBillingEngine {
            name: engine,
            store,
        })
    }
}

pub async fn seed_default_plans(
    store: &Arc<dyn Store>,
    currency: &str,
) -> Result<(), BillingError> {
    let now = crate::util::now_rfc3339();
    let plans = [
        ("free", "Free", 0_i64, 1_000_000_i64),
        ("starter", "Starter", 19_000_000, 5_000_000),
        ("pro", "Pro", 99_000_000, 50_000_000),
    ];
    for (plan_id, name, price_micros, included_tokens) in plans {
        let plan = PlanRecord {
            plan_id: plan_id.to_string(),
            name: name.to_string(),
            price_micros,
            included_tokens,
            currency: currency.to_string(),
            period: "monthly".to_string(),
            status: "active".to_string(),
            features_json: json!({"billing": true}).to_string(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        store.billing_upsert_plan(&plan).await?;
    }
    Ok(())
}

pub fn payment_event_record(
    provider: &str,
    event_id: &str,
    event_type: &str,
    payload: &serde_json::Value,
) -> PaymentEventRecord {
    PaymentEventRecord {
        provider_event_id: format!("{provider}:{event_id}"),
        provider: provider.to_string(),
        event_type: event_type.to_string(),
        payload_json: payload.to_string(),
        status: "received".to_string(),
        received_at: crate::util::now_rfc3339(),
        processed_at: None,
    }
}
