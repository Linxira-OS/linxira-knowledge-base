//! Structured logging setup.
//!
//! Without pulling the optional `tracing-subscriber/json` feature this module
//! implements a small JSON event formatter. Every access log line carries the
//! documented field set; missing values are `null` (or `[]` for
//! `degraded_codes`) so downstream log processors can rely on the schema.

use serde_json::{json, Map, Value};
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;
use std::time::Instant;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::EnvFilter;

const REQUIRED: &[&str] = &[
    "timestamp",
    "level",
    "service",
    "request_id",
    "trace_id",
    "actor_id",
    "actor_type",
    "token_id",
    "tenant",
    "space",
    "route",
    "method",
    "status",
    "latency_ms",
    "db_ms",
    "queue_ms",
    "model",
    "tokens_in",
    "tokens_out",
    "tokens_cached",
    "cache_hit",
    "degraded_codes",
    "error_code",
    "concurrency",
    "ip_hash",
    "user_agent",
];

static CONCURRENCY: AtomicUsize = AtomicUsize::new(0);
static SERVICE: OnceLock<String> = OnceLock::new();

pub fn concurrency_enter() -> usize {
    CONCURRENCY.fetch_add(1, Ordering::SeqCst) + 1
}

pub fn concurrency_leave() {
    CONCURRENCY.fetch_sub(1, Ordering::SeqCst);
}

pub fn concurrency() -> usize {
    CONCURRENCY.load(Ordering::SeqCst)
}

pub fn service_name() -> String {
    std::env::var("LKB_SERVICE")
        .ok()
        .or_else(|| SERVICE.get().cloned())
        .unwrap_or_else(|| "lkb".to_string())
}

pub fn init(service: &str) {
    let _ = SERVICE.set(service.to_string());
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| {
            std::env::var("LKB_LOG_LEVEL")
                .map(EnvFilter::new)
                .map_err(|_| ())
        })
        .unwrap_or_else(|_| EnvFilter::new("info"));
    let json = std::env::var("LKB_LOG_FORMAT")
        .map(|value| value.eq_ignore_ascii_case("json"))
        .unwrap_or(false);
    if json {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .with_writer(std::io::stderr)
            .event_format(JsonFormat)
            .try_init();
    } else {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .with_writer(std::io::stderr)
            .try_init();
    }
}

#[derive(Default)]
struct JsonVisitor {
    fields: Map<String, Value>,
}

impl Visit for JsonVisitor {
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields.insert(field.name().to_string(), json!(value));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields.insert(field.name().to_string(), json!(value));
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.fields.insert(field.name().to_string(), json!(value));
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields.insert(field.name().to_string(), json!(value));
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields.insert(field.name().to_string(), json!(value));
    }
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let name = field.name().to_string();
        let text = format!("{value:?}");
        if name == "degraded_codes" {
            if let Ok(parsed) = serde_json::from_str::<Value>(&text) {
                self.fields.insert(name, parsed);
                return;
            }
        }
        self.fields.insert(name, Value::String(text));
    }
}

pub struct JsonFormat;

impl<S, N> FormatEvent<S, N> for JsonFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let mut visitor = JsonVisitor::default();
        event.record(&mut visitor);
        let mut map = visitor.fields;
        map.insert("timestamp".to_string(), json!(crate::util::now_rfc3339()));
        map.insert(
            "level".to_string(),
            json!(event.metadata().level().as_str().to_ascii_lowercase()),
        );
        map.insert("service".to_string(), json!(service_name()));
        for key in REQUIRED {
            if !map.contains_key(*key) {
                let value = if *key == "degraded_codes" {
                    json!([])
                } else {
                    Value::Null
                };
                map.insert((*key).to_string(), value);
            }
        }
        let line = serde_json::to_string(&Value::Object(map)).unwrap_or_default();
        writeln!(writer, "{line}")
    }
}

/// Emit one access-log line for an HTTP request.
#[allow(clippy::too_many_arguments)]
pub fn access(
    request_id: &str,
    trace_id: &str,
    method: &str,
    route: &str,
    status: u16,
    latency_ms: u128,
    concurrency: usize,
    ip_hash: Option<&str>,
    user_agent: Option<&str>,
    query_hash: Option<&str>,
    query_length: Option<usize>,
    actor_id: Option<&str>,
    actor_type: Option<&str>,
    token_id: Option<&str>,
    tenant: Option<&str>,
    space: Option<&str>,
    error_code: Option<&str>,
    degraded: &[String],
) {
    tracing::info!(
        request_id = request_id,
        trace_id = trace_id,
        actor_id = actor_id.unwrap_or(""),
        actor_type = actor_type.unwrap_or("anonymous"),
        token_id = token_id.unwrap_or(""),
        tenant = tenant.unwrap_or(""),
        space = space.unwrap_or(""),
        route = route,
        method = method,
        status = status as u64,
        latency_ms = latency_ms as u64,
        db_ms = latency_ms as u64,
        queue_ms = 0u64,
        model = "",
        tokens_in = 0u64,
        tokens_out = 0u64,
        tokens_cached = 0u64,
        cache_hit = false,
        degraded_codes = ?degraded,
        error_code = error_code.unwrap_or(""),
        concurrency = concurrency as u64,
        ip_hash = ip_hash.unwrap_or(""),
        user_agent = user_agent.unwrap_or(""),
        query_hash = query_hash.unwrap_or(""),
        query_length = query_length.unwrap_or(0) as u64,
        "http.access"
    );
}

pub fn cli_event(action: &str, status: u16, error_code: Option<&str>, degraded: &[String]) {
    let request_id = format!("cli_{}", crate::util::random_hex(8));
    tracing::info!(
        request_id = request_id,
        trace_id = request_id,
        actor_id = "",
        actor_type = "cli",
        token_id = "",
        tenant = "",
        space = "",
        route = action,
        method = "CLI",
        status = status as u64,
        latency_ms = 0u64,
        db_ms = 0u64,
        queue_ms = 0u64,
        model = "",
        tokens_in = 0u64,
        tokens_out = 0u64,
        tokens_cached = 0u64,
        cache_hit = false,
        degraded_codes = ?degraded,
        error_code = error_code.unwrap_or(""),
        concurrency = 0u64,
        ip_hash = "",
        user_agent = "lkb-cli",
        "cli.command"
    );
}

pub struct Timer {
    start: Instant,
}

impl Timer {
    pub fn start() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    pub fn elapsed_ms(&self) -> u128 {
        self.start.elapsed().as_millis()
    }
}
