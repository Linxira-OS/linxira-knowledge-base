//! Minimal in-process metrics with a Prometheus text endpoint.
//!
//! Labels deliberately stay low-cardinality: no query, tenant or space labels.

use std::sync::atomic::{AtomicU64, Ordering};

static HTTP_TOTAL: AtomicU64 = AtomicU64::new(0);
static RATE_LIMITED: AtomicU64 = AtomicU64::new(0);
static QUEUE_FULL: AtomicU64 = AtomicU64::new(0);
static AUTH_FAILURES: AtomicU64 = AtomicU64::new(0);
static SERVER_ERRORS: AtomicU64 = AtomicU64::new(0);
static SLOW_QUERIES: AtomicU64 = AtomicU64::new(0);

pub fn http_request(status: u16) {
    HTTP_TOTAL.fetch_add(1, Ordering::Relaxed);
    if status >= 500 {
        SERVER_ERRORS.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn rate_limited() {
    RATE_LIMITED.fetch_add(1, Ordering::Relaxed);
}

pub fn queue_full() {
    QUEUE_FULL.fetch_add(1, Ordering::Relaxed);
}

pub fn auth_failure() {
    AUTH_FAILURES.fetch_add(1, Ordering::Relaxed);
}

pub fn slow_query() {
    SLOW_QUERIES.fetch_add(1, Ordering::Relaxed);
}

pub fn render() -> String {
    format!(
        "# TYPE lkb_http_requests_total counter\nlkb_http_requests_total {}\n\
         # TYPE lkb_http_5xx_total counter\nlkb_http_5xx_total {}\n\
         # TYPE lkb_rate_limited_total counter\nlkb_rate_limited_total {}\n\
         # TYPE lkb_queue_full_total counter\nlkb_queue_full_total {}\n\
         # TYPE lkb_auth_failures_total counter\nlkb_auth_failures_total {}\n\
         # TYPE lkb_slow_queries_total counter\nlkb_slow_queries_total {}\n\
         # TYPE lkb_inflight_requests gauge\nlkb_inflight_requests {}\n",
        HTTP_TOTAL.load(Ordering::Relaxed),
        SERVER_ERRORS.load(Ordering::Relaxed),
        RATE_LIMITED.load(Ordering::Relaxed),
        QUEUE_FULL.load(Ordering::Relaxed),
        AUTH_FAILURES.load(Ordering::Relaxed),
        SLOW_QUERIES.load(Ordering::Relaxed),
        crate::logging::concurrency(),
    )
}
