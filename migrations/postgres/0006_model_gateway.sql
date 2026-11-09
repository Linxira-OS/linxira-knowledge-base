-- P0c: model gateway control plane, usage events and token quota.
CREATE TABLE IF NOT EXISTS providers (
    provider_id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    base_url TEXT NOT NULL,
    api_key_env TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    priority BIGINT NOT NULL DEFAULT 100,
    weight BIGINT NOT NULL DEFAULT 1,
    limits_json TEXT NOT NULL DEFAULT '{}',
    timeouts_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS provider_models (
    provider_model_id TEXT PRIMARY KEY,
    provider_id TEXT NOT NULL,
    upstream_model TEXT NOT NULL,
    context_window BIGINT NOT NULL DEFAULT 0,
    pricing_json TEXT NOT NULL DEFAULT '{}',
    capabilities_json TEXT NOT NULL DEFAULT '["chat"]',
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (provider_id, upstream_model)
);

CREATE TABLE IF NOT EXISTS model_aliases (
    public_id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    capabilities_json TEXT NOT NULL DEFAULT '["chat"]',
    status TEXT NOT NULL DEFAULT 'active',
    access_policy_json TEXT NOT NULL DEFAULT '{}',
    price_json TEXT NOT NULL DEFAULT '{}',
    version BIGINT NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS routes (
    route_id TEXT PRIMARY KEY,
    alias_public_id TEXT NOT NULL,
    provider_model_id TEXT NOT NULL,
    weight BIGINT NOT NULL DEFAULT 1,
    priority BIGINT NOT NULL DEFAULT 100,
    health TEXT NOT NULL DEFAULT 'healthy',
    circuit_state TEXT NOT NULL DEFAULT 'closed',
    last_error_at TEXT,
    circuit_open_until TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (alias_public_id, provider_model_id)
);

CREATE INDEX IF NOT EXISTS idx_routes_alias ON routes (alias_public_id, priority);

CREATE TABLE IF NOT EXISTS usage_events (
    usage_event_id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    space_id TEXT,
    user_id TEXT NOT NULL,
    role TEXT NOT NULL,
    source TEXT NOT NULL,
    external_usage_id TEXT,
    alias TEXT NOT NULL,
    provider_model_id TEXT NOT NULL,
    input_tokens BIGINT NOT NULL DEFAULT 0,
    output_tokens BIGINT NOT NULL DEFAULT 0,
    cached_tokens BIGINT NOT NULL DEFAULT 0,
    token_source TEXT NOT NULL DEFAULT 'estimated',
    cost_micros BIGINT NOT NULL DEFAULT 0,
    currency TEXT NOT NULL DEFAULT 'USD',
    provider_cost_micros BIGINT NOT NULL DEFAULT 0,
    latency_ms BIGINT NOT NULL DEFAULT 0,
    status TEXT NOT NULL,
    cache_hit BIGINT NOT NULL DEFAULT 0,
    at TEXT NOT NULL,
    UNIQUE (tenant_id, external_usage_id)
);

CREATE INDEX IF NOT EXISTS idx_usage_tenant ON usage_events (tenant_id, at);
CREATE INDEX IF NOT EXISTS idx_usage_space ON usage_events (tenant_id, space_id, at);
CREATE INDEX IF NOT EXISTS idx_usage_user ON usage_events (tenant_id, user_id, at);
CREATE INDEX IF NOT EXISTS idx_usage_alias ON usage_events (tenant_id, alias, at);

CREATE TABLE IF NOT EXISTS token_quotas (
    quota_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    scope_type TEXT NOT NULL,
    scope_id TEXT NOT NULL,
    period TEXT NOT NULL,
    limit_tokens BIGINT,
    limit_requests BIGINT,
    limit_cost_micros BIGINT,
    currency TEXT NOT NULL DEFAULT 'USD',
    enforcement TEXT NOT NULL DEFAULT 'hard',
    action TEXT NOT NULL DEFAULT 'block',
    enabled BIGINT NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_token_quotas_scope
    ON token_quotas (tenant_id, scope_type, scope_id, enabled);

CREATE TABLE IF NOT EXISTS quota_counters (
    tenant_id TEXT NOT NULL,
    scope_type TEXT NOT NULL,
    scope_id TEXT NOT NULL,
    period_start TEXT NOT NULL,
    reserved_tokens BIGINT NOT NULL DEFAULT 0,
    used_tokens BIGINT NOT NULL DEFAULT 0,
    reserved_requests BIGINT NOT NULL DEFAULT 0,
    used_requests BIGINT NOT NULL DEFAULT 0,
    reserved_cost_micros BIGINT NOT NULL DEFAULT 0,
    used_cost_micros BIGINT NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, scope_type, scope_id, period_start)
);

CREATE TABLE IF NOT EXISTS quota_reservations (
    reservation_id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    scope_type TEXT NOT NULL,
    scope_id TEXT NOT NULL,
    period_start TEXT NOT NULL,
    reserved_tokens BIGINT NOT NULL,
    reserved_requests BIGINT NOT NULL,
    reserved_cost_micros BIGINT NOT NULL,
    status TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_quota_reservations_request
    ON quota_reservations (tenant_id, request_id);
