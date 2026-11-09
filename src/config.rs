use crate::util::{is_loopback_host, mask_database_url, parse_bool, split_bind_host};
use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileName {
    Solo,
    Team,
    Enterprise,
}

impl ProfileName {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "solo" => Ok(ProfileName::Solo),
            "team" => Ok(ProfileName::Team),
            "enterprise" => Ok(ProfileName::Enterprise),
            other => Err(format!(
                "unknown profile {other:?}; expected solo, team or enterprise"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ProfileName::Solo => "solo",
            ProfileName::Team => "team",
            ProfileName::Enterprise => "enterprise",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    Sqlite,
    Postgres,
}

impl Backend {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "sqlite" => Ok(Backend::Sqlite),
            "postgres" | "postgresql" | "pg" => Ok(Backend::Postgres),
            other => Err(format!(
                "unknown storage backend {other:?}; expected sqlite or postgres"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Backend::Sqlite => "sqlite",
            Backend::Postgres => "postgres",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    None,
    Session,
}

impl AuthMode {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "none" => Ok(AuthMode::None),
            "session" => Ok(AuthMode::Session),
            other => Err(format!(
                "unknown auth mode {other:?}; expected none or session"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            AuthMode::None => "none",
            AuthMode::Session => "session",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub profile: Option<ProfileName>,
    pub server: ServerConfig,
    pub auth: AuthConfig,
    pub sso: SsoConfig,
    pub storage: StorageConfig,
    pub models: ModelsConfig,
    pub retrieval: RetrievalConfig,
    pub seed: SeedConfig,
    pub spaces: SpacesConfig,
    pub groups: GroupsConfig,
    pub limits: LimitsConfig,
    pub deployment: DeploymentConfig,
    pub features: FeatureOverrides,
    pub i18n: I18nConfig,
    pub payment: PaymentConfig,
    pub billing: BillingConfig,
    pub summary: SummaryConfig,
    pub query: QueryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct QueryConfig {
    pub rewrite_enabled: bool,
    pub rewrite_alias: String,
    pub context_messages: usize,
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            rewrite_enabled: true,
            rewrite_alias: "default".to_string(),
            context_messages: 6,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SummaryConfig {
    pub enabled: bool,
    pub auto: bool,
    pub model_alias: String,
    pub prompt_version: String,
    pub max_chars: usize,
    pub batch: usize,
    pub timeout_ms: u64,
    pub max_attempts: u32,
    pub retry_backoff_ms: u64,
}

impl Default for SummaryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto: true,
            model_alias: "default".to_string(),
            prompt_version: "summary-v1".to_string(),
            max_chars: 1200,
            batch: 8,
            timeout_ms: 15_000,
            max_attempts: 3,
            retry_backoff_ms: 1_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentMode {
    Solo,
    Team,
    EnterpriseInternal,
    EnterpriseExternal,
}

impl DeploymentMode {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "solo" => Ok(Self::Solo),
            "team" => Ok(Self::Team),
            "enterprise_internal" => Ok(Self::EnterpriseInternal),
            "enterprise_external" => Ok(Self::EnterpriseExternal),
            other => Err(format!("unknown deployment mode {other:?}")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Solo => "solo",
            Self::Team => "team",
            Self::EnterpriseInternal => "enterprise_internal",
            Self::EnterpriseExternal => "enterprise_external",
        }
    }

    pub fn profile(self) -> ProfileName {
        match self {
            Self::Solo => ProfileName::Solo,
            Self::Team => ProfileName::Team,
            Self::EnterpriseInternal | Self::EnterpriseExternal => ProfileName::Enterprise,
        }
    }

    pub fn default_features(self) -> FeatureSet {
        let mut features = FeatureSet {
            signup: false,
            billing: false,
            connectors: true,
            sso: false,
            multi_tenant: false,
            public_api: false,
            mcp: false,
            audit: true,
            rate_limit: true,
            public_share: false,
        };
        match self {
            Self::Solo => {
                features.connectors = false;
            }
            Self::Team => {}
            Self::EnterpriseInternal => {
                features.sso = true;
                features.multi_tenant = true;
                features.public_api = true;
                features.mcp = true;
            }
            Self::EnterpriseExternal => {
                features.signup = true;
                features.billing = true;
                features.sso = true;
                features.multi_tenant = true;
                features.public_api = true;
                features.mcp = true;
                features.public_share = true;
            }
        }
        features
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DeploymentConfig {
    pub mode: Option<DeploymentMode>,
    pub commercial: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    Signup,
    Billing,
    Connectors,
    Sso,
    MultiTenant,
    PublicApi,
    Mcp,
    Audit,
    RateLimit,
    PublicShare,
}

impl Feature {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Signup => "signup",
            Self::Billing => "billing",
            Self::Connectors => "connectors",
            Self::Sso => "sso",
            Self::MultiTenant => "multi_tenant",
            Self::PublicApi => "public_api",
            Self::Mcp => "mcp",
            Self::Audit => "audit",
            Self::RateLimit => "rate_limit",
            Self::PublicShare => "public_share",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FeatureOverrides {
    pub signup: Option<bool>,
    pub billing: Option<bool>,
    pub connectors: Option<bool>,
    pub sso: Option<bool>,
    pub multi_tenant: Option<bool>,
    pub public_api: Option<bool>,
    pub mcp: Option<bool>,
    pub audit: Option<bool>,
    pub rate_limit: Option<bool>,
    pub public_share: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureSet {
    pub signup: bool,
    pub billing: bool,
    pub connectors: bool,
    pub sso: bool,
    pub multi_tenant: bool,
    pub public_api: bool,
    pub mcp: bool,
    pub audit: bool,
    pub rate_limit: bool,
    pub public_share: bool,
}

impl FeatureOverrides {
    pub fn resolve(&self, mode: DeploymentMode) -> FeatureSet {
        let mut out = mode.default_features();
        macro_rules! apply {
            ($field:ident) => {
                if let Some(value) = self.$field {
                    out.$field = value;
                }
            };
        }
        apply!(signup);
        apply!(billing);
        apply!(connectors);
        apply!(sso);
        apply!(multi_tenant);
        apply!(public_api);
        apply!(mcp);
        apply!(audit);
        apply!(rate_limit);
        apply!(public_share);
        out
    }
}

impl FeatureSet {
    pub fn enabled(&self, feature: Feature) -> bool {
        match feature {
            Feature::Signup => self.signup,
            Feature::Billing => self.billing,
            Feature::Connectors => self.connectors,
            Feature::Sso => self.sso,
            Feature::MultiTenant => self.multi_tenant,
            Feature::PublicApi => self.public_api,
            Feature::Mcp => self.mcp,
            Feature::Audit => self.audit,
            Feature::RateLimit => self.rate_limit,
            Feature::PublicShare => self.public_share,
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "signup": self.signup,
            "billing": self.billing,
            "connectors": self.connectors,
            "sso": self.sso,
            "multi_tenant": self.multi_tenant,
            "public_api": self.public_api,
            "mcp": self.mcp,
            "audit": self.audit,
            "rate_limit": self.rate_limit,
            "public_share": self.public_share,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct I18nConfig {
    pub default_locale: String,
    pub supported_locales: Vec<String>,
    pub tenant_locale: Option<String>,
}

impl Default for I18nConfig {
    fn default() -> Self {
        Self {
            default_locale: "zh-CN".to_string(),
            supported_locales: vec!["zh-CN".to_string(), "en".to_string()],
            tenant_locale: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PaymentConfig {
    pub provider: String,
    pub currency: String,
    pub mode: String,
    pub secret_env: Option<String>,
}

impl Default for PaymentConfig {
    fn default() -> Self {
        Self {
            provider: "none".to_string(),
            currency: "USD".to_string(),
            mode: "test".to_string(),
            secret_env: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BillingConfig {
    /// Self-hosted wallet/ledger is off unless explicitly enabled.
    pub enabled: bool,
    /// local | lago | openmeter | stripe_billing
    pub engine: String,
    pub currency: String,
    pub trial_days: u32,
    pub grace_days: u32,
    pub stripe_secret_env: Option<String>,
    pub stripe_webhook_secret_env: Option<String>,
    pub webhook_tolerance_seconds: u64,
}

impl Default for BillingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            engine: "local".to_string(),
            currency: "USD".to_string(),
            trial_days: 14,
            grace_days: 7,
            stripe_secret_env: Some("STRIPE_SECRET_KEY".to_string()),
            stripe_webhook_secret_env: Some("STRIPE_WEBHOOK_SECRET".to_string()),
            webhook_tolerance_seconds: 300,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LimitsConfig {
    pub qps_instance: u32,
    pub qps_ip: u32,
    pub qps_user: u32,
    pub qps_token: u32,
    pub qps_tenant: u32,
    pub qps_space: u32,
    pub max_concurrency_instance: u32,
    pub max_concurrency_user: u32,
    pub max_concurrency_tenant: u32,
    pub max_concurrency_space: u32,
    pub max_queue: u32,
    pub daily_requests_token: u32,
    pub retry_after_seconds: u64,
    pub max_query_chars: usize,
    pub max_top_k: usize,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            qps_instance: 1000,
            qps_ip: 100,
            qps_user: 500,
            qps_token: 200,
            qps_tenant: 1000,
            qps_space: 500,
            max_concurrency_instance: 256,
            max_concurrency_user: 64,
            max_concurrency_tenant: 256,
            max_concurrency_space: 64,
            max_queue: 512,
            daily_requests_token: 1_000_000,
            retry_after_seconds: 1,
            max_query_chars: 8192,
            max_top_k: 100,
        }
    }
}

impl LimitsConfig {
    pub fn tighten_for_team(&mut self) {
        let defaults = LimitsConfig::default();
        if self.qps_instance == defaults.qps_instance {
            self.qps_instance = 500;
        }
        if self.qps_ip == defaults.qps_ip {
            self.qps_ip = 30;
        }
        if self.qps_user == defaults.qps_user {
            self.qps_user = 100;
        }
        if self.qps_token == defaults.qps_token {
            self.qps_token = 60;
        }
        if self.qps_tenant == defaults.qps_tenant {
            self.qps_tenant = 300;
        }
        if self.qps_space == defaults.qps_space {
            self.qps_space = 120;
        }
        if self.max_concurrency_instance == defaults.max_concurrency_instance {
            self.max_concurrency_instance = 128;
        }
        if self.max_queue == defaults.max_queue {
            self.max_queue = 128;
        }
        if self.daily_requests_token == defaults.daily_requests_token {
            self.daily_requests_token = 200_000;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SpacesConfig {
    pub allow_user_creation: bool,
    pub max_per_user: u32,
    pub default_storage_bytes: Option<i64>,
    pub default_documents: Option<i64>,
    pub default_chunks: Option<i64>,
    pub default_members: Option<i64>,
}

impl Default for SpacesConfig {
    fn default() -> Self {
        Self {
            allow_user_creation: true,
            max_per_user: 10,
            default_storage_bytes: None,
            default_documents: None,
            default_chunks: None,
            default_members: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GroupsConfig {
    pub allow_user_creation: bool,
}

impl Default for GroupsConfig {
    fn default() -> Self {
        Self {
            allow_user_creation: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub bind: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:7331".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    pub mode: AuthMode,
    pub cookie_name: String,
    pub cookie_secure: bool,
    pub session_ttl_hours: u64,
    pub csrf: bool,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            mode: AuthMode::None,
            cookie_name: "lkb_session".to_string(),
            cookie_secure: false,
            session_ttl_hours: 24 * 7,
            csrf: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SsoConfig {
    /// off | trusted_header
    pub mode: String,
    pub user_header: String,
    pub email_header: String,
    pub groups_header: String,
    pub trusted_proxies: Vec<String>,
    pub auto_provision: bool,
    pub default_role: String,
    pub group_role_map: BTreeMap<String, String>,
    pub allowed_email_domains: Vec<String>,
    pub session_ttl_hours: Option<u64>,
}

impl Default for SsoConfig {
    fn default() -> Self {
        Self {
            mode: "off".to_string(),
            user_header: "X-Auth-Request-User".to_string(),
            email_header: "X-Auth-Request-Email".to_string(),
            groups_header: "X-Auth-Request-Groups".to_string(),
            trusted_proxies: Vec::new(),
            auto_provision: true,
            default_role: "user".to_string(),
            group_role_map: BTreeMap::new(),
            allowed_email_domains: Vec::new(),
            session_ttl_hours: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    pub backend: Backend,
    /// Name of the environment variable that carries the PostgreSQL URL.
    pub url_env: String,
    pub sqlite_path: PathBuf,
    pub tenant_id: String,
    pub default_space: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: Backend::Sqlite,
            url_env: "LKB_DATABASE_URL".to_string(),
            sqlite_path: PathBuf::from("data/lkb.sqlite"),
            tenant_id: "t1".to_string(),
            default_space: "s_default".to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelsConfig {
    pub llm: ModelConfig,
    pub embedding: ModelConfig,
    pub judge: ModelConfig,
    pub extractor: ModelConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RetrievalConfig {
    /// `bm25`, `bm25_judge`, `event`, `event_entity`, `hybrid`, or `auto`.
    pub mode: String,
    pub judge_alias: String,
    /// 1 (default) or 2 event-entity expansion hops.
    pub event_depth: u32,
    pub event_max_seeds: usize,
    pub event_max_events: usize,
    pub event_timeout_ms: u64,
    pub event_text_weight: f64,
    pub rrf_bm25_weight: f64,
    pub rrf_dense_weight: f64,
    pub rrf_event_weight: f64,
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            mode: "auto".to_string(),
            judge_alias: "default".to_string(),
            event_depth: 1,
            event_max_seeds: 10,
            event_max_events: 50,
            event_timeout_ms: 800,
            event_text_weight: 0.25,
            rrf_bm25_weight: 1.0,
            rrf_dense_weight: 1.0,
            rrf_event_weight: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SeedConfig {
    pub enabled: bool,
    pub auto: bool,
    pub space: String,
}

impl Default for SeedConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto: true,
            space: "product".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelConfig {
    pub provider: String,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub api_key_env: Option<String>,
    pub timeout_ms: u64,
    pub max_retries: u32,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: "none".to_string(),
            base_url: None,
            model: None,
            api_key_env: None,
            timeout_ms: 15_000,
            max_retries: 2,
        }
    }
}

impl ModelConfig {
    pub fn enabled(&self) -> bool {
        !self.provider.trim().is_empty() && !self.provider.eq_ignore_ascii_case("none")
    }

    pub fn api_key(&self) -> Option<String> {
        self.api_key_env
            .as_deref()
            .and_then(|name| env::var(name).ok())
            .filter(|value| !value.trim().is_empty())
    }

    pub fn api_key_state(&self) -> &'static str {
        match self.api_key_env.as_deref() {
            Some(name)
                if env::var(name)
                    .map(|v| !v.trim().is_empty())
                    .unwrap_or(false) =>
            {
                "set"
            }
            Some(_) => "missing",
            None => "not configured",
        }
    }

    pub fn base_url_or_default(&self) -> Option<String> {
        self.base_url.clone().or_else(
            || match self.provider.trim().to_ascii_lowercase().as_str() {
                "openai" => Some("https://api.openai.com/v1".to_string()),
                _ => None,
            },
        )
    }
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let mut config = if path.exists() {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("cannot read config {}", path.display()))?;
            toml::from_str::<Config>(&text)
                .with_context(|| format!("cannot parse config {}", path.display()))?
        } else {
            Config::default()
        };
        config.apply_env()?;
        config.normalize();
        config.validate()?;
        Ok(config)
    }

    fn apply_env(&mut self) -> anyhow::Result<()> {
        if let Ok(value) = env::var("LKB_PROFILE") {
            self.profile = Some(ProfileName::parse(&value).map_err(anyhow::Error::msg)?);
        }
        if let Ok(value) = env::var("LKB_BIND") {
            self.server.bind = value;
        }
        if let Ok(value) = env::var("LKB_AUTH_MODE") {
            self.auth.mode = AuthMode::parse(&value).map_err(anyhow::Error::msg)?;
        }
        if let Ok(value) = env::var("LKB_COOKIE_NAME") {
            self.auth.cookie_name = value;
        }
        if let Ok(value) = env::var("LKB_COOKIE_SECURE") {
            self.auth.cookie_secure = parse_bool(&value).map_err(anyhow::Error::msg)?;
        }
        if let Ok(value) = env::var("LKB_SESSION_TTL_HOURS") {
            self.auth.session_ttl_hours = value
                .parse()
                .context("LKB_SESSION_TTL_HOURS must be an integer")?;
        }
        if let Ok(value) = env::var("LKB_CSRF") {
            self.auth.csrf = parse_bool(&value).map_err(anyhow::Error::msg)?;
        }
        if let Ok(value) = env::var("LKB_STORAGE_BACKEND") {
            self.storage.backend = Backend::parse(&value).map_err(anyhow::Error::msg)?;
        }
        if let Ok(value) = env::var("LKB_SQLITE_PATH") {
            self.storage.sqlite_path = PathBuf::from(value);
        }
        if let Ok(value) = env::var("LKB_URL_ENV") {
            self.storage.url_env = value;
        }
        // Keep the URL in the environment; it is intentionally not copied
        // into the serializable config or printed unmasked. The backend is
        // chosen by storage.backend, not inferred from the presence of a URL.
        if let Ok(value) = env::var("LKB_STORAGE_TENANT_ID") {
            self.storage.tenant_id = value;
        }
        if let Ok(value) = env::var("LKB_DEFAULT_SPACE") {
            self.storage.default_space = value;
        }
        if let Ok(value) = env::var("LKB_RETRIEVAL_MODE") {
            self.retrieval.mode = value;
        }
        if let Ok(value) = env::var("LKB_SEED_ENABLED") {
            self.seed.enabled = parse_bool(&value).map_err(anyhow::Error::msg)?;
        }
        if let Ok(value) = env::var("LKB_SEED_AUTO") {
            self.seed.auto = parse_bool(&value).map_err(anyhow::Error::msg)?;
        }
        if let Ok(value) = env::var("LKB_SEED_SPACE") {
            self.seed.space = value;
        }
        if let Ok(value) = env::var("LKB_SPACES_ALLOW_USER_CREATION") {
            self.spaces.allow_user_creation = parse_bool(&value).map_err(anyhow::Error::msg)?;
        }
        if let Ok(value) = env::var("LKB_SPACES_MAX_PER_USER") {
            self.spaces.max_per_user = value
                .parse()
                .context("LKB_SPACES_MAX_PER_USER must be an integer")?;
        }
        if let Ok(value) = env::var("LKB_GROUPS_ALLOW_USER_CREATION") {
            self.groups.allow_user_creation = parse_bool(&value).map_err(anyhow::Error::msg)?;
        }
        if let Ok(value) = env::var("LKB_DEPLOYMENT_MODE") {
            self.deployment.mode = Some(DeploymentMode::parse(&value).map_err(anyhow::Error::msg)?);
        }
        if let Ok(value) = env::var("LKB_COMMERCIAL") {
            self.deployment.commercial = parse_bool(&value).map_err(anyhow::Error::msg)?;
        }
        if let Ok(value) = env::var("LKB_I18N_DEFAULT_LOCALE") {
            self.i18n.default_locale = value;
        }
        if let Ok(value) = env::var("LKB_PAYMENT_PROVIDER") {
            self.payment.provider = value;
        }
        Ok(())
    }

    fn normalize(&mut self) {
        let inferred = match self.storage.backend {
            Backend::Sqlite => ProfileName::Solo,
            Backend::Postgres => ProfileName::Team,
        };
        let profile = self.profile.unwrap_or(inferred);
        self.profile = Some(profile);
        if profile == ProfileName::Solo {
            self.storage.tenant_id = crate::governance::SOLO_TENANT.to_string();
            self.storage.default_space = crate::governance::SOLO_SPACE.to_string();
            self.spaces.allow_user_creation = false;
            self.groups.allow_user_creation = false;
        } else {
            self.limits.tighten_for_team();
        }
    }

    pub fn profile_name(&self) -> ProfileName {
        self.profile.unwrap_or(ProfileName::Solo)
    }

    pub fn deployment_mode(&self) -> DeploymentMode {
        self.deployment.mode.unwrap_or(match self.profile_name() {
            ProfileName::Solo => DeploymentMode::Solo,
            ProfileName::Team => DeploymentMode::Team,
            ProfileName::Enterprise => DeploymentMode::EnterpriseInternal,
        })
    }

    pub fn features(&self) -> FeatureSet {
        self.features.resolve(self.deployment_mode())
    }

    pub fn feature_enabled(&self, feature: Feature) -> bool {
        self.features().enabled(feature)
    }

    /// Startup warnings that should be logged even though the config is valid.
    pub fn sso_warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        if self.sso.mode.eq_ignore_ascii_case("trusted_header")
            && self.sso.trusted_proxies.is_empty()
        {
            warnings.push(
                "SSO trusted_header is enabled but trusted_proxies is empty; SSO will deny all peers"
                    .to_string(),
            );
        }
        warnings
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        let profile = self.profile_name();
        match (profile, self.storage.backend) {
            (ProfileName::Solo, Backend::Postgres) => {
                bail!("profile solo requires storage.backend = \"sqlite\"")
            }
            (ProfileName::Team | ProfileName::Enterprise, Backend::Sqlite) => {
                bail!("profile team/enterprise requires storage.backend = \"postgres\"")
            }
            _ => {}
        }

        let mode = self.deployment_mode();
        if mode.profile() != profile {
            bail!(
                "deployment.mode {:?} is incompatible with profile {:?}",
                mode.as_str(),
                profile.as_str()
            );
        }
        let features = self.features();
        if self.deployment.commercial && !features.billing {
            bail!("deployment.commercial = true requires features.billing = true");
        }
        match mode {
            DeploymentMode::Solo => {
                if features.billing {
                    bail!("solo deployment cannot enable billing");
                }
            }
            DeploymentMode::EnterpriseExternal => {
                if !features.signup
                    || !features.billing
                    || !features.multi_tenant
                    || !features.public_api
                {
                    bail!(
                        "enterprise_external requires signup, billing, multi_tenant and public_api"
                    );
                }
            }
            DeploymentMode::EnterpriseInternal | DeploymentMode::Team => {}
        }
        if mode.profile() == ProfileName::Enterprise && !features.audit {
            bail!("enterprise deployments cannot disable audit");
        }
        if self.i18n.default_locale.trim().is_empty() {
            bail!("i18n.default_locale must not be empty");
        }
        if self.i18n.supported_locales.is_empty() {
            bail!("i18n.supported_locales must not be empty");
        }
        if self.i18n.supported_locales.len() > 28 {
            bail!("i18n.supported_locales supports at most 28 locales (extension ceiling)");
        }
        if !self
            .i18n
            .supported_locales
            .iter()
            .any(|value| value.eq_ignore_ascii_case(&self.i18n.default_locale))
        {
            bail!("i18n.default_locale must be present in i18n.supported_locales");
        }
        if !self
            .i18n
            .supported_locales
            .iter()
            .any(|value| value.eq_ignore_ascii_case("en"))
        {
            bail!("i18n.supported_locales must include en as fallback");
        }
        if self.summary.model_alias.trim().is_empty() {
            bail!("summary.model_alias must not be empty");
        }
        if self.summary.prompt_version.trim().is_empty() {
            bail!("summary.prompt_version must not be empty");
        }
        if self.summary.max_chars == 0 || self.summary.batch == 0 || self.summary.timeout_ms == 0 {
            bail!("summary max_chars/batch/timeout_ms must be greater than zero");
        }
        if self.summary.max_attempts == 0 {
            bail!("summary.max_attempts must be greater than zero");
        }
        if self.query.rewrite_alias.trim().is_empty() || self.query.context_messages == 0 {
            bail!("query.rewrite_alias and query.context_messages must be non-empty/positive");
        }
        let sso_mode = self.sso.mode.trim().to_ascii_lowercase();
        if !matches!(sso_mode.as_str(), "off" | "trusted_header") {
            bail!(
                "sso.mode must be off or trusted_header, got {:?}",
                self.sso.mode
            );
        }
        if self.sso.user_header.trim().is_empty()
            || self.sso.email_header.trim().is_empty()
            || self.sso.groups_header.trim().is_empty()
        {
            bail!("sso.user_header/email_header/groups_header must not be empty");
        }
        for proxy in &self.sso.trusted_proxies {
            crate::web::parse_trusted_proxy(proxy).map_err(|err| anyhow::anyhow!(err))?;
        }
        crate::governance::UserRole::parse(&self.sso.default_role)
            .map_err(|err| anyhow::anyhow!("sso.default_role: {err}"))?;
        for (group, role) in &self.sso.group_role_map {
            if group.trim().is_empty() {
                bail!("sso.group_role_map keys must not be empty");
            }
            crate::governance::UserRole::parse(role)
                .map_err(|err| anyhow::anyhow!("sso.group_role_map[{group:?}]: {err}"))?;
        }
        for domain in &self.sso.allowed_email_domains {
            let domain = domain.trim();
            if domain.is_empty()
                || domain.contains('@')
                || domain.contains('/')
                || domain.chars().any(char::is_whitespace)
            {
                bail!("sso.allowed_email_domains contains an invalid domain {domain:?}");
            }
        }
        if let Some(hours) = self.sso.session_ttl_hours {
            if hours == 0 {
                bail!("sso.session_ttl_hours must be greater than zero when set");
            }
        }
        let currency = self.payment.currency.trim();
        if currency.len() != 3 || !currency.chars().all(|ch| ch.is_ascii_uppercase()) {
            bail!("payment.currency must be an ISO 4217 three-letter uppercase code");
        }
        if self.payment.mode != "test" && self.payment.mode != "live" {
            bail!("payment.mode must be test or live");
        }
        if let Some(secret_env) = &self.payment.secret_env {
            if secret_env.trim().is_empty() || secret_env.contains('=') {
                bail!(
                    "payment.secret_env must be an environment variable name, not a secret value"
                );
            }
        }

        let host = split_bind_host(&self.server.bind);
        if host.trim().is_empty() || !self.server.bind.contains(':') {
            bail!("server.bind must be host:port, got {:?}", self.server.bind);
        }
        if self.auth.mode == AuthMode::None && !is_loopback_host(host) {
            bail!(
                "refusing to start: auth.mode = \"none\" while server.bind {:?} is not loopback",
                self.server.bind
            );
        }
        if self.auth.session_ttl_hours == 0 {
            bail!("auth.session_ttl_hours must be greater than zero");
        }
        if self.storage.tenant_id.trim().is_empty() {
            bail!("storage.tenant_id must not be empty");
        }
        if self.storage.default_space.trim().is_empty() {
            bail!("storage.default_space must not be empty");
        }
        if self.storage.default_space.contains('/')
            || self.storage.default_space.contains('\\')
            || self.storage.default_space == ".."
            || self.storage.default_space == "."
        {
            bail!("storage.default_space must be a single path segment");
        }
        if self.seed.space.trim().is_empty()
            || self.seed.space.contains('/')
            || self.seed.space.contains('\\')
            || self.seed.space == ".."
            || self.seed.space == "."
        {
            bail!("seed.space must be a non-empty single path segment");
        }
        if self.storage.backend == Backend::Postgres && self.database_url().is_none() {
            bail!(
                "storage backend is postgres but environment variable {} is not set",
                self.storage.url_env
            );
        }
        let mode = self.retrieval.mode.trim().to_ascii_lowercase();
        if !matches!(
            mode.as_str(),
            "bm25" | "keyword" | "bm25_judge" | "event" | "event_entity" | "hybrid" | "auto"
        ) {
            bail!("retrieval.mode must be bm25, bm25_judge, event, event_entity, hybrid, or auto");
        }
        if !(1..=2).contains(&self.retrieval.event_depth) {
            bail!("retrieval.event_depth must be 1 or 2");
        }
        if !(1..=50).contains(&self.retrieval.event_max_seeds) {
            bail!("retrieval.event_max_seeds must be between 1 and 50");
        }
        if !(1..=500).contains(&self.retrieval.event_max_events) {
            bail!("retrieval.event_max_events must be between 1 and 500");
        }
        if !(1..=10_000).contains(&self.retrieval.event_timeout_ms) {
            bail!("retrieval.event_timeout_ms must be between 1 and 10000");
        }
        for (name, value) in [
            ("event_text_weight", self.retrieval.event_text_weight),
            ("rrf_bm25_weight", self.retrieval.rrf_bm25_weight),
            ("rrf_dense_weight", self.retrieval.rrf_dense_weight),
            ("rrf_event_weight", self.retrieval.rrf_event_weight),
        ] {
            if !value.is_finite() || value < 0.0 {
                bail!("retrieval.{name} must be a finite non-negative number");
            }
        }
        self.models.llm.validate("models.llm")?;
        self.models.embedding.validate("models.embedding")?;
        self.models.judge.validate("models.judge")?;
        self.models.extractor.validate("models.extractor")?;
        Ok(())
    }

    pub fn database_url(&self) -> Option<String> {
        env::var(&self.storage.url_env)
            .ok()
            .filter(|value| !value.trim().is_empty())
    }

    pub fn cookie_secure_for_host(&self, host_header: Option<&str>) -> bool {
        if self.auth.cookie_secure {
            return true;
        }
        match host_header {
            Some(host) => !is_loopback_host(split_bind_host(host)),
            None => false,
        }
    }

    pub fn describe(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("profile = {}\n", self.profile_name().as_str()));
        out.push_str(&format!("server.bind = {}\n", self.server.bind));
        out.push_str(&format!("auth.mode = {}\n", self.auth.mode.as_str()));
        out.push_str(&format!("auth.cookie_name = {}\n", self.auth.cookie_name));
        out.push_str(&format!(
            "auth.cookie_secure = {}\n",
            self.auth.cookie_secure
        ));
        out.push_str(&format!(
            "auth.session_ttl_hours = {}\n",
            self.auth.session_ttl_hours
        ));
        out.push_str(&format!("auth.csrf = {}\n", self.auth.csrf));
        out.push_str(&format!(
            "storage.backend = {}\n",
            self.storage.backend.as_str()
        ));
        out.push_str(&format!(
            "storage.sqlite_path = {}\n",
            self.storage.sqlite_path.display()
        ));
        out.push_str(&format!("storage.url_env = {}\n", self.storage.url_env));
        match self.database_url() {
            Some(url) => out.push_str(&format!(
                "storage.database_url = {}\n",
                mask_database_url(&url)
            )),
            None => out.push_str("storage.database_url = <not set>\n"),
        }
        out.push_str(&format!("storage.tenant_id = {}\n", self.storage.tenant_id));
        out.push_str(&format!(
            "storage.default_space = {}\n",
            self.storage.default_space
        ));
        out.push_str(&format!("retrieval.mode = {}\n", self.retrieval.mode));
        out.push_str(&format!(
            "seed.enabled = {}\nseed.auto = {}\nseed.space = {}\n",
            self.seed.enabled, self.seed.auto, self.seed.space
        ));
        out.push_str(&format!(
            "spaces.allow_user_creation = {}\nspaces.max_per_user = {}\n",
            self.spaces.allow_user_creation, self.spaces.max_per_user
        ));
        out.push_str(&format!(
            "groups.allow_user_creation = {}\n",
            self.groups.allow_user_creation
        ));
        out.push_str(&format!(
            "limits.qps_token = {}\nlimits.qps_tenant = {}\nlimits.qps_space = {}\nlimits.max_queue = {}\n",
            self.limits.qps_token, self.limits.qps_tenant, self.limits.qps_space, self.limits.max_queue
        ));
        out.push_str(&format!(
            "retrieval.event_depth = {}\nretrieval.event_max_seeds = {}\nretrieval.event_max_events = {}\nretrieval.event_timeout_ms = {}\nretrieval.event_text_weight = {}\nretrieval.rrf_bm25_weight = {}\nretrieval.rrf_dense_weight = {}\nretrieval.rrf_event_weight = {}\n",
            self.retrieval.event_depth,
            self.retrieval.event_max_seeds,
            self.retrieval.event_max_events,
            self.retrieval.event_timeout_ms,
            self.retrieval.event_text_weight,
            self.retrieval.rrf_bm25_weight,
            self.retrieval.rrf_dense_weight,
            self.retrieval.rrf_event_weight
        ));
        out.push_str(&format!(
            "summary.enabled = {}\nsummary.auto = {}\nsummary.model_alias = {}\nsummary.prompt_version = {}\nsummary.max_chars = {}\nsummary.batch = {}\n",
            self.summary.enabled, self.summary.auto, self.summary.model_alias, self.summary.prompt_version, self.summary.max_chars, self.summary.batch
        ));
        let features = self.features();
        out.push_str(&format!(
            "deployment.mode = {}\ndeployment.commercial = {}\nfeatures.signup = {}\nfeatures.billing = {}\nfeatures.connectors = {}\nfeatures.sso = {}\nfeatures.multi_tenant = {}\nfeatures.public_api = {}\nfeatures.mcp = {}\nfeatures.audit = {}\nfeatures.rate_limit = {}\nfeatures.public_share = {}\ni18n.default_locale = {}\ni18n.supported_locales = {:?}\npayment.provider = {}\npayment.currency = {}\npayment.mode = {}\npayment.secret_env = {}\n",
            self.deployment_mode().as_str(),
            self.deployment.commercial,
            features.signup,
            features.billing,
            features.connectors,
            features.sso,
            features.multi_tenant,
            features.public_api,
            features.mcp,
            features.audit,
            features.rate_limit,
            features.public_share,
            self.i18n.default_locale,
            self.i18n.supported_locales,
            self.payment.provider,
            self.payment.currency,
            self.payment.mode,
            self.payment.secret_env.as_deref().unwrap_or("<none>")
        ));
        for (label, model) in [
            ("models.llm", &self.models.llm),
            ("models.embedding", &self.models.embedding),
            ("models.judge", &self.models.judge),
            ("models.extractor", &self.models.extractor),
        ] {
            out.push_str(&format!("{label}.provider = {}\n", model.provider));
            let base_url = model.base_url_or_default();
            out.push_str(&format!(
                "{label}.base_url = {}\n",
                mask_database_url(base_url.as_deref().unwrap_or("<none>"))
            ));
            out.push_str(&format!(
                "{label}.model = {}\n",
                model.model.as_deref().unwrap_or("<none>")
            ));
            out.push_str(&format!(
                "{label}.api_key_env = {} ({})\n",
                model.api_key_env.as_deref().unwrap_or("<none>"),
                model.api_key_state()
            ));
            out.push_str(&format!(
                "{label}.timeout_ms = {}\n{label}.max_retries = {}\n",
                model.timeout_ms, model.max_retries
            ));
        }
        out.push_str(&format!(
            "repository = {}\nissues = {}\nsecurity = {}\n",
            crate::util::REPOSITORY_URL,
            crate::util::ISSUES_URL,
            crate::util::SECURITY_URL
        ));
        out
    }
}

impl ModelConfig {
    fn validate(&self, label: &str) -> anyhow::Result<()> {
        if !self.enabled() {
            return Ok(());
        }
        if self.provider.trim().is_empty() {
            bail!("{label}.provider must not be empty");
        }
        let base = self.base_url_or_default();
        match base {
            Some(url) if url.starts_with("http://") || url.starts_with("https://") => {}
            _ => bail!("{label}.base_url must be an http(s) URL when provider is not \"none\""),
        }
        if self.model.as_deref().unwrap_or("").trim().is_empty() {
            bail!("{label}.model is required when provider is not \"none\"");
        }
        if self.timeout_ms == 0 {
            bail!("{label}.timeout_ms must be greater than zero");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solo_defaults_to_sqlite_and_loopback() {
        let config = Config::default();
        assert_eq!(config.profile_name(), ProfileName::Solo);
        assert_eq!(config.storage.backend, Backend::Sqlite);
        assert!(is_loopback_host(split_bind_host(&config.server.bind)));
    }

    #[test]
    fn non_loopback_none_auth_is_rejected() {
        let mut config = Config::default();
        config.server.bind = "0.0.0.0:7331".into();
        assert!(config.validate().is_err());
    }

    #[test]
    fn solo_uses_fixed_governance_ids() {
        let mut config = Config::default();
        config.storage.tenant_id = "something".into();
        config.normalize();
        assert_eq!(config.storage.tenant_id, crate::governance::SOLO_TENANT);
        assert_eq!(config.storage.default_space, crate::governance::SOLO_SPACE);
    }
}
