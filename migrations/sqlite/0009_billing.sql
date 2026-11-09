-- SPDX-License-Identifier: AGPL-3.0-only
CREATE TABLE IF NOT EXISTS plans (
    plan_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    price_micros INTEGER NOT NULL,
    included_tokens INTEGER NOT NULL,
    currency TEXT NOT NULL,
    period TEXT NOT NULL,
    status TEXT NOT NULL,
    features_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS subscriptions (
    subscription_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    plan_id TEXT NOT NULL,
    status TEXT NOT NULL,
    trial_ends_at TEXT,
    current_period_start TEXT,
    current_period_end TEXT,
    past_due_at TEXT,
    suspended_at TEXT,
    cancelled_at TEXT,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_subscriptions_tenant ON subscriptions (tenant_id);

CREATE TABLE IF NOT EXISTS wallets (
    wallet_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    currency TEXT NOT NULL,
    balance_micros INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (tenant_id, currency)
);

CREATE TABLE IF NOT EXISTS ledger_entries (
    entry_id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL,
    wallet_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    account TEXT NOT NULL,
    direction TEXT NOT NULL,
    amount_micros INTEGER NOT NULL,
    currency TEXT NOT NULL,
    idempotency_key TEXT NOT NULL UNIQUE,
    request_id TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ledger_wallet ON ledger_entries (wallet_id, created_at);
CREATE INDEX IF NOT EXISTS idx_ledger_tenant ON ledger_entries (tenant_id, created_at);
CREATE INDEX IF NOT EXISTS idx_ledger_group ON ledger_entries (group_id);

CREATE TABLE IF NOT EXISTS topups (
    topup_id TEXT PRIMARY KEY,
    wallet_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    amount_micros INTEGER NOT NULL,
    currency TEXT NOT NULL,
    channel TEXT NOT NULL,
    provider_ref TEXT,
    status TEXT NOT NULL,
    idempotency_key TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_topups_tenant ON topups (tenant_id, created_at);

CREATE TABLE IF NOT EXISTS invoices (
    invoice_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    period_start TEXT,
    period_end TEXT,
    amount_micros INTEGER NOT NULL,
    currency TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_invoices_tenant ON invoices (tenant_id, created_at);

CREATE TABLE IF NOT EXISTS payment_events (
    provider_event_id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    status TEXT NOT NULL,
    received_at TEXT NOT NULL,
    processed_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_payment_events_provider ON payment_events (provider, received_at);

CREATE TABLE IF NOT EXISTS entitlements (
    tenant_id TEXT PRIMARY KEY,
    plan_id TEXT,
    status TEXT NOT NULL,
    balance_micros INTEGER NOT NULL DEFAULT 0,
    included_tokens INTEGER NOT NULL DEFAULT 0,
    used_tokens INTEGER NOT NULL DEFAULT 0,
    currency TEXT NOT NULL,
    period_start TEXT,
    period_end TEXT,
    updated_at TEXT NOT NULL
);
