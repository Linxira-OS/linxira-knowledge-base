use crate::answer::{build_answer, build_answer_with_ledger, AnswerOptions};
use crate::config::{AuthMode, Config, Feature};
use crate::contract::{Answer, Degraded};
use crate::error::AppError;
use crate::governance::{
    Acl, GroupRole, Policy, Principal, PrincipalKind, PrincipalType, SpaceRole, TokenScope,
};
use crate::ingest::{attach_embeddings, chunker, doc_id, extract_and_store_events};
use crate::providers::ProviderSet;
use crate::search::{search_with_tuning, SearchMode, SearchResponse, SearchTuning};
use crate::store::{
    AuditFilter, ChunkRecord, ConversationRecord, DocumentRecord, GroupMemberRecord, GroupRecord,
    MessageRecord, ModelAliasRecord, PageRecord, ProviderModelRecord, ProviderRecord,
    QuotaOverrideRecord, QuotaRecord, QuotaScope, RevisionRecord, RouteRecord, SpaceMemberRecord,
    SpaceRecord, StorageUsage, Store, TokenQuotaRecord, UsageFilter,
};
use crate::util::{
    constant_time_eq, html_escape, now_rfc3339, random_hex, sha256_hex, sha256_hex_str,
    truncate_chars, url_encode_path_segment, ISSUES_URL, REPOSITORY_URL, SECURITY_URL,
};
use argon2::password_hash::{PasswordHash, PasswordVerifier};
use argon2::Argon2;
use axum::body::{Body, Bytes};
use axum::extract::{ConnectInfo, Form, Path as AxumPath, Query, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path as FsPath, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<dyn Store>,
    pub config: Arc<Config>,
    pub providers: Arc<ProviderSet>,
    pub limiter: Arc<LoginLimiter>,
    pub limits: Arc<crate::limits::Limits>,
}

impl AppState {
    pub fn new(store: Arc<dyn Store>, config: Arc<Config>, providers: Arc<ProviderSet>) -> Self {
        let limits = Arc::new(crate::limits::Limits::new(config.limits.clone()));
        Self {
            store,
            config,
            providers,
            limiter: Arc::new(LoginLimiter::default()),
            limits,
        }
    }
}

pub fn router(state: AppState) -> Router {
    let static_root = resolve_static_root(&state.config);
    let mut router = Router::new()
        .route("/health", get(health))
        .route("/openapi.json", get(openapi_json))
        .route("/login", get(login_form).post(login_submit))
        .route("/logout", post(logout))
        .route("/w/{space}/{slug}", get(page_view).post(page_save))
        .route("/w/{space}/{slug}/edit", get(page_edit))
        .route("/w/{space}/{slug}/history", get(page_history))
        .route("/api/search", get(api_search))
        .route("/api/ask", post(api_ask))
        .route("/api/v1/spaces", get(list_spaces_v1).post(create_space_v1))
        .route(
            "/api/v1/spaces/{space_id}",
            get(get_space_v1)
                .patch(patch_space_v1)
                .delete(delete_space_v1),
        )
        .route("/api/v1/spaces/{space_id}/archive", post(archive_space_v1))
        .route("/api/v1/spaces/{space_id}/restore", post(restore_space_v1))
        .route(
            "/api/v1/spaces/{space_id}/transfer",
            post(transfer_space_v1),
        )
        .route("/api/v1/spaces/{space_id}/usage", get(space_usage_v1))
        .route(
            "/api/v1/spaces/{space_id}/members",
            get(list_space_members_v1).post(add_space_member_v1),
        )
        .route(
            "/api/v1/spaces/{space_id}/members/{principal_type}/{principal_id}",
            axum::routing::patch(patch_space_member_v1).delete(remove_space_member_v1),
        )
        .route(
            "/api/v1/spaces/{space_id}/quota",
            axum::routing::put(set_space_quota_v1),
        )
        .route("/api/v1/groups", get(list_groups_v1).post(create_group_v1))
        .route(
            "/api/v1/groups/{group_id}",
            get(get_group_v1)
                .patch(patch_group_v1)
                .delete(delete_group_v1),
        )
        .route(
            "/api/v1/groups/{group_id}/members",
            get(list_group_members_v1).post(add_group_member_v1),
        )
        .route(
            "/api/v1/groups/{group_id}/members/{principal_id}",
            axum::routing::patch(patch_group_member_v1).delete(remove_group_member_v1),
        )
        .route("/metrics", get(metrics_handler))
        .route("/api/v1/features", get(features_v1))
        .route("/api/v1/billing/checkout", post(billing_checkout_v2))
        .route("/api/v1/billing/plans", get(billing_plans_v1))
        .route("/api/v1/billing/wallet", get(billing_wallet_v1))
        .route("/api/v1/billing/ledger", get(billing_ledger_v1))
        .route("/api/v1/billing/invoices", get(billing_invoices_v1))
        .route("/api/v1/billing/topup", post(billing_topup_v1))
        .route("/api/v1/billing/subscribe", post(billing_subscribe_v1))
        .route(
            "/api/v1/billing/subscription",
            get(billing_subscription_v1).patch(billing_subscription_patch_v1),
        )
        .route("/api/v1/billing/entitlement", get(billing_entitlement_v1))
        .route(
            "/api/v1/billing/webhook/{provider}",
            post(billing_webhook_v1),
        )
        .route("/api/v1/gateway/chat", post(gateway_chat_v1))
        .route("/api/v1/auth/sso/status", get(auth_sso_status_v1))
        .route("/api/v1/auth/csrf", get(auth_csrf_v1))
        .route("/api/v1/auth/login", post(auth_login_v1))
        .route("/api/v1/auth/me", get(auth_me_v1))
        .route("/api/v1/auth/logout", post(auth_logout_v1))
        .route("/api/v1/pages", get(pages_list_v1))
        .route(
            "/api/v1/pages/{space}/{slug}/revisions/{revision_id}",
            get(page_revision_v1),
        )
        .route("/api/v1/search", get(search_v1))
        .route("/api/v1/answer", post(answer_v1))
        .route("/api/v1/answer/stream", get(answer_stream_v1))
        .route(
            "/api/v1/pages/{space}/{slug}",
            get(page_json_v1)
                .put(page_save_json_v1)
                .post(page_save_json_v1)
                .delete(page_delete_json_v1),
        )
        .route("/api/v1/pages/{space}/{slug}/history", get(page_history_v1))
        .route(
            "/api/v1/pages/{space}/{slug}/restore",
            post(page_restore_json_v1),
        )
        .route("/api/v1/pages/{space}/{slug}/summary", get(page_summary_v1))
        .route("/api/v1/query", post(query_v1))
        .route("/api/v1/query/stream", post(query_stream_v1))
        .route(
            "/api/v1/conversations",
            get(list_conversations_v1).post(create_conversation_v1),
        )
        .route(
            "/api/v1/conversations/{conversation_id}",
            get(get_conversation_v1).delete(delete_conversation_v1),
        )
        .route(
            "/api/v1/conversations/{conversation_id}/external-session",
            post(bind_external_session_v1),
        )
        .route(
            "/api/v1/conversations/{conversation_id}/restore",
            post(restore_conversation_v1),
        )
        .route(
            "/api/v1/admin/providers",
            get(admin_providers_v1).post(admin_provider_create_v1),
        )
        .route(
            "/api/v1/admin/providers/{provider_id}",
            axum::routing::patch(admin_provider_patch_v1).delete(admin_provider_delete_v1),
        )
        .route(
            "/api/v1/admin/provider-models",
            get(admin_provider_models_v1).post(admin_provider_model_create_v1),
        )
        .route(
            "/api/v1/admin/aliases",
            get(admin_aliases_v1).post(admin_alias_create_v1),
        )
        .route(
            "/api/v1/admin/aliases/{public_id}",
            axum::routing::patch(admin_alias_patch_v1).delete(admin_alias_delete_v1),
        )
        .route(
            "/api/v1/admin/routes",
            get(admin_routes_v1).post(admin_route_create_v1),
        )
        .route(
            "/api/v1/admin/routes/{route_id}",
            axum::routing::patch(admin_route_patch_v1).delete(admin_route_delete_v1),
        )
        .route("/api/v1/admin/usage", get(admin_usage_v1))
        .route(
            "/api/v1/admin/quotas",
            get(admin_quotas_v1).post(admin_quota_create_v1),
        )
        .route(
            "/api/v1/admin/quotas/{quota_id}",
            axum::routing::delete(admin_quota_delete_v1),
        )
        .route("/api/v1/admin/users", get(admin_users_v1))
        .route(
            "/api/v1/admin/users/{user_id}",
            axum::routing::patch(admin_user_patch_v1),
        )
        .route(
            "/api/v1/admin/users/{user_id}/disable",
            post(admin_user_disable_v1),
        )
        .route("/api/v1/admin/roles", get(admin_roles_v1))
        .route("/api/v1/admin/groups", get(admin_groups_v1))
        .route("/api/v1/admin/spaces", get(admin_spaces_v1))
        .route(
            "/api/v1/admin/usage/aggregate",
            get(admin_usage_aggregate_v1),
        )
        .route(
            "/api/v1/connectors",
            get(connectors_v1).post(connectors_create_v1),
        )
        .route(
            "/api/v1/connectors/{connector_id}",
            axum::routing::patch(connectors_patch_v1).delete(connectors_delete_v1),
        )
        .route(
            "/api/v1/connectors/{connector_id}/test",
            post(connectors_test_v1),
        )
        .route(
            "/api/v1/connectors/{connector_id}/sync",
            post(connectors_sync_v1),
        )
        .route(
            "/api/v1/connectors/{connector_id}/jobs",
            get(connectors_jobs_v1),
        )
        .route("/api/v1/external-refs", get(external_refs_v1))
        .route("/api/v1/admin/audit", get(list_audit_v1))
        .route(
            "/api/v1/admin/quota-overrides",
            post(create_quota_override_v1),
        );

    // With a web directory configured, "/" is the SPA entry point and every
    // route the API does not claim falls through to static files; without one
    // the built-in HTML index keeps working.
    if let Some(dir) = static_root {
        let dir = Arc::new(dir);
        router = router.fallback(move |request: Request| {
            let dir = dir.clone();
            async move { serve_static(&dir, request).await }
        });
    } else {
        router = router.route("/", get(root));
    }

    router
        .layer(middleware::from_fn_with_state(
            state.clone(),
            request_logging,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            sso_middleware,
        ))
        .with_state(state)
}

pub async fn serve(state: AppState, bind: &str) -> anyhow::Result<()> {
    let address: SocketAddr = match bind.parse() {
        Ok(address) => address,
        Err(_) => tokio::net::lookup_host(bind)
            .await?
            .next()
            .ok_or_else(|| anyhow::anyhow!("cannot resolve bind address {bind:?}"))?,
    };
    for warning in state.config.sso_warnings() {
        tracing::warn!("{warning}");
    }
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, profile = state.config.profile_name().as_str(), "lkb listening");
    axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

#[derive(Default)]
pub struct LoginLimiter {
    failures: Mutex<HashMap<String, Vec<Instant>>>,
}

impl LoginLimiter {
    const WINDOW: Duration = Duration::from_secs(300);
    const MAX_FAILURES: usize = 5;

    pub fn allowed(&self, key: &str) -> bool {
        let mut guard = self.failures.lock().expect("login limiter poisoned");
        let now = Instant::now();
        let entries = guard.entry(key.to_string()).or_default();
        entries.retain(|instant| now.duration_since(*instant) < Self::WINDOW);
        entries.len() < Self::MAX_FAILURES
    }

    pub fn record_failure(&self, key: &str) {
        let mut guard = self.failures.lock().expect("login limiter poisoned");
        let now = Instant::now();
        let entries = guard.entry(key.to_string()).or_default();
        entries.retain(|instant| now.duration_since(*instant) < Self::WINDOW);
        entries.push(now);
    }

    pub fn clear(&self, key: &str) {
        let mut guard = self.failures.lock().expect("login limiter poisoned");
        guard.remove(key);
    }
}
/// Canonical web root for `server.web_dir` / `LKB_WEB_DIR`, if it is usable.
/// A missing directory only disables static hosting (the API still starts), so
/// a stale path cannot take the whole server down.
fn resolve_static_root(config: &Config) -> Option<PathBuf> {
    let dir = config.server.web_dir.as_deref()?;
    match std::fs::canonicalize(dir) {
        Ok(root) if root.is_dir() => Some(root),
        Ok(root) => {
            tracing::warn!(
                path = %root.display(),
                "server.web_dir is not a directory; static hosting disabled"
            );
            None
        }
        Err(err) => {
            tracing::warn!(
                path = %dir.display(),
                error = %err,
                "server.web_dir cannot be read; static hosting disabled"
            );
            None
        }
    }
}

/// Fallback handler: anything the router did not claim is served from the web
/// directory. Files are served as-is (content-hashed `assets/` get long cache
/// headers, `index.html` is always revalidated), extension-less paths fall
/// back to `index.html` so client-side history routing works, and a missing
/// asset stays a 404 instead of silently turning into HTML.
async fn serve_static(root: &FsPath, request: Request) -> Response {
    let head_only = request.method() == Method::HEAD;
    let response = match *request.method() {
        Method::GET | Method::HEAD => static_response(root, request).await,
        _ => {
            let mut response = AppError::new(
                StatusCode::METHOD_NOT_ALLOWED,
                "method_not_allowed",
                "static files are served with GET and HEAD",
            )
            .into_response();
            response
                .headers_mut()
                .insert(header::ALLOW, HeaderValue::from_static("GET, HEAD"));
            response
        }
    };
    if !head_only {
        return response;
    }
    // `Router::fallback` routes are called for HEAD as-is, so trim the body
    // here and keep the headers (including Content-Length where known).
    let (parts, _) = response.into_parts();
    Response::from_parts(parts, Body::empty())
}

async fn static_response(root: &FsPath, request: Request) -> Response {
    let raw_path = request.uri().path();
    if is_api_path(raw_path) {
        // A missing API route must never be answered with the SPA shell.
        return AppError::not_found("api route not found").into_response();
    }
    let Some(relative) = safe_relative_path(raw_path) else {
        return AppError::not_found("file not found").into_response();
    };
    if relative.as_os_str().is_empty() {
        return index_response(root).await;
    }
    match resolve_static_file(root, &relative).await {
        Some(file) => file_response(&file, &relative).await,
        None if relative.extension().is_none() => index_response(root).await,
        None => AppError::not_found("file not found").into_response(),
    }
}

fn is_api_path(path: &str) -> bool {
    path == "/api" || path.starts_with("/api/")
}

async fn index_response(root: &FsPath) -> Response {
    let index = FsPath::new("index.html");
    match resolve_static_file(root, index).await {
        Some(file) => file_response(&file, index).await,
        None => AppError::not_found("index.html is missing from the web directory").into_response(),
    }
}

/// Canonicalize `root/relative` and keep it only when it stays inside `root`,
/// so traversal attempts and symlinks pointing outside the web directory are
/// rejected even if a segment check misses them.
async fn resolve_static_file(root: &FsPath, relative: &FsPath) -> Option<PathBuf> {
    let canonical = tokio::fs::canonicalize(root.join(relative)).await.ok()?;
    if !canonical.starts_with(root) {
        return None;
    }
    let metadata = tokio::fs::metadata(&canonical).await.ok()?;
    if !metadata.is_file() {
        return None;
    }
    Some(canonical)
}

async fn file_response(path: &FsPath, relative: &FsPath) -> Response {
    let metadata = match tokio::fs::metadata(path).await {
        Ok(metadata) if metadata.is_file() => metadata,
        _ => return AppError::not_found("file not found").into_response(),
    };
    let file = match tokio::fs::File::open(path).await {
        Ok(file) => file,
        Err(_) => return AppError::not_found("file not found").into_response(),
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type(relative))
        .header(header::CONTENT_LENGTH, metadata.len().to_string())
        .header(header::CACHE_CONTROL, cache_control(relative))
        .header(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        )
        .body(file_body(file))
        .expect("static file response")
}

/// Stream the file instead of buffering it, so large assets stay off the heap.
fn file_body(file: tokio::fs::File) -> Body {
    let stream = futures_util::stream::try_unfold(file, |mut file| async move {
        let mut chunk = vec![0u8; 64 * 1024];
        let read = file.read(&mut chunk).await?;
        if read == 0 {
            return Ok::<_, std::io::Error>(None);
        }
        chunk.truncate(read);
        Ok(Some((Bytes::from(chunk), file)))
    });
    Body::from_stream(stream)
}

/// Decode a request path into a relative path, rejecting anything that could
/// escape the web directory (`..`, backslashes, drive letters, NUL) or hide a
/// traversal behind percent encoding.
fn safe_relative_path(raw: &str) -> Option<PathBuf> {
    let decoded = percent_decode(raw)?;
    if decoded.contains('\0') {
        return None;
    }
    let mut relative = PathBuf::new();
    for segment in decoded.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return None;
        }
        if segment.contains('\\') || segment.contains(':') {
            return None;
        }
        if cfg!(windows) && (segment.ends_with(' ') || segment.ends_with('.')) {
            // Win32 strips trailing dots and spaces, which would alias other paths.
            return None;
        }
        relative.push(segment);
    }
    Some(relative)
}

/// Decode `%XX` escapes; returns `None` for invalid UTF-8. `+` stays literal
/// (that is query-string syntax, not path syntax).
fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 3 <= bytes.len() {
            let high = (bytes[index + 1] as char).to_digit(16);
            let low = (bytes[index + 2] as char).to_digit(16);
            if let (Some(high), Some(low)) = (high, low) {
                out.push((high * 16 + low) as u8);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(out).ok()
}

/// Content types for the files a web dist actually ships; anything else stays
/// `application/octet-stream` rather than guessing.
fn content_type(path: &FsPath) -> &'static str {
    const TYPES: &[(&str, &str)] = &[
        ("html", "text/html; charset=utf-8"),
        ("htm", "text/html; charset=utf-8"),
        ("js", "text/javascript; charset=utf-8"),
        ("mjs", "text/javascript; charset=utf-8"),
        ("css", "text/css; charset=utf-8"),
        ("json", "application/json; charset=utf-8"),
        ("map", "application/json; charset=utf-8"),
        ("svg", "image/svg+xml"),
        ("png", "image/png"),
        ("jpg", "image/jpeg"),
        ("jpeg", "image/jpeg"),
        ("gif", "image/gif"),
        ("webp", "image/webp"),
        ("avif", "image/avif"),
        ("ico", "image/x-icon"),
        ("woff", "font/woff"),
        ("woff2", "font/woff2"),
        ("ttf", "font/ttf"),
        ("otf", "font/otf"),
        ("wasm", "application/wasm"),
        ("txt", "text/plain; charset=utf-8"),
        ("xml", "application/xml; charset=utf-8"),
        ("webmanifest", "application/manifest+json"),
        ("pdf", "application/pdf"),
        ("mp4", "video/mp4"),
        ("webm", "video/webm"),
        ("mp3", "audio/mpeg"),
    ];
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    for (candidate, mime) in TYPES {
        if extension.eq_ignore_ascii_case(candidate) {
            return mime;
        }
    }
    "application/octet-stream"
}

fn cache_control(relative: &FsPath) -> &'static str {
    // Vite emits content-hashed bundles under `assets/`; those names can never
    // change meaning, everything else (`index.html` included) revalidates.
    match relative.components().next() {
        Some(std::path::Component::Normal(first)) if first.eq_ignore_ascii_case("assets") => {
            "public, max-age=31536000, immutable"
        }
        _ => "no-cache",
    }
}


fn cookie_map(headers: &HeaderMap) -> HashMap<String, String> {
    let mut cookies = HashMap::new();
    for value in headers.get_all(header::COOKIE).iter() {
        if let Ok(value) = value.to_str() {
            for part in value.split(';') {
                if let Some((name, value)) = part.split_once('=') {
                    cookies.insert(name.trim().to_string(), value.trim().to_string());
                }
            }
        }
    }
    cookies
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    cookie_map(headers).get(name).cloned()
}

fn host_header(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
}

fn build_cookie(
    name: &str,
    value: &str,
    max_age_seconds: i64,
    http_only: bool,
    secure: bool,
) -> String {
    let mut cookie = format!("{name}={value}; Path=/; SameSite=Lax; Max-Age={max_age_seconds}");
    if http_only {
        cookie.push_str("; HttpOnly");
    }
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

fn append_set_cookie(response: &mut Response, cookie: String) {
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
}

fn csrf_token_for(state: &AppState, headers: &HeaderMap) -> (String, Option<String>) {
    if let Some(existing) = cookie_value(headers, "lkb_csrf") {
        if !existing.is_empty() {
            return (existing, None);
        }
    }
    let token = random_hex(32);
    let secure = state.config.cookie_secure_for_host(host_header(headers));
    let cookie = build_cookie("lkb_csrf", &token, 24 * 3600, false, secure);
    (token, Some(cookie))
}

fn check_origin(headers: &HeaderMap) -> Result<(), AppError> {
    let origin = match headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    {
        Some(value) => value,
        None => return Ok(()),
    };
    if origin == "null" {
        return Err(AppError::forbidden("cross-origin request rejected"));
    }
    let authority = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"))
        .unwrap_or(origin)
        .split('/')
        .next()
        .unwrap_or("");
    let host = host_header(headers).unwrap_or("");
    if authority.is_empty() || host.is_empty() || !authority.eq_ignore_ascii_case(host) {
        return Err(AppError::forbidden("origin does not match host"));
    }
    Ok(())
}

fn verify_csrf(
    state: &AppState,
    headers: &HeaderMap,
    form_token: Option<&str>,
) -> Result<(), AppError> {
    check_origin(headers)?;
    if !state.config.auth.csrf {
        return Ok(());
    }
    let cookie = cookie_value(headers, "lkb_csrf")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::forbidden("missing CSRF cookie"))?;
    let form = form_token
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::forbidden("missing CSRF form token"))?;
    if !constant_time_eq(cookie.as_bytes(), form.as_bytes()) {
        return Err(AppError::forbidden("CSRF token mismatch"));
    }
    Ok(())
}

fn session_hash(headers: &HeaderMap, state: &AppState) -> Option<String> {
    cookie_value(headers, &state.config.auth.cookie_name).map(|value| sha256_hex_str(&value))
}

async fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<Principal, AppError> {
    // Set only by the trusted-header SSO middleware after it stripped any
    // client-supplied copy of this header.
    if let Some(user_id) = header_value(headers, "x-lkb-sso-authenticated") {
        let user = state
            .store
            .get_user_by_id(&state.config.storage.tenant_id, &user_id)
            .await?
            .ok_or_else(|| AppError::unauthorized("sso user no longer exists"))?;
        if user.disabled {
            return Err(AppError::unauthorized("user is disabled"));
        }
        let mut principal = Principal {
            principal_id: user.user_id,
            display_name: user.display_name,
            tenant_id: state.config.storage.tenant_id.clone(),
            allowed_space_ids: Vec::new(),
            groups: Vec::new(),
            allow_all: false,
            kind: crate::governance::PrincipalKind::Human,
            scope: if user.role.is_admin() {
                TokenScope::Admin
            } else {
                TokenScope::Write
            },
            tenant_role: user.role,
            token_id: None,
        };
        principal.allowed_space_ids = crate::access::allowed_space_ids(&state.store, &principal)
            .await
            .map_err(AppError::from)?;
        return Ok(principal);
    }
    if let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    {
        let token = value
            .strip_prefix("Bearer ")
            .ok_or_else(|| AppError::unauthorized("unsupported authorization scheme"))?
            .trim();
        if token.is_empty() {
            return Err(AppError::unauthorized("empty bearer token"));
        }
        let token_hash = sha256_hex_str(token);
        let record = state
            .store
            .get_token(&token_hash)
            .await?
            .ok_or_else(|| AppError::unauthorized("unknown token"))?;
        let now = now_rfc3339();
        if !record.is_active(&now) {
            return Err(AppError::unauthorized("token is expired or revoked"));
        }
        if record.tenant_id != state.config.storage.tenant_id {
            return Err(AppError::unauthorized("token belongs to another tenant"));
        }
        let user = state
            .store
            .get_user_by_id(&record.tenant_id, &record.user_id)
            .await?
            .ok_or_else(|| AppError::unauthorized("token user no longer exists"))?;
        if user.disabled {
            return Err(AppError::unauthorized("user is disabled"));
        }
        let mut principal = Principal {
            principal_id: user.user_id,
            display_name: user.display_name,
            tenant_id: record.tenant_id,
            allowed_space_ids: Vec::new(),
            groups: Vec::new(),
            allow_all: false,
            kind: record.kind,
            scope: record.scope,
            tenant_role: user.role,
            token_id: Some(record.token_id.clone()),
        };
        principal.allowed_space_ids = crate::access::allowed_space_ids(&state.store, &principal)
            .await
            .map_err(AppError::from)?;
        return Ok(principal);
    }

    if state.config.auth.mode == AuthMode::None {
        let mut principal = Principal::solo();
        principal.tenant_id = state.config.storage.tenant_id.clone();
        principal.allowed_space_ids = crate::access::allowed_space_ids(&state.store, &principal)
            .await
            .map_err(AppError::from)?;
        return Ok(principal);
    }

    let hash = session_hash(headers, state)
        .ok_or_else(|| AppError::unauthorized("authentication required"))?;
    let session = state
        .store
        .get_session(&hash)
        .await?
        .ok_or_else(|| AppError::unauthorized("session not found"))?;
    let now = now_rfc3339();
    if session.expires_at.as_str() < now.as_str() {
        let _ = state.store.delete_session(&hash).await;
        return Err(AppError::unauthorized("session expired"));
    }
    if session.tenant_id != state.config.storage.tenant_id {
        return Err(AppError::unauthorized("session belongs to another tenant"));
    }
    let user = state
        .store
        .get_user_by_id(&session.tenant_id, &session.user_id)
        .await?
        .ok_or_else(|| AppError::unauthorized("session user no longer exists"))?;
    if user.disabled {
        return Err(AppError::unauthorized("user is disabled"));
    }
    let mut principal = Principal {
        principal_id: user.user_id,
        display_name: user.display_name,
        tenant_id: session.tenant_id,
        allowed_space_ids: Vec::new(),
        groups: Vec::new(),
        allow_all: false,
        kind: crate::governance::PrincipalKind::Human,
        scope: if user.role.is_admin() {
            TokenScope::Admin
        } else {
            TokenScope::Write
        },
        tenant_role: user.role,
        token_id: None,
    };
    principal.allowed_space_ids = crate::access::allowed_space_ids(&state.store, &principal)
        .await
        .map_err(AppError::from)?;
    Ok(principal)
}

fn client_ip(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|value| value.to_str().ok())
                .map(|value| value.to_string())
        })
}

fn user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.chars().take(300).collect())
}

#[derive(Debug, Clone, Copy)]
struct V1Auth {
    session: bool,
}

async fn authenticate_v1(
    state: &AppState,
    headers: &HeaderMap,
    action: &str,
) -> Result<(Principal, V1Auth), AppError> {
    let via_bearer = headers.contains_key(header::AUTHORIZATION);
    match authenticate(state, headers).await {
        Ok(principal) => Ok((
            principal,
            V1Auth {
                session: state.config.auth.mode != AuthMode::None && !via_bearer,
            },
        )),
        Err(err) => {
            crate::audit::record(
                &state.store,
                &state.config.storage.tenant_id,
                None,
                "anonymous",
                "anonymous",
                action,
                "api",
                action,
                "denied",
                err.status().as_u16(),
                None,
                client_ip(headers).as_deref(),
                user_agent(headers).as_deref(),
                json!({"reason": err.code()}),
            )
            .await;
            Err(err)
        }
    }
}

fn require_scope(principal: &Principal, scope: TokenScope, action: &str) -> Result<(), AppError> {
    let allowed = match scope {
        TokenScope::Read => principal.scope.allows_read(),
        TokenScope::Write => principal.scope.allows_write(),
        TokenScope::Admin => principal.scope.allows_admin(),
    };
    if allowed {
        Ok(())
    } else {
        Err(AppError::forbidden(format!(
            "scope {:?} required for {action}",
            scope
        )))
    }
}

async fn require_space_role(
    state: &AppState,
    principal: &Principal,
    space_id: &str,
    minimum: SpaceRole,
    action: &str,
    headers: &HeaderMap,
) -> Result<crate::access::SpaceAccess, AppError> {
    let space = state
        .store
        .get_space(&principal.tenant_id, space_id)
        .await?
        .ok_or_else(|| AppError::not_found("space not found"))?;
    if space.status == "deleted" {
        return Err(AppError::not_found("space not found"));
    }
    let access = crate::access::effective_space_role(&state.store, principal, &space)
        .await
        .map_err(AppError::from)?;
    match access {
        Some(access) if access.role >= minimum => Ok(access),
        Some(_) | None => {
            crate::audit::record(
                &state.store,
                &principal.tenant_id,
                Some(space_id),
                &principal.principal_id,
                match principal.kind {
                    crate::governance::PrincipalKind::Human => "user",
                    _ => "agent",
                },
                action,
                "space",
                space_id,
                "denied",
                403,
                None,
                client_ip(headers).as_deref(),
                user_agent(headers).as_deref(),
                json!({"required": minimum.as_str()}),
            )
            .await;
            Err(AppError::forbidden("space role insufficient"))
        }
    }
}

async fn require_active_space(space: &SpaceRecord) -> Result<(), AppError> {
    if space.status == "active" {
        Ok(())
    } else {
        Err(AppError::new(
            StatusCode::CONFLICT,
            "space_not_active",
            "space is archived or deleted",
        ))
    }
}

fn policy_for(principal: Principal) -> Result<Policy, AppError> {
    Policy::new(principal, None).map_err(AppError::bad_request)
}

fn require_write(principal: &Principal) -> Result<(), AppError> {
    if principal.can_write() {
        Ok(())
    } else {
        Err(AppError::forbidden("write scope required"))
    }
}

fn lang_arg(lang: Option<String>) -> Option<String> {
    lang.map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
}

fn render_markdown(markdown: &str) -> String {
    let parser = pulldown_cmark::Parser::new_ext(markdown, pulldown_cmark::Options::all());
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, parser);
    ammonia::clean(&html)
}

fn html_page(title: &str, body: &str, principal: Option<&Principal>, csrf: Option<&str>) -> String {
    let nav = match (principal, csrf) {
        (Some(principal), Some(csrf)) => format!(
            "<nav><span>{}</span> <a href=\"/\">首页</a> \
             <form method=\"post\" action=\"/logout\" class=\"inline\">\
             <input type=\"hidden\" name=\"csrf_token\" value=\"{}\">\
             <button type=\"submit\">登出</button></form></nav>",
            html_escape(&principal.display_name),
            html_escape(csrf)
        ),
        (Some(principal), None) => format!(
            "<nav><span>{}</span> <a href=\"/\">首页</a></nav>",
            html_escape(&principal.display_name)
        ),
        _ => String::new(),
    };
    // Static canonical links only: nothing user-supplied is interpolated, and
    // the opened pages get no window handle back (`rel="noopener"`).
    let footer = format!(
        "<footer class=\"repo-links\">\
         <a href=\"{REPOSITORY_URL}\" target=\"_blank\" rel=\"noopener\">GitHub</a> · \
         <a href=\"{ISSUES_URL}\" target=\"_blank\" rel=\"noopener\">Issues</a> · \
         <a href=\"{SECURITY_URL}\" target=\"_blank\" rel=\"noopener\">Security</a>\
         </footer>"
    );
    format!(
        "<!doctype html><html lang=\"zh\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>{}</title></head><body>{nav}<main>{body}</main>{footer}</body></html>",
        html_escape(title)
    )
}

async fn root(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, AppError> {
    let principal = authenticate(&state, &headers).await?;
    let _limits = crate::limits::enforce(
        &state.store,
        &state.limits,
        &principal,
        None,
        "page.list",
        &headers,
    )
    .await?;
    audit_admin_content_read(&state, &principal, &headers, "content.read.break_glass").await;
    let policy = policy_for(principal.clone())?;
    let pages = state.store.list_pages(&policy, 1000).await?;
    let mut body = String::new();
    if state.config.auth.mode == AuthMode::None {
        body.push_str("<p class=\"muted\">solo 模式：本地 allow-all 策略，只监听回环地址。</p>");
    }
    if pages.is_empty() {
        let hint = format!(
            "<p>还没有 Wiki 页面。导入的文档可用 <code>lkb search</code> / <code>lkb ask</code> 查询；\
             新建页面可直接打开 <code>/w/{}/{}/edit?lang=zh</code>。</p>",
            url_encode_path_segment(&state.config.storage.default_space),
            "new-page"
        );
        body.push_str(&hint);
    } else {
        body.push_str("<table><thead><tr><th>空间</th><th>语言</th><th>页面</th><th>版本</th><th>更新时间</th></tr></thead><tbody>");
        for page in pages {
            body.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td><a href=\"/w/{}/{}?lang={}\">{}</a></td><td>{}</td><td>{}</td></tr>",
                html_escape(&page.space_id),
                html_escape(&page.lang),
                url_encode_path_segment(&page.space_id),
                url_encode_path_segment(&page.slug),
                url_encode_path_segment(&page.lang),
                html_escape(&page.title),
                page.version,
                html_escape(&page.updated_at)
            ));
        }
        body.push_str("</tbody></table>");
    }
    let (csrf, cookie) = csrf_token_for(&state, &headers);
    let mut response = Html(html_page(
        "知识库",
        &body,
        Some(&principal),
        Some(csrf.as_str()),
    ))
    .into_response();
    if let Some(cookie) = cookie {
        append_set_cookie(&mut response, cookie);
    }
    Ok(response)
}

async fn health(State(state): State<AppState>) -> Response {
    match state.store.health().await {
        Ok(()) => {
            let seed = state
                .store
                .get_seed_meta(&state.config.storage.tenant_id, &state.config.seed.space)
                .await
                .ok()
                .flatten();
            (
                StatusCode::OK,
                Json(json!({
                    "status": "ok",
                    "profile": state.config.profile_name().as_str(),
                    "backend": state.store.backend(),
                    "fts": state.store.fts_available(),
                    "seed": seed,
                })),
            )
                .into_response()
        }
        Err(err) => {
            tracing::error!(error = %err, "health check failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"status": "unavailable", "error": "storage_unavailable"})),
            )
                .into_response()
        }
    }
}

async fn authenticate_api(state: &AppState, headers: &HeaderMap) -> Result<Principal, AppError> {
    let outcome = if headers.contains_key(header::AUTHORIZATION)
        || state.config.auth.mode == AuthMode::None
    {
        authenticate(state, headers).await
    } else {
        Err(AppError::unauthorized(
            "bearer token required for API access",
        ))
    };
    if let Err(err) = &outcome {
        crate::audit::record(
            &state.store,
            &state.config.storage.tenant_id,
            None,
            "anonymous",
            "anonymous",
            "api.auth",
            "api",
            "api.auth",
            "denied",
            err.status().as_u16(),
            None,
            client_ip(headers).as_deref(),
            user_agent(headers).as_deref(),
            json!({"reason": err.code()}),
        )
        .await;
    }
    outcome
}

#[derive(Deserialize)]
struct ApiSearchQuery {
    q: String,
    mode: Option<String>,
    k: Option<usize>,
    lang: Option<String>,
}

async fn api_search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ApiSearchQuery>,
) -> Result<Json<SearchResponse>, AppError> {
    let principal = authenticate_api(&state, &headers).await?;
    let k = query.k.unwrap_or(10);
    state.limits.check_query(&query.q, k)?;
    let _limits = crate::limits::enforce(
        &state.store,
        &state.limits,
        &principal,
        None,
        "search",
        &headers,
    )
    .await?;
    audit_admin_content_read(&state, &principal, &headers, "content.read.break_glass").await;
    let policy = policy_for(principal)?;
    let mode_str = query
        .mode
        .clone()
        .unwrap_or_else(|| state.config.retrieval.mode.clone());
    let mode = SearchMode::parse(&mode_str).map_err(AppError::bad_request)?;
    let mut tuning = SearchTuning::from_retrieval(&state.config.retrieval);
    tuning.lang = lang_arg(query.lang.clone());
    tuning.billing_local_ledger = state.config.billing.engine.eq_ignore_ascii_case("local");
    let response = search_with_tuning(
        &state.store,
        &state.providers,
        &policy,
        &query.q,
        mode,
        query.k.unwrap_or(10),
        &tuning,
    )
    .await?;
    Ok(Json(response))
}

#[derive(Deserialize)]
struct ApiAsk {
    query: String,
    mode: Option<String>,
    limit: Option<usize>,
    lang: Option<String>,
}

async fn api_ask(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ApiAsk>,
) -> Result<Json<Answer>, AppError> {
    let principal = authenticate_api(&state, &headers).await?;
    let limit = request.limit.unwrap_or(8);
    state.limits.check_query(&request.query, limit)?;
    let _limits = crate::limits::enforce(
        &state.store,
        &state.limits,
        &principal,
        None,
        "ask",
        &headers,
    )
    .await?;
    audit_admin_content_read(&state, &principal, &headers, "content.read.break_glass").await;
    let policy = policy_for(principal)?;
    let mode_str = request
        .mode
        .clone()
        .unwrap_or_else(|| state.config.retrieval.mode.clone());
    let mode = SearchMode::parse(&mode_str).map_err(AppError::bad_request)?;
    let mut tuning = SearchTuning::from_retrieval(&state.config.retrieval);
    tuning.lang = lang_arg(request.lang.clone());
    tuning.billing_local_ledger = state.config.billing.engine.eq_ignore_ascii_case("local");
    let answer = build_answer(
        &state.store,
        &state.providers,
        &policy,
        &request.query,
        &AnswerOptions {
            mode,
            limit: request.limit.unwrap_or(8),
            tuning,
        },
    )
    .await?;
    Ok(Json(answer))
}

#[derive(Deserialize)]
struct LangQuery {
    lang: Option<String>,
}

#[derive(Deserialize)]
struct LoginForm {
    username: String,
    password: String,
    csrf_token: Option<String>,
}

fn login_rate_key(headers: &HeaderMap, username: &str) -> String {
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|value| value.to_str().ok())
        })
        .unwrap_or("local");
    format!("{ip}|{username}")
}

async fn login_form(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let (csrf, cookie) = csrf_token_for(&state, &headers);
    let body = format!(
        "<form method=\"post\" action=\"/login\">\
<input type=\"hidden\" name=\"csrf_token\" value=\"{}\">\
<p><label>用户名<br><input name=\"username\" autocomplete=\"username\" required></label></p>\
<p><label>密码<br><input name=\"password\" type=\"password\" autocomplete=\"current-password\" required></label></p>\
<p><button type=\"submit\">登录</button></p></form>",
        html_escape(&csrf)
    );
    let mut response = Html(html_page("登录", &body, None, None)).into_response();
    if let Some(cookie) = cookie {
        append_set_cookie(&mut response, cookie);
    }
    response
}

async fn login_submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<LoginForm>,
) -> Result<Response, AppError> {
    verify_csrf(&state, &headers, form.csrf_token.as_deref())?;
    if state.config.auth.mode == AuthMode::None {
        return Ok(Redirect::to("/").into_response());
    }
    let key = login_rate_key(&headers, &form.username);
    if !state.limiter.allowed(&key) {
        crate::audit::record(
            &state.store,
            &state.config.storage.tenant_id,
            None,
            "anonymous",
            "anonymous",
            "auth.login",
            "user",
            &form.username,
            "denied",
            429,
            None,
            client_ip(&headers).as_deref(),
            user_agent(&headers).as_deref(),
            json!({"reason": "rate_limited"}),
        )
        .await;
        return Err(AppError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "too many failed login attempts",
        ));
    }
    let user = state
        .store
        .get_user(&state.config.storage.tenant_id, &form.username)
        .await?;
    let user = match user {
        Some(user) if !user.disabled => user,
        _ => {
            state.limiter.record_failure(&key);
            crate::audit::record(
                &state.store,
                &state.config.storage.tenant_id,
                None,
                "anonymous",
                "anonymous",
                "auth.login",
                "user",
                &form.username,
                "denied",
                401,
                None,
                client_ip(&headers).as_deref(),
                user_agent(&headers).as_deref(),
                json!({"reason": "invalid_credentials"}),
            )
            .await;
            return Err(AppError::unauthorized("invalid username or password"));
        }
    };
    let parsed = PasswordHash::new(&user.password_hash)
        .map_err(|_| AppError::internal("stored password hash is invalid"))?;
    if Argon2::default()
        .verify_password(form.password.as_bytes(), &parsed)
        .is_err()
    {
        state.limiter.record_failure(&key);
        crate::audit::record(
            &state.store,
            &state.config.storage.tenant_id,
            None,
            "anonymous",
            "anonymous",
            "auth.login",
            "user",
            &form.username,
            "denied",
            401,
            None,
            client_ip(&headers).as_deref(),
            user_agent(&headers).as_deref(),
            json!({"reason": "invalid_credentials"}),
        )
        .await;
        return Err(AppError::unauthorized("invalid username or password"));
    }
    state.limiter.clear(&key);

    if let Some(old) = session_hash(&headers, &state) {
        let _ = state.store.delete_session(&old).await;
    }
    let raw_session = random_hex(32);
    let hash = sha256_hex_str(&raw_session);
    let now = chrono::Utc::now();
    let expires = now + chrono::Duration::hours(state.config.auth.session_ttl_hours as i64);
    state
        .store
        .create_session(&crate::store::SessionRecord {
            session_hash: hash,
            tenant_id: state.config.storage.tenant_id.clone(),
            user_id: user.user_id,
            created_at: now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            expires_at: expires.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        })
        .await?;

    let secure = state.config.cookie_secure_for_host(host_header(&headers));
    let cookie = build_cookie(
        &state.config.auth.cookie_name,
        &raw_session,
        state.config.auth.session_ttl_hours as i64 * 3600,
        true,
        secure,
    );
    let mut response = Redirect::to("/").into_response();
    append_set_cookie(&mut response, cookie);
    Ok(response)
}

#[derive(Deserialize)]
struct LogoutForm {
    csrf_token: Option<String>,
}

async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<LogoutForm>,
) -> Result<Response, AppError> {
    verify_csrf(&state, &headers, form.csrf_token.as_deref())?;
    if let Some(hash) = session_hash(&headers, &state) {
        state.store.delete_session(&hash).await?;
    }
    let secure = state.config.cookie_secure_for_host(host_header(&headers));
    let cookie = build_cookie(&state.config.auth.cookie_name, "", 0, true, secure);
    let mut response = Redirect::to("/login").into_response();
    append_set_cookie(&mut response, cookie);
    Ok(response)
}

async fn page_view(
    State(state): State<AppState>,
    AxumPath((space, slug)): AxumPath<(String, String)>,
    Query(query): Query<LangQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let principal = authenticate(&state, &headers).await?;
    let _limits = crate::limits::enforce(
        &state.store,
        &state.limits,
        &principal,
        Some(&space),
        "page.read",
        &headers,
    )
    .await?;
    let access = require_space_role(
        &state,
        &principal,
        &space,
        SpaceRole::Viewer,
        "page.read",
        &headers,
    )
    .await?;
    if access.break_glass {
        crate::audit::record(
            &state.store,
            &principal.tenant_id,
            Some(&space),
            &principal.principal_id,
            "user",
            "page.read.break_glass",
            "page",
            &slug,
            "allow",
            200,
            None,
            client_ip(&headers).as_deref(),
            user_agent(&headers).as_deref(),
            json!({"role": "space_owner"}),
        )
        .await;
    }
    let policy = policy_for(principal.clone())?;
    let lang = lang_arg(query.lang);
    let view = match state
        .store
        .get_page(&policy, &space, &slug, lang.as_deref())
        .await?
    {
        Some(view) => view,
        None => {
            if state
                .store
                .get_page_unfiltered(&principal.tenant_id, &space, &slug, lang.as_deref())
                .await?
                .is_some()
            {
                crate::audit::record(
                    &state.store,
                    &principal.tenant_id,
                    Some(&space),
                    &principal.principal_id,
                    v1_actor_type(&principal),
                    "acl.denied",
                    "page",
                    &slug,
                    "denied",
                    403,
                    None,
                    client_ip(&headers).as_deref(),
                    user_agent(&headers).as_deref(),
                    json!({"reason": "document_acl_or_temporal"}),
                )
                .await;
                return Err(AppError::forbidden("page is not visible to this principal"));
            }
            return Err(AppError::not_found("page not found"));
        }
    };
    let (csrf, cookie) = csrf_token_for(&state, &headers);
    let rendered = render_markdown(&view.revision.markdown);
    let body = format!(
        "<p class=\"muted\">空间 {} · 语言 {} · 版本 {} · 作者 {} · {}</p>\
<div>{}</div>\
<p><a href=\"/w/{}/{}/edit?lang={}\">编辑</a> · <a href=\"/w/{}/{}/history?lang={}\">历史</a></p>",
        html_escape(&view.page.space_id),
        html_escape(&view.page.lang),
        view.page.version,
        html_escape(&view.revision.author),
        html_escape(&view.revision.created_at),
        rendered,
        url_encode_path_segment(&space),
        url_encode_path_segment(&slug),
        url_encode_path_segment(&view.page.lang),
        url_encode_path_segment(&space),
        url_encode_path_segment(&slug),
        url_encode_path_segment(&view.page.lang)
    );
    let mut response = Html(html_page(
        &view.page.title,
        &body,
        Some(&principal),
        Some(csrf.as_str()),
    ))
    .into_response();
    if let Some(cookie) = cookie {
        append_set_cookie(&mut response, cookie);
    }
    Ok(response)
}

async fn page_edit(
    State(state): State<AppState>,
    AxumPath((space, slug)): AxumPath<(String, String)>,
    Query(query): Query<LangQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let principal = authenticate(&state, &headers).await?;
    require_write(&principal)?;
    require_space_role(
        &state,
        &principal,
        &space,
        SpaceRole::Editor,
        "page.write",
        &headers,
    )
    .await?;
    let space_record = state
        .store
        .get_space(&principal.tenant_id, &space)
        .await?
        .ok_or_else(|| AppError::not_found("space not found"))?;
    require_active_space(&space_record).await?;
    let policy = policy_for(principal.clone())?;
    let lang = lang_arg(query.lang);
    let view = state
        .store
        .get_page(&policy, &space, &slug, lang.as_deref())
        .await?;
    let (csrf, cookie) = csrf_token_for(&state, &headers);
    let (title, markdown, page_lang) = match &view {
        Some(view) => (
            view.page.title.clone(),
            view.revision.markdown.clone(),
            view.page.lang.clone(),
        ),
        None => (
            slug.clone(),
            String::new(),
            lang.unwrap_or_else(|| "zh".to_string()),
        ),
    };
    let body = format!(
        "<form method=\"post\" action=\"/w/{}/{}\">\
<input type=\"hidden\" name=\"csrf_token\" value=\"{}\">\
<input type=\"hidden\" name=\"lang\" value=\"{}\">\
<p><label>标题<br><input name=\"title\" value=\"{}\" required></label></p>\
<p><label>Markdown<br><textarea name=\"markdown\" required>{}</textarea></label></p>\
<p><button type=\"submit\">保存</button></p></form>",
        url_encode_path_segment(&space),
        url_encode_path_segment(&slug),
        html_escape(&csrf),
        html_escape(&page_lang),
        html_escape(&title),
        html_escape(&markdown)
    );
    let mut response = Html(html_page(
        "编辑",
        &body,
        Some(&principal),
        Some(csrf.as_str()),
    ))
    .into_response();
    if let Some(cookie) = cookie {
        append_set_cookie(&mut response, cookie);
    }
    Ok(response)
}

async fn page_history(
    State(state): State<AppState>,
    AxumPath((space, slug)): AxumPath<(String, String)>,
    Query(query): Query<LangQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let principal = authenticate(&state, &headers).await?;
    let _limits = crate::limits::enforce(
        &state.store,
        &state.limits,
        &principal,
        Some(&space),
        "page.read",
        &headers,
    )
    .await?;
    require_space_role(
        &state,
        &principal,
        &space,
        SpaceRole::Viewer,
        "page.read",
        &headers,
    )
    .await?;
    let policy = policy_for(principal.clone())?;
    let lang = lang_arg(query.lang);
    let view = state
        .store
        .get_page(&policy, &space, &slug, lang.as_deref())
        .await?
        .ok_or_else(|| AppError::not_found("page not found"))?;
    let history_lang = lang.clone().unwrap_or_else(|| view.page.lang.clone());
    let revisions = state
        .store
        .list_revisions(
            &principal.tenant_id,
            &space,
            &slug,
            Some(history_lang.as_str()),
        )
        .await?;
    let mut body = String::from("<table><thead><tr><th>版本</th><th>作者</th><th>时间</th><th>哈希</th></tr></thead><tbody>");
    for revision in revisions {
        body.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td><code>{}</code></td></tr>",
            revision.version,
            html_escape(&revision.author),
            html_escape(&revision.created_at),
            html_escape(&truncate_chars(&revision.content_hash, 16))
        ));
    }
    body.push_str("</tbody></table>");
    body.push_str(&format!(
        "<p><a href=\"/w/{}/{}?lang={}\">返回</a></p>",
        url_encode_path_segment(&space),
        url_encode_path_segment(&slug),
        url_encode_path_segment(&view.page.lang)
    ));
    let (csrf, cookie) = csrf_token_for(&state, &headers);
    let mut response = Html(html_page(
        &format!("{} · 历史", view.page.title),
        &body,
        Some(&principal),
        Some(csrf.as_str()),
    ))
    .into_response();
    if let Some(cookie) = cookie {
        append_set_cookie(&mut response, cookie);
    }
    Ok(response)
}

#[derive(Deserialize)]
struct PageForm {
    title: String,
    markdown: String,
    lang: Option<String>,
    csrf_token: Option<String>,
}

async fn page_save(
    State(state): State<AppState>,
    AxumPath((space, slug)): AxumPath<(String, String)>,
    headers: HeaderMap,
    Form(form): Form<PageForm>,
) -> Result<Response, AppError> {
    let principal = authenticate(&state, &headers).await?;
    let _limits = crate::limits::enforce(
        &state.store,
        &state.limits,
        &principal,
        Some(&space),
        "page.write",
        &headers,
    )
    .await?;
    require_write(&principal)?;
    let access = require_space_role(
        &state,
        &principal,
        &space,
        SpaceRole::Editor,
        "page.write",
        &headers,
    )
    .await?;
    let space_record = state
        .store
        .get_space(&principal.tenant_id, &space)
        .await?
        .ok_or_else(|| AppError::not_found("space not found"))?;
    require_active_space(&space_record).await?;
    if access.break_glass {
        crate::audit::record(
            &state.store,
            &principal.tenant_id,
            Some(&space),
            &principal.principal_id,
            "user",
            "page.write.break_glass",
            "page",
            &slug,
            "allow",
            200,
            None,
            client_ip(&headers).as_deref(),
            user_agent(&headers).as_deref(),
            json!({"role": "space_owner"}),
        )
        .await;
    }
    verify_csrf(&state, &headers, form.csrf_token.as_deref())?;
    validate_slug(&slug)?;
    if form.title.trim().is_empty() {
        return Err(AppError::bad_request("title must not be empty"));
    }
    if !principal
        .allowed_space_ids
        .iter()
        .any(|allowed| allowed == &space)
    {
        return Err(AppError::forbidden("space is outside the principal scope"));
    }
    let page_lang = lang_arg(form.lang.clone()).unwrap_or_else(|| "zh".to_string());
    let policy = policy_for(principal.clone())?;
    let existing = state
        .store
        .get_page(&policy, &space, &slug, Some(&page_lang))
        .await?;
    if existing.is_none()
        && state
            .store
            .get_page_unfiltered(&principal.tenant_id, &space, &slug, Some(&page_lang))
            .await?
            .is_some()
    {
        return Err(AppError::forbidden(
            "page exists but is outside the principal's ACL",
        ));
    }
    let acl = match &existing {
        Some(view) => {
            Acl::from_json(&view.page.acl_json).unwrap_or_else(|_| page_acl(&state, &principal))
        }
        None => page_acl(&state, &principal),
    };
    if state
        .store
        .get_space(&principal.tenant_id, &space)
        .await?
        .is_none()
    {
        state
            .store
            .ensure_space(&SpaceRecord::new(
                &principal.tenant_id,
                &space,
                &space,
                &acl,
                &principal.principal_id,
                &now_rfc3339(),
            ))
            .await?;
    }

    let now = now_rfc3339();
    let content_hash = sha256_hex(form.markdown.as_bytes());
    let current_version = existing.as_ref().map(|view| view.page.version).unwrap_or(0);
    let version = current_version + 1;
    let revision_id = doc_id::derive_revision_id(
        &principal.tenant_id,
        &space,
        &slug,
        &page_lang,
        &content_hash,
        version,
    );
    let page = PageRecord {
        tenant_id: principal.tenant_id.clone(),
        space_id: space.clone(),
        slug: slug.clone(),
        lang: page_lang.clone(),
        title: form.title.clone(),
        summary: existing
            .as_ref()
            .map(|view| view.page.summary.clone())
            .unwrap_or_default(),
        order_index: existing
            .as_ref()
            .map(|view| view.page.order_index)
            .unwrap_or_default(),
        current_revision_id: revision_id.clone(),
        content_hash: content_hash.clone(),
        acl_json: acl.to_json(),
        acl_principals: acl.principals_key(),
        valid_from: now.clone(),
        valid_to: None,
        version,
        superseded_by: None,
        created_at: existing
            .as_ref()
            .map(|view| view.page.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now.clone(),
    };
    let revision = RevisionRecord {
        tenant_id: principal.tenant_id.clone(),
        revision_id,
        space_id: space.clone(),
        slug: slug.clone(),
        lang: page_lang.clone(),
        title: form.title.clone(),
        markdown: form.markdown.clone(),
        content_hash: content_hash.clone(),
        author: principal.principal_id.clone(),
        created_at: now.clone(),
        version,
        acl_json: page.acl_json.clone(),
        acl_principals: page.acl_principals.clone(),
        valid_from: now.clone(),
        valid_to: None,
        superseded_by: None,
        source_url: None,
        source_revision: None,
    };
    let drafts = chunker::chunk_text(&form.markdown, chunker::DEFAULT_CHUNK_CHARS);
    let quota_delta = crate::quota::QuotaDelta {
        source_bytes: form.markdown.len() as i64,
        index_bytes: drafts.iter().map(|draft| draft.text.len() as i64).sum(),
        history_bytes: 0,
        documents: 1,
        chunks: drafts.len() as i64,
    };
    let _quota_guard = crate::quota::write_guard().await;
    crate::quota::reserve(&state.store, &space_record, &quota_delta)
        .await
        .map_err(|err| err.to_app_error())?;
    if let Err(err) = state.store.save_page(&page, &revision).await {
        crate::quota::release(&state.store, &space_record, &quota_delta).await;
        return Err(err.into());
    }
    if let Err(err) = index_wiki_document(
        &state,
        &principal,
        WikiIndexInput {
            space: &space,
            slug: &slug,
            lang: &page_lang,
            title: &form.title,
            markdown: &form.markdown,
            acl: &acl,
        },
    )
    .await
    {
        crate::quota::release(&state.store, &space_record, &quota_delta).await;
        return Err(err);
    }
    let _ = crate::quota::refresh(&state.store, &principal.tenant_id, &space).await;
    let target = format!(
        "/w/{}/{}?lang={}",
        url_encode_path_segment(&space),
        url_encode_path_segment(&slug),
        url_encode_path_segment(&page_lang)
    );
    Ok(Redirect::to(target.as_str()).into_response())
}

fn page_acl(_state: &AppState, _principal: &Principal) -> Acl {
    // Space membership/roles gate access; document ACL stays allow-all so all
    // authenticated space members can be checked against their space role.
    Acl::allow_all()
}

fn validate_slug(slug: &str) -> Result<(), AppError> {
    if slug.trim().is_empty()
        || slug.len() > 200
        || slug.contains('/')
        || slug.contains('\\')
        || slug == ".."
        || slug == "."
    {
        return Err(AppError::bad_request("invalid page slug"));
    }
    Ok(())
}

struct WikiIndexInput<'a> {
    space: &'a str,
    slug: &'a str,
    lang: &'a str,
    title: &'a str,
    markdown: &'a str,
    acl: &'a Acl,
}

async fn index_wiki_document(
    state: &AppState,
    principal: &Principal,
    input: WikiIndexInput<'_>,
) -> Result<(), AppError> {
    let WikiIndexInput {
        space,
        slug,
        lang,
        title,
        markdown,
        acl,
    } = input;
    let relative = format!("wiki/{space}/{lang}/{slug}.md");
    let canonical = doc_id::canonical_relative_path(&relative).map_err(AppError::bad_request)?;
    let content_hash = sha256_hex(markdown.as_bytes());
    let doc_id = doc_id::derive_doc_id(&canonical, &content_hash);
    let now = now_rfc3339();
    let acl_json = acl.to_json();
    let acl_principals = acl.principals_key();
    let drafts = chunker::chunk_text(markdown, chunker::DEFAULT_CHUNK_CHARS);
    let mut chunks: Vec<ChunkRecord> = drafts
        .iter()
        .enumerate()
        .map(|(index, draft)| {
            let chunk_hash = sha256_hex(draft.text.as_bytes());
            ChunkRecord {
                tenant_id: principal.tenant_id.clone(),
                chunk_id: doc_id::derive_chunk_id(
                    &principal.tenant_id,
                    &doc_id,
                    index as u32,
                    &chunk_hash,
                ),
                doc_id: doc_id.clone(),
                space_id: space.to_string(),
                seq: index as u32,
                text: draft.text.clone(),
                line_start: draft.line_start,
                line_end: draft.line_end,
                content_hash: chunk_hash,
                acl_json: acl_json.clone(),
                acl_principals: acl_principals.clone(),
                lang: lang.to_string(),
                valid_from: now.clone(),
                valid_to: None,
                superseded_by: None,
                version: 1,
                embedding: None,
            }
        })
        .collect();
    if let Some(provider) = &state.providers.embedding {
        let mut warnings = Vec::new();
        attach_embeddings(&mut chunks, provider.as_ref(), 32, &mut warnings).await;
        for warning in warnings {
            tracing::warn!(warning, "wiki embedding");
        }
    }
    let document = DocumentRecord {
        tenant_id: principal.tenant_id.clone(),
        doc_id,
        space_id: space.to_string(),
        logical_path: canonical,
        source_uri: format!("wiki://{space}/{lang}/{slug}"),
        title: title.to_string(),
        mime: "text/markdown".to_string(),
        raw_hash: content_hash.clone(),
        content_hash,
        content: markdown.to_string(),
        acl_json,
        acl_principals,
        lang: lang.to_string(),
        valid_from: now.clone(),
        valid_to: None,
        version: 1,
        superseded_by: None,
        status: "active".to_string(),
        source_role: "standalone".to_string(),
        canonicalization_version: doc_id::CANONICALIZATION_VERSION.to_string(),
        extractor_version: doc_id::EXTRACTOR_VERSION.to_string(),
        chunker_version: doc_id::CHUNKER_VERSION.to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    state
        .store
        .upsert_document(&document, &chunks, None)
        .await?;
    crate::summary::enqueue(&state.store, &document, &document.content_hash).await;
    if let Some(extractor) = &state.providers.extractor {
        let mut warnings = Vec::new();
        extract_and_store_events(
            &state.store,
            extractor.as_ref(),
            &document,
            &chunks,
            &mut warnings,
        )
        .await;
        for warning in warnings {
            tracing::warn!(warning, "wiki event extraction");
        }
    }
    Ok(())
}

fn ip_hash(headers: &HeaderMap) -> Option<String> {
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|value| value.to_str().ok())
                .map(|value| value.to_string())
        })
        .unwrap_or_else(|| "local".to_string());
    Some(crate::util::sha256_hex(ip.as_bytes()))
}

fn require_feature(state: &AppState, feature: Feature) -> Result<(), AppError> {
    if state.config.feature_enabled(feature) {
        Ok(())
    } else {
        Err(AppError::new(
            StatusCode::FORBIDDEN,
            "feature_disabled",
            format!("feature {} is disabled", feature.as_str()),
        ))
    }
}

async fn features_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (_principal, _session, _limits) =
        v1_auth(&state, &headers, TokenScope::Read, "features", None).await?;
    Ok(Json(json!({
        "mode": state.config.deployment_mode().as_str(),
        "features": state.config.features().to_json(),
        "i18n": {
            "default_locale": state.config.i18n.default_locale,
            "supported_locales": state.config.i18n.supported_locales,
        }
    })))
}

async fn metrics_handler() -> Response {
    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        crate::metrics::render(),
    )
        .into_response()
}

async fn request_logging(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(128).collect::<String>())
        .unwrap_or_else(|| format!("req_{}", random_hex(12)));
    let trace_id = request
        .headers()
        .get("x-trace-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(128).collect::<String>())
        .unwrap_or_else(|| request_id.clone());
    let method = request.method().to_string();
    let route = request.uri().path().to_string();
    let query = request.uri().query().unwrap_or("").to_string();
    let query_hash = if query.is_empty() {
        None
    } else {
        Some(crate::util::sha256_hex(query.as_bytes()))
    };
    let query_length = if query.is_empty() {
        None
    } else {
        Some(query.chars().count())
    };
    let token_id = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(|value| crate::util::sha256_hex(value.trim().as_bytes())[..16].to_string());
    let user_agent = request
        .headers()
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.chars().take(256).collect::<String>());
    let ip_hash = ip_hash(request.headers());
    let locale = crate::i18n::resolve(
        request
            .headers()
            .get(header::ACCEPT_LANGUAGE)
            .and_then(|value| value.to_str().ok()),
        None,
        state.config.i18n.tenant_locale.as_deref(),
        &state.config.i18n.default_locale,
        &state.config.i18n.supported_locales,
    );
    let concurrency = crate::logging::concurrency_enter();
    let start = std::time::Instant::now();
    if let Some(hash) = ip_hash.as_deref() {
        if let Err(err) = state.limits.check_ip(hash) {
            crate::logging::concurrency_leave();
            let mut response = err.into_response();
            crate::logging::access(
                &request_id,
                &trace_id,
                &method,
                &route,
                response.status().as_u16(),
                start.elapsed().as_millis(),
                concurrency,
                ip_hash.as_deref(),
                user_agent.as_deref(),
                query_hash.as_deref(),
                query_length,
                None,
                None,
                token_id.as_deref(),
                Some(&state.config.storage.tenant_id),
                None,
                Some("rate_limited"),
                &[],
            );
            if let Ok(value) = axum::http::HeaderValue::from_str(&request_id) {
                response.headers_mut().insert("x-request-id", value);
            }
            return response;
        }
    }
    let mut response =
        crate::i18n::with_locale(locale.clone(), async move { next.run(request).await }).await;
    crate::logging::concurrency_leave();
    let status = response.status().as_u16();
    let error_code = response
        .extensions()
        .get::<crate::error::ErrorCode>()
        .map(|code| code.0);
    crate::metrics::http_request(status);
    match error_code {
        Some("rate_limited") => crate::metrics::rate_limited(),
        Some("queue_full") => crate::metrics::queue_full(),
        Some("unauthorized") => crate::metrics::auth_failure(),
        _ => {}
    }
    crate::logging::access(
        &request_id,
        &trace_id,
        &method,
        &route,
        status,
        start.elapsed().as_millis(),
        concurrency,
        ip_hash.as_deref(),
        user_agent.as_deref(),
        query_hash.as_deref(),
        query_length,
        None,
        None,
        token_id.as_deref(),
        Some(&state.config.storage.tenant_id),
        None,
        error_code,
        &[],
    );
    if let Ok(value) = axum::http::HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", value);
    }
    if let Ok(value) = axum::http::HeaderValue::from_str(&locale) {
        response
            .headers_mut()
            .insert(header::CONTENT_LANGUAGE, value);
    }
    response
}

// ---------------------------------------------------------------------------
// /api/v1: self-service spaces, groups, members, quotas and audit.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CreateSpaceRequest {
    space_id: Option<String>,
    name: Option<String>,
    visibility: Option<String>,
    owner_type: Option<String>,
    owner_id: Option<String>,
    settings: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct UpdateSpaceRequest {
    name: Option<String>,
    visibility: Option<String>,
    settings: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct TransferSpaceRequest {
    owner_type: String,
    owner_id: String,
}

#[derive(Debug, Deserialize)]
struct MemberRequest {
    principal_type: String,
    principal_id: String,
    role: String,
}

#[derive(Debug, Deserialize)]
struct MemberPatchRequest {
    role: String,
}

#[derive(Debug, Deserialize)]
struct GroupRequest {
    name: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GroupMemberRequest {
    principal_id: String,
    role: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GroupMemberPatchRequest {
    role: String,
}

#[derive(Debug, Deserialize)]
struct AuditQuery {
    actor: Option<String>,
    action: Option<String>,
    space: Option<String>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<i64>,
}

fn v1_request_id() -> String {
    format!("req_{}", random_hex(8))
}

fn v1_actor_type(principal: &Principal) -> &'static str {
    match principal.kind {
        PrincipalKind::Human => "user",
        PrincipalKind::Agent | PrincipalKind::Service => "agent",
    }
}

async fn audit_admin_content_read(
    state: &AppState,
    principal: &Principal,
    headers: &HeaderMap,
    action: &str,
) {
    if principal.tenant_role.is_admin() {
        crate::audit::record(
            &state.store,
            &principal.tenant_id,
            None,
            &principal.principal_id,
            v1_actor_type(principal),
            action,
            "content",
            "search",
            "allow",
            200,
            None,
            client_ip(headers).as_deref(),
            user_agent(headers).as_deref(),
            json!({"reason": "tenant_admin_break_glass"}),
        )
        .await;
    }
}

fn v1_session_write_guard(
    state: &AppState,
    headers: &HeaderMap,
    session: bool,
) -> Result<(), AppError> {
    if session {
        verify_csrf_header(state, headers)?;
    }
    Ok(())
}

fn verify_csrf_header(state: &AppState, headers: &HeaderMap) -> Result<(), AppError> {
    check_origin(headers)?;
    if !state.config.auth.csrf {
        return Ok(());
    }
    let cookie = cookie_value(headers, "lkb_csrf")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::forbidden("missing CSRF cookie"))?;
    let header = headers
        .get("x-csrf-token")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::forbidden("missing CSRF header"))?;
    if !constant_time_eq(cookie.as_bytes(), header.as_bytes()) {
        return Err(AppError::forbidden("CSRF token mismatch"));
    }
    Ok(())
}

fn space_json(space: &SpaceRecord, role: Option<SpaceRole>) -> Value {
    json!({
        "space_id": space.space_id,
        "tenant_id": space.tenant_id,
        "name": space.name,
        "visibility": space.visibility,
        "owner_type": space.owner_type,
        "owner_id": space.owner_id,
        "status": space.status,
        "role": role.map(|value| value.as_str()),
        "created_at": space.created_at,
        "archived_at": space.archived_at,
        "deleted_at": space.deleted_at,
    })
}

fn member_json(member: &SpaceMemberRecord) -> Value {
    json!({
        "space_id": member.space_id,
        "principal_type": member.principal_type.as_str(),
        "principal_id": member.principal_id,
        "role": member.role.as_str(),
        "added_by": member.added_by,
        "created_at": member.created_at,
    })
}

fn group_json(group: &GroupRecord) -> Value {
    json!({
        "group_id": group.group_id,
        "tenant_id": group.tenant_id,
        "name": group.name,
        "description": group.description,
        "created_by": group.created_by,
        "status": group.status,
        "created_at": group.created_at,
        "updated_at": group.updated_at,
    })
}

fn quota_json(quota: &QuotaRecord) -> Value {
    json!({
        "scope_type": quota.scope_type.as_str(),
        "scope_id": quota.scope_id,
        "storage_bytes": quota.storage_bytes,
        "documents": quota.documents,
        "chunks": quota.chunks,
        "members": quota.members,
        "spaces": quota.spaces,
        "enforcement": quota.enforcement,
        "updated_by": quota.updated_by,
        "updated_at": quota.updated_at,
    })
}

fn usage_json(usage: &StorageUsage) -> Value {
    json!({
        "space_id": usage.space_id,
        "source_bytes": usage.source_bytes,
        "index_bytes": usage.index_bytes,
        "history_bytes": usage.history_bytes,
        "total_bytes": usage.total_bytes,
        "reserved_bytes": usage.reserved_bytes,
        "documents": usage.documents,
        "chunks": usage.chunks,
        "members": usage.members,
        "updated_at": usage.updated_at,
    })
}

async fn v1_auth(
    state: &AppState,
    headers: &HeaderMap,
    scope: TokenScope,
    action: &str,
    space: Option<&str>,
) -> Result<(Principal, bool, crate::limits::LimitGuard), AppError> {
    let (principal, auth) = authenticate_v1(state, headers, action).await?;
    if let Err(err) = require_scope(&principal, scope, action) {
        crate::audit::record(
            &state.store,
            &principal.tenant_id,
            space,
            &principal.principal_id,
            v1_actor_type(&principal),
            action,
            "api",
            action,
            "denied",
            err.status().as_u16(),
            None,
            client_ip(headers).as_deref(),
            user_agent(headers).as_deref(),
            json!({"required_scope": format!("{scope:?}")}),
        )
        .await;
        return Err(err);
    }
    let guard = crate::limits::enforce(
        &state.store,
        &state.limits,
        &principal,
        space,
        action,
        headers,
    )
    .await?;
    Ok((principal, auth.session, guard))
}

async fn list_spaces_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, _, _limits) =
        v1_auth(&state, &headers, TokenScope::Read, "space.list", None).await?;
    let mut out = Vec::new();
    for space in state.store.list_spaces(&principal.tenant_id).await? {
        if space.status == "deleted" {
            continue;
        }
        let access = crate::access::effective_space_role(&state.store, &principal, &space)
            .await
            .map_err(AppError::from)?;
        if access.is_some() {
            out.push(space_json(&space, access.map(|value| value.role)));
        } else if space.visibility == "internal" {
            out.push(space_json(&space, None));
        }
    }
    Ok(Json(json!({"spaces": out})))
}

async fn audit_admin_write(
    state: &AppState,
    principal: &Principal,
    action: &str,
    target_type: &str,
    target_id: impl std::fmt::Display,
) {
    let target_id = target_id.to_string();
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        None,
        &principal.principal_id,
        "user",
        action,
        target_type,
        &target_id,
        "allow",
        200,
        None,
        None,
        None,
        json!({}),
    )
    .await;
}

async fn create_space_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateSpaceRequest>,
) -> Result<Response, AppError> {
    let (principal, session, _limits) =
        v1_auth(&state, &headers, TokenScope::Write, "space.create", None).await?;
    v1_session_write_guard(&state, &headers, session)?;
    if principal.kind == PrincipalKind::Agent {
        return Err(AppError::forbidden("agents cannot create spaces"));
    }
    let owner_type = request.owner_type.unwrap_or_else(|| "user".to_string());
    if owner_type != "user" && owner_type != "group" {
        return Err(AppError::bad_request("owner_type must be user or group"));
    }
    let owner_id = request
        .owner_id
        .unwrap_or_else(|| principal.principal_id.clone());
    if !principal.tenant_role.is_admin()
        && owner_type == "user"
        && owner_id != principal.principal_id
    {
        return Err(AppError::forbidden(
            "cannot create a space for another user",
        ));
    }
    if owner_type == "group" {
        let group = state
            .store
            .get_group(&principal.tenant_id, &owner_id)
            .await?
            .ok_or_else(|| AppError::bad_request("group not found"))?;
        let is_admin = state
            .store
            .get_group_member(&group.group_id, &principal.principal_id)
            .await?
            .map(|member| member.role == GroupRole::GroupAdmin)
            .unwrap_or(false);
        if !is_admin && !principal.tenant_role.is_admin() {
            return Err(AppError::forbidden(
                "group_admin required to create a group space",
            ));
        }
    }
    if !principal.tenant_role.is_admin() && !state.config.spaces.allow_user_creation {
        return Err(AppError::forbidden(
            "self-service space creation is disabled",
        ));
    }
    let space_id = match request.space_id {
        Some(value) => {
            validate_space_id(&value)?;
            value
        }
        None => format!("sp_{}", random_hex(8)),
    };
    if state
        .store
        .get_space(&principal.tenant_id, &space_id)
        .await?
        .is_some()
    {
        return Err(AppError::new(
            StatusCode::CONFLICT,
            "conflict",
            "space already exists",
        ));
    }
    crate::quota::check_space_creation(
        &state.store,
        &principal.tenant_id,
        &owner_type,
        &owner_id,
        state.config.spaces.max_per_user,
    )
    .await
    .map_err(|err| err.to_app_error())?;

    let now = now_rfc3339();
    let mut space = SpaceRecord::new(
        &principal.tenant_id,
        &space_id,
        request.name.as_deref().unwrap_or(&space_id),
        &Acl::allow_all(),
        &principal.principal_id,
        &now,
    );
    space.owner_type = owner_type.clone();
    space.owner_id = owner_id.clone();
    space.owner_principal_id = owner_id.clone();
    space.visibility = request.visibility.unwrap_or_else(|| "private".to_string());
    if space.visibility != "private" && space.visibility != "internal" {
        return Err(AppError::bad_request(
            "visibility must be private or internal",
        ));
    }
    space.settings = request
        .settings
        .map(|value| value.to_string())
        .unwrap_or_else(|| "{}".to_string());
    state.store.ensure_space(&space).await?;

    if owner_type == "user" {
        state
            .store
            .upsert_space_member(&SpaceMemberRecord {
                space_id: space_id.clone(),
                principal_type: PrincipalType::User,
                principal_id: owner_id.clone(),
                role: SpaceRole::SpaceOwner,
                added_by: principal.principal_id.clone(),
                created_at: now.clone(),
            })
            .await?;
    }
    crate::quota::apply_default_quota(
        &state.store,
        &principal.tenant_id,
        &space_id,
        &principal.principal_id,
        &state.config.spaces,
    )
    .await?;
    let request_id = v1_request_id();
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        Some(&space_id),
        &principal.principal_id,
        v1_actor_type(&principal),
        "space.created",
        "space",
        &space_id,
        "allow",
        201,
        Some(&request_id),
        client_ip(&headers).as_deref(),
        user_agent(&headers).as_deref(),
        json!({"owner_type": owner_type, "owner_id": owner_id}),
    )
    .await;
    Ok((
        StatusCode::CREATED,
        Json(json!({"space": space_json(&space, Some(SpaceRole::SpaceOwner)), "request_id": request_id})),
    )
        .into_response())
}

async fn get_space_v1(
    State(state): State<AppState>,
    AxumPath(space_id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, _, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Read,
        "space.get",
        Some(&space_id),
    )
    .await?;
    let space = state
        .store
        .get_space(&principal.tenant_id, &space_id)
        .await?
        .ok_or_else(|| AppError::not_found("space not found"))?;
    if space.status == "deleted" {
        return Err(AppError::not_found("space not found"));
    }
    let access = crate::access::effective_space_role(&state.store, &principal, &space)
        .await
        .map_err(AppError::from)?;
    if access.is_none() && space.visibility != "internal" {
        return Err(AppError::forbidden("space is private"));
    }
    let now = now_rfc3339();
    let usage = state
        .store
        .get_storage_usage(&principal.tenant_id, &space_id)
        .await?
        .unwrap_or_else(|| StorageUsage::zero(&principal.tenant_id, &space_id, &now));
    Ok(Json(json!({
        "space": space_json(&space, access.map(|value| value.role)),
        "usage": usage_json(&usage),
    })))
}

async fn patch_space_v1(
    State(state): State<AppState>,
    AxumPath(space_id): AxumPath<String>,
    headers: HeaderMap,
    Json(request): Json<UpdateSpaceRequest>,
) -> Result<Json<Value>, AppError> {
    let (principal, session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Write,
        "space.update",
        Some(&space_id),
    )
    .await?;
    v1_session_write_guard(&state, &headers, session)?;
    let mut space = state
        .store
        .get_space(&principal.tenant_id, &space_id)
        .await?
        .ok_or_else(|| AppError::not_found("space not found"))?;
    require_space_role(
        &state,
        &principal,
        &space_id,
        SpaceRole::SpaceAdmin,
        "space.update",
        &headers,
    )
    .await?;
    require_active_space(&space).await?;
    if let Some(name) = request.name {
        if name.trim().is_empty() {
            return Err(AppError::bad_request("name must not be empty"));
        }
        space.name = name;
    }
    if let Some(visibility) = request.visibility {
        if visibility != "private" && visibility != "internal" {
            return Err(AppError::bad_request(
                "visibility must be private or internal",
            ));
        }
        space.visibility = visibility;
    }
    if let Some(settings) = request.settings {
        space.settings = settings.to_string();
    }
    state.store.update_space(&space).await?;
    let request_id = v1_request_id();
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        Some(&space_id),
        &principal.principal_id,
        v1_actor_type(&principal),
        "space.updated",
        "space",
        &space_id,
        "allow",
        200,
        Some(&request_id),
        client_ip(&headers).as_deref(),
        user_agent(&headers).as_deref(),
        json!({}),
    )
    .await;
    Ok(Json(
        json!({"space": space_json(&space, Some(SpaceRole::SpaceAdmin)), "request_id": request_id}),
    ))
}

async fn delete_space_v1(
    State(state): State<AppState>,
    AxumPath(space_id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Write,
        "space.delete",
        Some(&space_id),
    )
    .await?;
    v1_session_write_guard(&state, &headers, session)?;
    let mut space = state
        .store
        .get_space(&principal.tenant_id, &space_id)
        .await?
        .ok_or_else(|| AppError::not_found("space not found"))?;
    require_space_role(
        &state,
        &principal,
        &space_id,
        SpaceRole::SpaceOwner,
        "space.delete",
        &headers,
    )
    .await?;
    space.status = "deleted".to_string();
    space.deleted_at = Some(now_rfc3339());
    state.store.update_space(&space).await?;
    let request_id = v1_request_id();
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        Some(&space_id),
        &principal.principal_id,
        v1_actor_type(&principal),
        "space.deleted",
        "space",
        &space_id,
        "allow",
        200,
        Some(&request_id),
        client_ip(&headers).as_deref(),
        user_agent(&headers).as_deref(),
        json!({}),
    )
    .await;
    Ok(Json(
        json!({"space_id": space_id, "status": "deleted", "request_id": request_id}),
    ))
}

async fn archive_space_v1(
    State(state): State<AppState>,
    AxumPath(space_id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Write,
        "space.archive",
        Some(&space_id),
    )
    .await?;
    v1_session_write_guard(&state, &headers, session)?;
    let mut space = state
        .store
        .get_space(&principal.tenant_id, &space_id)
        .await?
        .ok_or_else(|| AppError::not_found("space not found"))?;
    require_space_role(
        &state,
        &principal,
        &space_id,
        SpaceRole::SpaceOwner,
        "space.archive",
        &headers,
    )
    .await?;
    if space.status == "deleted" {
        return Err(AppError::not_found("space not found"));
    }
    space.status = "archived".to_string();
    space.archived_at = Some(now_rfc3339());
    state.store.update_space(&space).await?;
    let request_id = v1_request_id();
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        Some(&space_id),
        &principal.principal_id,
        v1_actor_type(&principal),
        "space.archived",
        "space",
        &space_id,
        "allow",
        200,
        Some(&request_id),
        client_ip(&headers).as_deref(),
        user_agent(&headers).as_deref(),
        json!({}),
    )
    .await;
    Ok(Json(
        json!({"space_id": space_id, "status": "archived", "request_id": request_id}),
    ))
}

async fn restore_space_v1(
    State(state): State<AppState>,
    AxumPath(space_id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Write,
        "space.restore",
        Some(&space_id),
    )
    .await?;
    v1_session_write_guard(&state, &headers, session)?;
    let mut space = state
        .store
        .get_space(&principal.tenant_id, &space_id)
        .await?
        .ok_or_else(|| AppError::not_found("space not found"))?;
    require_space_role(
        &state,
        &principal,
        &space_id,
        SpaceRole::SpaceOwner,
        "space.restore",
        &headers,
    )
    .await?;
    space.status = "active".to_string();
    space.archived_at = None;
    space.deleted_at = None;
    state.store.update_space(&space).await?;
    let request_id = v1_request_id();
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        Some(&space_id),
        &principal.principal_id,
        v1_actor_type(&principal),
        "space.restored",
        "space",
        &space_id,
        "allow",
        200,
        Some(&request_id),
        client_ip(&headers).as_deref(),
        user_agent(&headers).as_deref(),
        json!({}),
    )
    .await;
    Ok(Json(
        json!({"space_id": space_id, "status": "active", "request_id": request_id}),
    ))
}

async fn transfer_space_v1(
    State(state): State<AppState>,
    AxumPath(space_id): AxumPath<String>,
    headers: HeaderMap,
    Json(request): Json<TransferSpaceRequest>,
) -> Result<Json<Value>, AppError> {
    let (principal, session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Write,
        "space.transfer",
        Some(&space_id),
    )
    .await?;
    v1_session_write_guard(&state, &headers, session)?;
    let mut space = state
        .store
        .get_space(&principal.tenant_id, &space_id)
        .await?
        .ok_or_else(|| AppError::not_found("space not found"))?;
    let access = crate::access::effective_space_role(&state.store, &principal, &space)
        .await
        .map_err(AppError::from)?;
    if !principal.tenant_role.is_admin()
        && !access
            .map(|value| value.role >= SpaceRole::SpaceOwner)
            .unwrap_or(false)
    {
        return Err(AppError::forbidden("space_owner required to transfer"));
    }
    if request.owner_type != "user" && request.owner_type != "group" {
        return Err(AppError::bad_request("owner_type must be user or group"));
    }
    match request.owner_type.as_str() {
        "user" => {
            if state
                .store
                .get_user_by_id(&principal.tenant_id, &request.owner_id)
                .await?
                .is_none()
            {
                return Err(AppError::bad_request("target user not found"));
            }
        }
        "group"
            if state
                .store
                .get_group(&principal.tenant_id, &request.owner_id)
                .await?
                .is_none() =>
        {
            return Err(AppError::bad_request("target group not found"));
        }
        _ => {}
    }
    let old_type = space.owner_type.clone();
    let old_id = space.owner_id.clone();
    space.owner_type = request.owner_type.clone();
    space.owner_id = request.owner_id.clone();
    space.owner_principal_id = request.owner_id.clone();
    state.store.update_space(&space).await?;
    let now = now_rfc3339();
    if request.owner_type == "user" {
        state
            .store
            .upsert_space_member(&SpaceMemberRecord {
                space_id: space_id.clone(),
                principal_type: PrincipalType::User,
                principal_id: request.owner_id.clone(),
                role: SpaceRole::SpaceOwner,
                added_by: principal.principal_id.clone(),
                created_at: now.clone(),
            })
            .await?;
    }
    if old_type == "user" && old_id != request.owner_id {
        state
            .store
            .upsert_space_member(&SpaceMemberRecord {
                space_id: space_id.clone(),
                principal_type: PrincipalType::User,
                principal_id: old_id.clone(),
                role: SpaceRole::SpaceAdmin,
                added_by: principal.principal_id.clone(),
                created_at: now,
            })
            .await?;
    }
    let request_id = v1_request_id();
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        Some(&space_id),
        &principal.principal_id,
        v1_actor_type(&principal),
        "ownership.transferred",
        "space",
        &space_id,
        "allow",
        200,
        Some(&request_id),
        client_ip(&headers).as_deref(),
        user_agent(&headers).as_deref(),
        json!({"from_type": old_type, "from_id": old_id, "to_type": request.owner_type, "to_id": request.owner_id}),
    )
    .await;
    Ok(Json(
        json!({"space": space_json(&space, Some(SpaceRole::SpaceOwner)), "request_id": request_id}),
    ))
}

async fn space_usage_v1(
    State(state): State<AppState>,
    AxumPath(space_id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, _, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Read,
        "space.usage",
        Some(&space_id),
    )
    .await?;
    let space = state
        .store
        .get_space(&principal.tenant_id, &space_id)
        .await?
        .ok_or_else(|| AppError::not_found("space not found"))?;
    require_space_role(
        &state,
        &principal,
        &space_id,
        SpaceRole::Viewer,
        "space.usage",
        &headers,
    )
    .await?;
    let now = now_rfc3339();
    let usage = state
        .store
        .get_storage_usage(&principal.tenant_id, &space_id)
        .await?
        .unwrap_or_else(|| StorageUsage::zero(&principal.tenant_id, &space_id, &now));
    let mut quotas = Vec::new();
    for (scope, scope_id) in [
        (QuotaScope::Tenant, principal.tenant_id.clone()),
        (
            if space.owner_type == "group" {
                QuotaScope::Group
            } else {
                QuotaScope::User
            },
            space.owner_id.clone(),
        ),
        (QuotaScope::Space, space_id.clone()),
    ] {
        if let Some(quota) = state
            .store
            .get_quota(&principal.tenant_id, scope, &scope_id)
            .await?
        {
            quotas.push(quota_json(&quota));
        }
    }
    Ok(Json(json!({
        "space": space_json(&space, None),
        "usage": usage_json(&usage),
        "quotas": quotas,
    })))
}

fn validate_space_id(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty()
        || value.len() > 100
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err(AppError::bad_request(
            "space_id must be an ASCII alphanumeric/-/_ segment",
        ));
    }
    Ok(())
}

fn parse_principal_type(value: &str) -> Result<PrincipalType, AppError> {
    PrincipalType::parse(value).map_err(AppError::bad_request)
}

fn parse_space_role(value: &str) -> Result<SpaceRole, AppError> {
    SpaceRole::parse(value).map_err(AppError::bad_request)
}

fn parse_group_role(value: &str) -> Result<GroupRole, AppError> {
    GroupRole::parse(value).map_err(AppError::bad_request)
}

async fn require_group_admin(
    state: &AppState,
    principal: &Principal,
    group_id: &str,
    action: &str,
    headers: &HeaderMap,
) -> Result<GroupRecord, AppError> {
    let group = state
        .store
        .get_group(&principal.tenant_id, group_id)
        .await?
        .ok_or_else(|| AppError::not_found("group not found"))?;
    if principal.tenant_role.is_admin() {
        return Ok(group);
    }
    let member = state
        .store
        .get_group_member(group_id, &principal.principal_id)
        .await?;
    match member {
        Some(member) if member.role >= GroupRole::GroupAdmin => Ok(group),
        _ => {
            crate::audit::record(
                &state.store,
                &principal.tenant_id,
                None,
                &principal.principal_id,
                v1_actor_type(principal),
                action,
                "group",
                group_id,
                "denied",
                403,
                None,
                client_ip(headers).as_deref(),
                user_agent(headers).as_deref(),
                json!({"required": "group_admin"}),
            )
            .await;
            Err(AppError::forbidden("group_admin required"))
        }
    }
}

async fn validate_member_target(
    state: &AppState,
    tenant_id: &str,
    principal_type: PrincipalType,
    principal_id: &str,
) -> Result<(), AppError> {
    match principal_type {
        PrincipalType::User | PrincipalType::Agent => {
            if state
                .store
                .get_user_by_id(tenant_id, principal_id)
                .await?
                .is_none()
            {
                return Err(AppError::bad_request("user not found"));
            }
        }
        PrincipalType::Group => {
            if state
                .store
                .get_group(tenant_id, principal_id)
                .await?
                .is_none()
            {
                return Err(AppError::bad_request("group not found"));
            }
        }
    }
    Ok(())
}

async fn list_space_members_v1(
    State(state): State<AppState>,
    AxumPath(space_id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, _, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Read,
        "member.list",
        Some(&space_id),
    )
    .await?;
    require_space_role(
        &state,
        &principal,
        &space_id,
        SpaceRole::SpaceAdmin,
        "member.list",
        &headers,
    )
    .await?;
    let members = state.store.list_space_members(&space_id).await?;
    Ok(Json(json!({
        "space_id": space_id,
        "members": members.iter().map(member_json).collect::<Vec<_>>(),
    })))
}

async fn add_space_member_v1(
    State(state): State<AppState>,
    AxumPath(space_id): AxumPath<String>,
    headers: HeaderMap,
    Json(request): Json<MemberRequest>,
) -> Result<Response, AppError> {
    let (principal, session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Write,
        "member.add",
        Some(&space_id),
    )
    .await?;
    v1_session_write_guard(&state, &headers, session)?;
    let space = state
        .store
        .get_space(&principal.tenant_id, &space_id)
        .await?
        .ok_or_else(|| AppError::not_found("space not found"))?;
    require_space_role(
        &state,
        &principal,
        &space_id,
        SpaceRole::SpaceAdmin,
        "member.add",
        &headers,
    )
    .await?;
    require_active_space(&space).await?;
    let principal_type = parse_principal_type(&request.principal_type)?;
    let role = parse_space_role(&request.role)?;
    validate_member_target(
        &state,
        &principal.tenant_id,
        principal_type,
        &request.principal_id,
    )
    .await?;
    let existing = state
        .store
        .get_space_member(&space_id, principal_type, &request.principal_id)
        .await?;
    if existing.is_none() {
        crate::quota::check_member_quota(&state.store, &space, 1)
            .await
            .map_err(|err| err.to_app_error())?;
    }
    state
        .store
        .upsert_space_member(&SpaceMemberRecord {
            space_id: space_id.clone(),
            principal_type,
            principal_id: request.principal_id.clone(),
            role,
            added_by: principal.principal_id.clone(),
            created_at: now_rfc3339(),
        })
        .await?;
    let _ = crate::quota::refresh(&state.store, &principal.tenant_id, &space_id).await;
    let request_id = v1_request_id();
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        Some(&space_id),
        &principal.principal_id,
        v1_actor_type(&principal),
        "member.added",
        "space_member",
        &format!("{}:{}", principal_type.as_str(), request.principal_id),
        "allow",
        201,
        Some(&request_id),
        client_ip(&headers).as_deref(),
        user_agent(&headers).as_deref(),
        json!({"role": role.as_str()}),
    )
    .await;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "space_id": space_id,
            "principal_type": principal_type.as_str(),
            "principal_id": request.principal_id,
            "role": role.as_str(),
            "request_id": request_id,
        })),
    )
        .into_response())
}

async fn patch_space_member_v1(
    State(state): State<AppState>,
    AxumPath((space_id, principal_type, principal_id)): AxumPath<(String, String, String)>,
    headers: HeaderMap,
    Json(request): Json<MemberPatchRequest>,
) -> Result<Json<Value>, AppError> {
    let (principal, session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Write,
        "member.role",
        Some(&space_id),
    )
    .await?;
    v1_session_write_guard(&state, &headers, session)?;
    require_space_role(
        &state,
        &principal,
        &space_id,
        SpaceRole::SpaceAdmin,
        "member.role",
        &headers,
    )
    .await?;
    let principal_type = parse_principal_type(&principal_type)?;
    let role = parse_space_role(&request.role)?;
    let existing = state
        .store
        .get_space_member(&space_id, principal_type, &principal_id)
        .await?
        .ok_or_else(|| AppError::not_found("member not found"))?;
    if existing.role == SpaceRole::SpaceOwner
        && role != SpaceRole::SpaceOwner
        && state.store.count_space_owners(&space_id).await? <= 1
    {
        return Err(AppError::new(
            StatusCode::CONFLICT,
            "last_space_owner_protected",
            "cannot remove or demote the last space_owner",
        ));
    }
    state
        .store
        .upsert_space_member(&SpaceMemberRecord {
            space_id: space_id.clone(),
            principal_type,
            principal_id: principal_id.clone(),
            role,
            added_by: principal.principal_id.clone(),
            created_at: existing.created_at,
        })
        .await?;
    let request_id = v1_request_id();
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        Some(&space_id),
        &principal.principal_id,
        v1_actor_type(&principal),
        "member.role_changed",
        "space_member",
        &format!("{}:{}", principal_type.as_str(), principal_id),
        "allow",
        200,
        Some(&request_id),
        client_ip(&headers).as_deref(),
        user_agent(&headers).as_deref(),
        json!({"from": existing.role.as_str(), "to": role.as_str()}),
    )
    .await;
    Ok(Json(json!({
        "space_id": space_id,
        "principal_type": principal_type.as_str(),
        "principal_id": principal_id,
        "role": role.as_str(),
        "request_id": request_id,
    })))
}

async fn remove_space_member_v1(
    State(state): State<AppState>,
    AxumPath((space_id, principal_type, principal_id)): AxumPath<(String, String, String)>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Write,
        "member.remove",
        Some(&space_id),
    )
    .await?;
    v1_session_write_guard(&state, &headers, session)?;
    require_space_role(
        &state,
        &principal,
        &space_id,
        SpaceRole::SpaceAdmin,
        "member.remove",
        &headers,
    )
    .await?;
    let principal_type = parse_principal_type(&principal_type)?;
    let existing = state
        .store
        .get_space_member(&space_id, principal_type, &principal_id)
        .await?
        .ok_or_else(|| AppError::not_found("member not found"))?;
    if existing.role == SpaceRole::SpaceOwner
        && state.store.count_space_owners(&space_id).await? <= 1
    {
        return Err(AppError::new(
            StatusCode::CONFLICT,
            "last_space_owner_protected",
            "cannot remove the last space_owner",
        ));
    }
    state
        .store
        .remove_space_member(&space_id, principal_type, &principal_id)
        .await?;
    let _ = crate::quota::refresh(&state.store, &principal.tenant_id, &space_id).await;
    let request_id = v1_request_id();
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        Some(&space_id),
        &principal.principal_id,
        v1_actor_type(&principal),
        "member.removed",
        "space_member",
        &format!("{}:{}", principal_type.as_str(), principal_id),
        "allow",
        200,
        Some(&request_id),
        client_ip(&headers).as_deref(),
        user_agent(&headers).as_deref(),
        json!({"role": existing.role.as_str()}),
    )
    .await;
    Ok(Json(
        json!({"space_id": space_id, "removed": true, "request_id": request_id}),
    ))
}

#[derive(Debug, Deserialize)]
struct SpaceQuotaRequest {
    storage_bytes: Option<i64>,
    documents: Option<i64>,
    chunks: Option<i64>,
    members: Option<i64>,
    enforcement: Option<String>,
    reason: Option<String>,
}

async fn set_space_quota_v1(
    State(state): State<AppState>,
    AxumPath(space_id): AxumPath<String>,
    headers: HeaderMap,
    Json(request): Json<SpaceQuotaRequest>,
) -> Result<Json<Value>, AppError> {
    let (principal, session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Write,
        "quota.update",
        Some(&space_id),
    )
    .await?;
    v1_session_write_guard(&state, &headers, session)?;
    require_space_role(
        &state,
        &principal,
        &space_id,
        SpaceRole::SpaceAdmin,
        "quota.update",
        &headers,
    )
    .await?;
    let enforcement = request.enforcement.unwrap_or_else(|| "hard".to_string());
    if enforcement != "hard" && enforcement != "soft" {
        return Err(AppError::bad_request("enforcement must be hard or soft"));
    }
    let quota = QuotaRecord {
        tenant_id: principal.tenant_id.clone(),
        scope_type: QuotaScope::Space,
        scope_id: space_id.clone(),
        storage_bytes: request.storage_bytes,
        documents: request.documents,
        chunks: request.chunks,
        members: request.members,
        spaces: None,
        enforcement,
        updated_by: principal.principal_id.clone(),
        updated_at: now_rfc3339(),
    };
    state.store.upsert_quota(&quota).await?;
    let request_id = v1_request_id();
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        Some(&space_id),
        &principal.principal_id,
        v1_actor_type(&principal),
        "quota.updated",
        "space_quota",
        &space_id,
        "allow",
        200,
        Some(&request_id),
        client_ip(&headers).as_deref(),
        user_agent(&headers).as_deref(),
        json!({"reason": request.reason}),
    )
    .await;
    Ok(Json(
        json!({"quota": quota_json(&quota), "request_id": request_id}),
    ))
}

#[derive(Debug, Deserialize)]
struct QuotaOverrideRequest {
    scope_type: String,
    scope_id: String,
    dimension: String,
    limit_value: i64,
    reason: String,
    expires_at: Option<String>,
}

async fn create_quota_override_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<QuotaOverrideRequest>,
) -> Result<Json<Value>, AppError> {
    let (principal, session, _limits) =
        v1_auth(&state, &headers, TokenScope::Admin, "quota.override", None).await?;
    v1_session_write_guard(&state, &headers, session)?;
    if request.reason.trim().is_empty() {
        return Err(AppError::bad_request("reason is required"));
    }
    let scope_type = QuotaScope::parse(&request.scope_type).map_err(AppError::bad_request)?;
    let item = QuotaOverrideRecord {
        override_id: format!("qov_{}", random_hex(8)),
        tenant_id: principal.tenant_id.clone(),
        scope_type,
        scope_id: request.scope_id.clone(),
        dimension: request.dimension.clone(),
        limit_value: request.limit_value,
        reason: request.reason.clone(),
        actor: principal.principal_id.clone(),
        expires_at: request.expires_at.clone(),
        created_at: now_rfc3339(),
    };
    state.store.upsert_quota_override(&item).await?;
    let request_id = v1_request_id();
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        None,
        &principal.principal_id,
        v1_actor_type(&principal),
        "quota.override",
        "quota_override",
        &item.override_id,
        "allow",
        200,
        Some(&request_id),
        client_ip(&headers).as_deref(),
        user_agent(&headers).as_deref(),
        json!({
            "scope_type": scope_type.as_str(),
            "scope_id": request.scope_id,
            "dimension": request.dimension,
            "limit_value": request.limit_value,
        }),
    )
    .await;
    Ok(Json(json!({
        "override_id": item.override_id,
        "scope_type": scope_type.as_str(),
        "scope_id": item.scope_id,
        "dimension": item.dimension,
        "limit_value": item.limit_value,
        "expires_at": item.expires_at,
        "request_id": request_id,
    })))
}

async fn list_groups_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, _, _limits) =
        v1_auth(&state, &headers, TokenScope::Read, "group.list", None).await?;
    let groups = if principal.tenant_role.is_admin() {
        state.store.list_groups(&principal.tenant_id).await?
    } else {
        state
            .store
            .list_group_memberships(&principal.tenant_id, &principal.principal_id)
            .await?
            .into_iter()
            .map(|membership| membership.group)
            .collect()
    };
    Ok(Json(json!({
        "groups": groups.iter().map(group_json).collect::<Vec<_>>(),
    })))
}

async fn create_group_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<GroupRequest>,
) -> Result<Response, AppError> {
    let (principal, session, _limits) =
        v1_auth(&state, &headers, TokenScope::Write, "group.create", None).await?;
    v1_session_write_guard(&state, &headers, session)?;
    if !principal.tenant_role.is_admin() && !state.config.groups.allow_user_creation {
        return Err(AppError::forbidden(
            "self-service group creation is disabled",
        ));
    }
    let name = request
        .name
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::bad_request("name is required"))?;
    let now = now_rfc3339();
    let group = GroupRecord {
        group_id: format!("grp_{}", random_hex(8)),
        tenant_id: principal.tenant_id.clone(),
        name,
        description: request.description.unwrap_or_default(),
        created_by: principal.principal_id.clone(),
        status: "active".to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    state.store.create_group(&group).await?;
    state
        .store
        .add_group_member(&GroupMemberRecord {
            group_id: group.group_id.clone(),
            principal_id: principal.principal_id.clone(),
            role: GroupRole::GroupAdmin,
            added_by: principal.principal_id.clone(),
            created_at: now_rfc3339(),
        })
        .await?;
    let request_id = v1_request_id();
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        None,
        &principal.principal_id,
        v1_actor_type(&principal),
        "group.created",
        "group",
        &group.group_id,
        "allow",
        201,
        Some(&request_id),
        client_ip(&headers).as_deref(),
        user_agent(&headers).as_deref(),
        json!({"name": group.name}),
    )
    .await;
    Ok((
        StatusCode::CREATED,
        Json(json!({"group": group_json(&group), "request_id": request_id})),
    )
        .into_response())
}

async fn get_group_v1(
    State(state): State<AppState>,
    AxumPath(group_id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, _, _limits) =
        v1_auth(&state, &headers, TokenScope::Read, "group.get", None).await?;
    let group = require_group_admin(&state, &principal, &group_id, "group.get", &headers).await?;
    Ok(Json(json!({"group": group_json(&group)})))
}

async fn patch_group_v1(
    State(state): State<AppState>,
    AxumPath(group_id): AxumPath<String>,
    headers: HeaderMap,
    Json(request): Json<GroupRequest>,
) -> Result<Json<Value>, AppError> {
    let (principal, session, _limits) =
        v1_auth(&state, &headers, TokenScope::Write, "group.update", None).await?;
    v1_session_write_guard(&state, &headers, session)?;
    let mut group =
        require_group_admin(&state, &principal, &group_id, "group.update", &headers).await?;
    if let Some(name) = request.name {
        if name.trim().is_empty() {
            return Err(AppError::bad_request("name must not be empty"));
        }
        group.name = name;
    }
    if let Some(description) = request.description {
        group.description = description;
    }
    group.updated_at = now_rfc3339();
    state.store.update_group(&group).await?;
    let request_id = v1_request_id();
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        None,
        &principal.principal_id,
        v1_actor_type(&principal),
        "group.updated",
        "group",
        &group_id,
        "allow",
        200,
        Some(&request_id),
        client_ip(&headers).as_deref(),
        user_agent(&headers).as_deref(),
        json!({}),
    )
    .await;
    Ok(Json(
        json!({"group": group_json(&group), "request_id": request_id}),
    ))
}

async fn delete_group_v1(
    State(state): State<AppState>,
    AxumPath(group_id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, session, _limits) =
        v1_auth(&state, &headers, TokenScope::Write, "group.archive", None).await?;
    v1_session_write_guard(&state, &headers, session)?;
    let mut group =
        require_group_admin(&state, &principal, &group_id, "group.archive", &headers).await?;
    group.status = "archived".to_string();
    group.updated_at = now_rfc3339();
    state.store.update_group(&group).await?;
    let request_id = v1_request_id();
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        None,
        &principal.principal_id,
        v1_actor_type(&principal),
        "group.archived",
        "group",
        &group_id,
        "allow",
        200,
        Some(&request_id),
        client_ip(&headers).as_deref(),
        user_agent(&headers).as_deref(),
        json!({}),
    )
    .await;
    Ok(Json(
        json!({"group_id": group_id, "status": "archived", "request_id": request_id}),
    ))
}

async fn list_group_members_v1(
    State(state): State<AppState>,
    AxumPath(group_id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, _, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Read,
        "group.member_list",
        None,
    )
    .await?;
    require_group_admin(&state, &principal, &group_id, "group.member_list", &headers).await?;
    let members = state.store.list_group_members(&group_id).await?;
    Ok(Json(json!({
        "group_id": group_id,
        "members": members
            .into_iter()
            .map(|member| json!({
                "principal_id": member.principal_id,
                "role": member.role.as_str(),
                "added_by": member.added_by,
                "created_at": member.created_at,
            }))
            .collect::<Vec<_>>(),
    })))
}

async fn add_group_member_v1(
    State(state): State<AppState>,
    AxumPath(group_id): AxumPath<String>,
    headers: HeaderMap,
    Json(request): Json<GroupMemberRequest>,
) -> Result<Response, AppError> {
    let (principal, session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Write,
        "group.member_add",
        None,
    )
    .await?;
    v1_session_write_guard(&state, &headers, session)?;
    require_group_admin(&state, &principal, &group_id, "group.member_add", &headers).await?;
    if state
        .store
        .get_user_by_id(&principal.tenant_id, &request.principal_id)
        .await?
        .is_none()
    {
        return Err(AppError::bad_request("user not found"));
    }
    let role = request
        .role
        .as_deref()
        .map(parse_group_role)
        .transpose()?
        .unwrap_or(GroupRole::GroupMember);
    state
        .store
        .add_group_member(&GroupMemberRecord {
            group_id: group_id.clone(),
            principal_id: request.principal_id.clone(),
            role,
            added_by: principal.principal_id.clone(),
            created_at: now_rfc3339(),
        })
        .await?;
    let request_id = v1_request_id();
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        None,
        &principal.principal_id,
        v1_actor_type(&principal),
        "group.member_added",
        "group_member",
        &format!("{group_id}:{}", request.principal_id),
        "allow",
        201,
        Some(&request_id),
        client_ip(&headers).as_deref(),
        user_agent(&headers).as_deref(),
        json!({"role": role.as_str()}),
    )
    .await;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "group_id": group_id,
            "principal_id": request.principal_id,
            "role": role.as_str(),
            "request_id": request_id,
        })),
    )
        .into_response())
}

async fn patch_group_member_v1(
    State(state): State<AppState>,
    AxumPath((group_id, principal_id)): AxumPath<(String, String)>,
    headers: HeaderMap,
    Json(request): Json<GroupMemberPatchRequest>,
) -> Result<Json<Value>, AppError> {
    let (principal, session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Write,
        "group.member_role",
        None,
    )
    .await?;
    v1_session_write_guard(&state, &headers, session)?;
    require_group_admin(&state, &principal, &group_id, "group.member_role", &headers).await?;
    let role = parse_group_role(&request.role)?;
    let existing = state
        .store
        .get_group_member(&group_id, &principal_id)
        .await?
        .ok_or_else(|| AppError::not_found("group member not found"))?;
    if existing.role == GroupRole::GroupAdmin
        && role != GroupRole::GroupAdmin
        && state.store.count_group_admins(&group_id).await? <= 1
    {
        return Err(AppError::new(
            StatusCode::CONFLICT,
            "last_group_admin_protected",
            "cannot remove or demote the last group_admin",
        ));
    }
    state
        .store
        .update_group_member_role(&group_id, &principal_id, role)
        .await?;
    let request_id = v1_request_id();
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        None,
        &principal.principal_id,
        v1_actor_type(&principal),
        "group.member_role_changed",
        "group_member",
        &format!("{group_id}:{principal_id}"),
        "allow",
        200,
        Some(&request_id),
        client_ip(&headers).as_deref(),
        user_agent(&headers).as_deref(),
        json!({"from": existing.role.as_str(), "to": role.as_str()}),
    )
    .await;
    Ok(Json(json!({
        "group_id": group_id,
        "principal_id": principal_id,
        "role": role.as_str(),
        "request_id": request_id,
    })))
}

async fn remove_group_member_v1(
    State(state): State<AppState>,
    AxumPath((group_id, principal_id)): AxumPath<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Write,
        "group.member_remove",
        None,
    )
    .await?;
    v1_session_write_guard(&state, &headers, session)?;
    require_group_admin(
        &state,
        &principal,
        &group_id,
        "group.member_remove",
        &headers,
    )
    .await?;
    let existing = state
        .store
        .get_group_member(&group_id, &principal_id)
        .await?
        .ok_or_else(|| AppError::not_found("group member not found"))?;
    if existing.role == GroupRole::GroupAdmin
        && state.store.count_group_admins(&group_id).await? <= 1
    {
        return Err(AppError::new(
            StatusCode::CONFLICT,
            "last_group_admin_protected",
            "cannot remove the last group_admin",
        ));
    }
    state
        .store
        .remove_group_member(&group_id, &principal_id)
        .await?;
    let request_id = v1_request_id();
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        None,
        &principal.principal_id,
        v1_actor_type(&principal),
        "group.member_removed",
        "group_member",
        &format!("{group_id}:{principal_id}"),
        "allow",
        200,
        Some(&request_id),
        client_ip(&headers).as_deref(),
        user_agent(&headers).as_deref(),
        json!({"role": existing.role.as_str()}),
    )
    .await;
    Ok(Json(
        json!({"group_id": group_id, "removed": true, "request_id": request_id}),
    ))
}

fn sse_response(rx: mpsc::Receiver<Bytes>) -> Result<Response, AppError> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from_stream(
            ReceiverStream::new(rx).map(Ok::<_, std::convert::Infallible>),
        ))
        .map_err(|err| AppError::internal(format!("stream response failed: {err}")))
}

fn requested_space(principal: &Principal, requested: Option<&String>) -> Result<String, AppError> {
    let space = requested
        .cloned()
        .or_else(|| principal.allowed_space_ids.first().cloned())
        .ok_or_else(|| AppError::forbidden("no space available"))?;
    if !principal
        .allowed_space_ids
        .iter()
        .any(|allowed| allowed == &space)
    {
        return Err(AppError::forbidden("space is outside the principal scope"));
    }
    Ok(space)
}

async fn search_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let (principal, _session, _limits) =
        v1_auth(&state, &headers, TokenScope::Read, "search", None).await?;
    let text = query.get("q").cloned().unwrap_or_default();
    state.limits.check_query(&text, 8)?;
    let space = requested_space(&principal, query.get("space"))?;
    let mode = query
        .get("mode")
        .map(|value| SearchMode::parse(value))
        .transpose()
        .map_err(AppError::bad_request)?
        .unwrap_or(SearchMode::Auto);
    let k = query
        .get("k")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(10)
        .clamp(1, state.config.limits.max_top_k.max(1));
    let mut scoped = principal.clone();
    scoped.allowed_space_ids = vec![space];
    let policy = Policy::new(scoped, None)
        .map_err(AppError::bad_request)?
        .with_lang(query.get("lang").cloned());
    let mut tuning = SearchTuning::from_retrieval(&state.config.retrieval);
    tuning.lang = policy.lang.clone();
    tuning.billing_local_ledger = state.config.billing.engine.eq_ignore_ascii_case("local");
    let response = search_with_tuning(
        &state.store,
        &state.providers,
        &policy,
        &text,
        mode,
        k,
        &tuning,
    )
    .await?;
    Ok(Json(serde_json::to_value(response).unwrap_or(Value::Null)))
}

#[derive(Debug, Deserialize)]
struct AnswerBody {
    query: String,
    space: Option<String>,
    lang: Option<String>,
    conversation_id: Option<String>,
    #[serde(default)]
    top_k: Option<usize>,
}

async fn answer_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<AnswerBody>,
) -> Result<Json<Value>, AppError> {
    let (principal, _session, _limits) =
        v1_auth(&state, &headers, TokenScope::Read, "answer", None).await?;
    state.limits.check_query(&body.query, 8)?;
    let space = requested_space(&principal, body.space.as_ref())?;
    let mut scoped = principal.clone();
    scoped.allowed_space_ids = vec![space.clone()];
    let policy = Policy::new(scoped, None)
        .map_err(AppError::bad_request)?
        .with_lang(body.lang.clone());
    let mut tuning = SearchTuning::from_retrieval(&state.config.retrieval);
    tuning.lang = policy.lang.clone();
    tuning.billing_local_ledger = state.config.billing.engine.eq_ignore_ascii_case("local");
    let options = AnswerOptions {
        mode: SearchMode::Auto,
        limit: body.top_k.unwrap_or(8).clamp(1, 100),
        tuning,
    };
    let mut answer = build_answer_with_ledger(
        &state.store,
        &state.providers,
        &policy,
        &body.query,
        &options,
        state.config.billing.engine.eq_ignore_ascii_case("local"),
    )
    .await?;
    if let Some(conversation_id) = &body.conversation_id {
        let conversation = state
            .store
            .get_conversation(&principal.tenant_id, conversation_id)
            .await?
            .ok_or_else(|| AppError::not_found("conversation not found"))?;
        if conversation.owner_id != principal.principal_id || conversation.space_id != space {
            return Err(AppError::forbidden(
                "conversation belongs to another owner or space",
            ));
        }
        answer.conversation_id = Some(conversation_id.clone());
        answer.summary_kind = Some("dynamic".to_string());
    }
    Ok(Json(serde_json::to_value(answer).unwrap_or(Value::Null)))
}

async fn answer_stream_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Response, AppError> {
    let request: QueryRequest = serde_json::from_value(json!({
        "query": query.get("q").cloned().unwrap_or_default(),
        "space": query.get("space"),
        "lang": query.get("lang"),
        "mode": "answer",
        "conversation_id": query.get("conversation_id"),
        "top_k": query.get("k").and_then(|value| value.parse::<usize>().ok()),
    }))
    .map_err(|err| AppError::bad_request(format!("invalid query parameters: {err}")))?;
    let (tx, rx) = mpsc::channel::<Bytes>(64);
    let task_tx = tx.clone();
    tokio::spawn(async move {
        if let Err(err) =
            run_query(State(state), headers, Json(request), Some(task_tx.clone())).await
        {
            let _ = task_tx
                .send(sse_frame(
                    "error",
                    &json!({"code": err.code(), "message": err.message()}),
                ))
                .await;
        }
        drop(task_tx);
    });
    sse_response(rx)
}

fn page_json(view: &crate::store::PageView) -> Value {
    json!({
        "space": view.page.space_id,
        "slug": view.page.slug,
        "lang": view.page.lang,
        "title": view.page.title,
        "summary": view.page.summary,
        "version": view.page.version,
        "current_revision_id": view.page.current_revision_id,
        "content_hash": view.page.content_hash,
        "created_at": view.page.created_at,
        "updated_at": view.page.updated_at,
        "markdown": view.revision.markdown,
        "html": render_markdown(&view.revision.markdown),
        "author": view.revision.author,
        "revision_version": view.revision.version,
    })
}

async fn page_json_v1(
    State(state): State<AppState>,
    AxumPath((space, slug)): AxumPath<(String, String)>,
    headers: HeaderMap,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let (principal, _session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Read,
        "page.json.read",
        Some(&space),
    )
    .await?;
    let policy = Policy::new(principal.clone(), None)
        .map_err(AppError::bad_request)?
        .with_lang(query.get("lang").cloned());
    let view = state
        .store
        .get_page(
            &policy,
            &space,
            &slug,
            query.get("lang").map(String::as_str),
        )
        .await?
        .ok_or_else(|| AppError::not_found("page not found"))?;
    Ok(Json(page_json(&view)))
}

async fn page_history_v1(
    State(state): State<AppState>,
    AxumPath((space, slug)): AxumPath<(String, String)>,
    headers: HeaderMap,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let (principal, _session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Read,
        "page.json.history",
        Some(&space),
    )
    .await?;
    let revisions = state
        .store
        .list_revisions(
            &principal.tenant_id,
            &space,
            &slug,
            query.get("lang").map(String::as_str),
        )
        .await?;
    if query.get("include_markdown").map(String::as_str) == Some("true") {
        let mut items = Vec::new();
        for revision in &revisions {
            let full = state
                .store
                .get_revision(&principal.tenant_id, &revision.revision_id)
                .await?;
            items.push(json!({
                "revision_id": revision.revision_id,
                "version": revision.version,
                "author": revision.author,
                "created_at": revision.created_at,
                "content_hash": revision.content_hash,
                "markdown": full.map(|record| record.markdown),
            }));
        }
        return Ok(Json(
            json!({"space": space, "slug": slug, "revisions": items}),
        ));
    }
    Ok(Json(
        json!({"space": space, "slug": slug, "revisions": revisions}),
    ))
}

#[derive(Debug, Deserialize)]
struct PageJsonBody {
    markdown: String,
    title: Option<String>,
    lang: Option<String>,
    summary: Option<String>,
}

async fn page_save_json_v1(
    State(state): State<AppState>,
    AxumPath((space, slug)): AxumPath<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<PageJsonBody>,
) -> Result<Json<Value>, AppError> {
    let (principal, session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Write,
        "page.json.save",
        Some(&space),
    )
    .await?;
    v1_session_write_guard(&state, &headers, session)?;
    require_space_role(
        &state,
        &principal,
        &space,
        SpaceRole::Editor,
        "page.json.save",
        &headers,
    )
    .await?;
    validate_slug(&slug)?;
    if body.markdown.trim().is_empty() {
        return Err(AppError::bad_request("markdown must not be empty"));
    }
    let lang = body.lang.clone().unwrap_or_else(|| "zh".to_string());
    let if_match = headers
        .get(header::IF_MATCH)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .ok_or_else(|| AppError::bad_request("If-Match header is required"))?;
    let policy = Policy::new(principal.clone(), None)
        .map_err(AppError::bad_request)?
        .with_lang(Some(lang.clone()));
    let existing = state
        .store
        .get_page(&policy, &space, &slug, Some(&lang))
        .await?;
    let current_revision = existing
        .as_ref()
        .map(|view| view.page.current_revision_id.clone());
    if if_match != "*" && current_revision.as_deref() != Some(if_match.as_str()) {
        return Err(AppError::new(
            StatusCode::CONFLICT,
            "revision_conflict",
            "If-Match revision does not match the current revision",
        ));
    }
    if existing.is_none() && if_match != "*" {
        return Err(AppError::not_found("page not found"));
    }
    let acl = match &existing {
        Some(view) => {
            Acl::from_json(&view.page.acl_json).unwrap_or_else(|_| page_acl(&state, &principal))
        }
        None => page_acl(&state, &principal),
    };
    if state
        .store
        .get_space(&principal.tenant_id, &space)
        .await?
        .is_none()
    {
        state
            .store
            .ensure_space(&crate::store::SpaceRecord::new(
                &principal.tenant_id,
                &space,
                &space,
                &acl,
                &principal.principal_id,
                &now_rfc3339(),
            ))
            .await?;
    }
    let now = now_rfc3339();
    let content_hash = crate::util::sha256_hex(body.markdown.as_bytes());
    let version = existing
        .as_ref()
        .map(|view| view.page.version + 1)
        .unwrap_or(1);
    let revision_id = crate::ingest::doc_id::derive_revision_id(
        &principal.tenant_id,
        &space,
        &slug,
        &lang,
        &content_hash,
        version,
    );
    let title = body.title.clone().unwrap_or_else(|| slug.clone());
    let page = crate::store::PageRecord {
        tenant_id: principal.tenant_id.clone(),
        space_id: space.clone(),
        slug: slug.clone(),
        lang: lang.clone(),
        title: title.clone(),
        summary: body
            .summary
            .clone()
            .unwrap_or_else(|| body.markdown.chars().take(200).collect()),
        order_index: existing
            .as_ref()
            .map(|view| view.page.order_index)
            .unwrap_or(0),
        current_revision_id: revision_id.clone(),
        content_hash: content_hash.clone(),
        acl_json: acl.to_json(),
        acl_principals: acl.principals_key(),
        valid_from: now.clone(),
        valid_to: None,
        version,
        superseded_by: None,
        created_at: existing
            .as_ref()
            .map(|view| view.page.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now.clone(),
    };
    let revision = crate::store::RevisionRecord {
        tenant_id: principal.tenant_id.clone(),
        revision_id: revision_id.clone(),
        space_id: space.clone(),
        slug: slug.clone(),
        lang: lang.clone(),
        title,
        markdown: body.markdown.clone(),
        content_hash,
        author: principal.principal_id.clone(),
        created_at: now.clone(),
        version,
        acl_json: acl.to_json(),
        acl_principals: acl.principals_key(),
        valid_from: now,
        valid_to: None,
        superseded_by: None,
        source_url: None,
        source_revision: None,
    };
    let space_record = state
        .store
        .get_space(&principal.tenant_id, &space)
        .await?
        .ok_or_else(|| AppError::not_found("space not found"))?;
    let drafts = chunker::chunk_text(&body.markdown, chunker::DEFAULT_CHUNK_CHARS);
    let quota_delta = crate::quota::QuotaDelta {
        source_bytes: body.markdown.len() as i64,
        index_bytes: drafts.iter().map(|draft| draft.text.len() as i64).sum(),
        history_bytes: 0,
        documents: 1,
        chunks: drafts.len() as i64,
    };
    let _quota_guard = crate::quota::write_guard().await;
    crate::quota::reserve(&state.store, &space_record, &quota_delta)
        .await
        .map_err(|err| err.to_app_error())?;
    if let Err(err) = state.store.save_page(&page, &revision).await {
        crate::quota::release(&state.store, &space_record, &quota_delta).await;
        return Err(err.into());
    }
    if let Err(err) = index_wiki_document(
        &state,
        &principal,
        WikiIndexInput {
            space: &space,
            slug: &slug,
            lang: &lang,
            title: &page.title,
            markdown: &body.markdown,
            acl: &acl,
        },
    )
    .await
    {
        crate::quota::release(&state.store, &space_record, &quota_delta).await;
        return Err(err);
    }
    let _ = crate::quota::refresh(&state.store, &principal.tenant_id, &space).await;
    Ok(Json(json!({
        "space": space, "slug": slug, "lang": lang, "version": version,
        "revision_id": revision_id, "content_hash": page.content_hash, "title": page.title,
    })))
}

async fn page_delete_json_v1(
    State(state): State<AppState>,
    AxumPath((space, slug)): AxumPath<(String, String)>,
    headers: HeaderMap,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let (principal, session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Write,
        "page.json.delete",
        Some(&space),
    )
    .await?;
    v1_session_write_guard(&state, &headers, session)?;
    require_space_role(
        &state,
        &principal,
        &space,
        SpaceRole::SpaceOwner,
        "page.json.delete",
        &headers,
    )
    .await?;
    let lang = query
        .get("lang")
        .cloned()
        .unwrap_or_else(|| "zh".to_string());
    let removed = state
        .store
        .archive_page(&principal.tenant_id, &space, &slug, &lang)
        .await?;
    if !removed {
        return Err(AppError::not_found("page not found"));
    }
    let logical = format!("wiki/{space}/{lang}/{slug}.md");
    let canonical = crate::ingest::doc_id::canonical_relative_path(&logical)
        .map_err(|err| AppError::internal(format!("canonical path failed: {err}")))?;
    state
        .store
        .archive_document_path(&principal.tenant_id, &space, &canonical)
        .await
        .map_err(|err| AppError::internal(format!("index archive failed: {err}")))?;
    Ok(Json(
        json!({"space": space, "slug": slug, "archived": true}),
    ))
}

async fn page_restore_json_v1(
    State(state): State<AppState>,
    AxumPath((space, slug)): AxumPath<(String, String)>,
    headers: HeaderMap,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let (principal, session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Write,
        "page.json.restore",
        Some(&space),
    )
    .await?;
    v1_session_write_guard(&state, &headers, session)?;
    require_space_role(
        &state,
        &principal,
        &space,
        SpaceRole::SpaceOwner,
        "page.json.restore",
        &headers,
    )
    .await?;
    let lang = query
        .get("lang")
        .cloned()
        .unwrap_or_else(|| "zh".to_string());
    let restored = state
        .store
        .restore_page(&principal.tenant_id, &space, &slug, &lang)
        .await?;
    if !restored {
        return Err(AppError::not_found("archived page not found"));
    }
    let logical = format!("wiki/{space}/{lang}/{slug}.md");
    let canonical = crate::ingest::doc_id::canonical_relative_path(&logical)
        .map_err(|err| AppError::internal(format!("canonical path failed: {err}")))?;
    state
        .store
        .restore_document_path(&principal.tenant_id, &space, &canonical)
        .await
        .map_err(|err| AppError::internal(format!("index restore failed: {err}")))?;
    Ok(Json(
        json!({"space": space, "slug": slug, "restored": true}),
    ))
}

fn principal_json(principal: &Principal) -> Value {
    json!({
        "user_id": principal.principal_id,
        "display_name": principal.display_name,
        "tenant_id": principal.tenant_id,
        "allowed_space_ids": principal.allowed_space_ids,
        "kind": principal.kind,
        "scope": principal.scope.as_str(),
        "tenant_role": principal.tenant_role.as_str(),
        "token_id": principal.token_id,
    })
}

fn verify_api_csrf(
    state: &AppState,
    headers: &HeaderMap,
    token: Option<&str>,
) -> Result<(), AppError> {
    check_origin(headers)?;
    if !state.config.auth.csrf {
        return Ok(());
    }
    let cookie = cookie_value(headers, "lkb_csrf")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::forbidden("missing CSRF cookie"))?;
    let token = token
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::forbidden("missing CSRF token"))?;
    if !constant_time_eq(cookie.as_bytes(), token.as_bytes()) {
        return Err(AppError::forbidden("CSRF token mismatch"));
    }
    Ok(())
}

async fn auth_csrf_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    check_origin(&headers)?;
    let (token, cookie) = csrf_token_for(&state, &headers);
    let mut response = Json(json!({"csrf_token": token})).into_response();
    if let Some(cookie) = cookie {
        append_set_cookie(&mut response, cookie);
    }
    Ok(response)
}

#[derive(Debug, Deserialize)]
struct ApiLoginBody {
    username: String,
    password: String,
    #[serde(default)]
    csrf_token: Option<String>,
}

async fn auth_login_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ApiLoginBody>,
) -> Result<Response, AppError> {
    let header_token = headers
        .get("x-csrf-token")
        .and_then(|value| value.to_str().ok());
    verify_api_csrf(
        &state,
        &headers,
        body.csrf_token.as_deref().or(header_token),
    )?;
    if state.config.auth.mode == AuthMode::None {
        return Err(AppError::forbidden("authentication is disabled"));
    }
    let key = login_rate_key(&headers, &body.username);
    if !state.limiter.allowed(&key) {
        return Err(AppError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "too many failed login attempts",
        ));
    }
    let user = match state
        .store
        .get_user(&state.config.storage.tenant_id, &body.username)
        .await?
    {
        Some(user) if !user.disabled => user,
        _ => {
            state.limiter.record_failure(&key);
            crate::audit::record(
                &state.store,
                &state.config.storage.tenant_id,
                None,
                "anonymous",
                "anonymous",
                "auth.login",
                "user",
                &body.username,
                "denied",
                401,
                None,
                client_ip(&headers).as_deref(),
                user_agent(&headers).as_deref(),
                json!({"reason": "invalid_credentials"}),
            )
            .await;
            return Err(AppError::unauthorized("invalid username or password"));
        }
    };
    let parsed = PasswordHash::new(&user.password_hash)
        .map_err(|_| AppError::internal("stored password hash is invalid"))?;
    if Argon2::default()
        .verify_password(body.password.as_bytes(), &parsed)
        .is_err()
    {
        state.limiter.record_failure(&key);
        crate::audit::record(
            &state.store,
            &state.config.storage.tenant_id,
            None,
            "anonymous",
            "anonymous",
            "auth.login",
            "user",
            &body.username,
            "denied",
            401,
            None,
            client_ip(&headers).as_deref(),
            user_agent(&headers).as_deref(),
            json!({"reason": "invalid_credentials"}),
        )
        .await;
        return Err(AppError::unauthorized("invalid username or password"));
    }
    state.limiter.clear(&key);
    if let Some(old) = session_hash(&headers, &state) {
        let _ = state.store.delete_session(&old).await;
    }
    let raw_session = random_hex(32);
    let now = chrono::Utc::now();
    let expires = now + chrono::Duration::hours(state.config.auth.session_ttl_hours as i64);
    state
        .store
        .create_session(&crate::store::SessionRecord {
            session_hash: sha256_hex_str(&raw_session),
            tenant_id: state.config.storage.tenant_id.clone(),
            user_id: user.user_id.clone(),
            created_at: now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            expires_at: expires.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        })
        .await?;
    let mut principal = Principal {
        principal_id: user.user_id.clone(),
        display_name: user.display_name.clone(),
        tenant_id: state.config.storage.tenant_id.clone(),
        allowed_space_ids: Vec::new(),
        groups: Vec::new(),
        allow_all: false,
        kind: crate::governance::PrincipalKind::Human,
        scope: if user.role.is_admin() {
            TokenScope::Admin
        } else {
            TokenScope::Write
        },
        tenant_role: user.role,
        token_id: None,
    };
    principal.allowed_space_ids = crate::access::allowed_space_ids(&state.store, &principal)
        .await
        .map_err(AppError::from)?;
    let secure = state.config.cookie_secure_for_host(host_header(&headers));
    let cookie = build_cookie(
        &state.config.auth.cookie_name,
        &raw_session,
        state.config.auth.session_ttl_hours as i64 * 3600,
        true,
        secure,
    );
    let mut response = Json(json!({
        "principal": principal_json(&principal),
        "features": state.config.features().to_json(),
    }))
    .into_response();
    append_set_cookie(&mut response, cookie);
    Ok(response)
}

async fn auth_me_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    check_origin(&headers)?;
    let principal = authenticate(&state, &headers).await?;
    Ok(Json(json!({
        "principal": principal_json(&principal),
        "features": state.config.features().to_json(),
    })))
}

async fn auth_logout_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let header_token = headers
        .get("x-csrf-token")
        .and_then(|value| value.to_str().ok());
    verify_api_csrf(&state, &headers, header_token)?;
    if let Some(hash) = session_hash(&headers, &state) {
        state.store.delete_session(&hash).await?;
    }
    let secure = state.config.cookie_secure_for_host(host_header(&headers));
    let mut response = Json(json!({"logged_out": true})).into_response();
    append_set_cookie(
        &mut response,
        build_cookie(&state.config.auth.cookie_name, "", 0, true, secure),
    );
    Ok(response)
}

async fn pages_list_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let (principal, _session, _limits) =
        v1_auth(&state, &headers, TokenScope::Read, "pages.list", None).await?;
    let space = requested_space(&principal, query.get("space"))?;
    let mut scoped = principal.clone();
    scoped.allowed_space_ids = vec![space.clone()];
    let policy = Policy::new(scoped, None).map_err(AppError::bad_request)?;
    let limit = query
        .get("limit")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(50)
        .clamp(1, 200);
    let lang = query.get("lang").cloned();
    let needle = query.get("q").map(|value| value.to_lowercase());
    let cursor = query.get("cursor").cloned();
    let mut pages = state.store.list_page_records(&policy, 500).await?;
    pages.retain(|page| page.space_id == space);
    if let Some(lang) = &lang {
        pages.retain(|page| &page.lang == lang);
    }
    if let Some(needle) = &needle {
        pages.retain(|page| {
            page.slug.to_lowercase().contains(needle.as_str())
                || page.title.to_lowercase().contains(needle.as_str())
        });
    }
    pages.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.slug.cmp(&b.slug))
    });
    if let Some(cursor) = &cursor {
        if let Some((updated_at, slug)) = cursor.split_once('|') {
            pages.retain(|page| {
                page.updated_at.as_str() < updated_at
                    || (page.updated_at.as_str() == updated_at && page.slug.as_str() > slug)
            });
        }
    }
    let next_cursor = if pages.len() > limit {
        pages
            .get(limit - 1)
            .map(|page| format!("{}|{}", page.updated_at, page.slug))
    } else {
        None
    };
    pages.truncate(limit);
    let items: Vec<Value> = pages
        .iter()
        .map(|page| {
            json!({
                "slug": page.slug,
                "title": page.title,
                "lang": page.lang,
                "order_index": page.order_index,
                "updated_at": page.updated_at,
                "current_revision_id": page.current_revision_id,
                "version": page.version,
                "summary": page.summary,
            })
        })
        .collect();
    Ok(Json(json!({"pages": items, "next_cursor": next_cursor})))
}

async fn page_revision_v1(
    State(state): State<AppState>,
    AxumPath((space, slug, revision_id)): AxumPath<(String, String, String)>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, _session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Read,
        "page.revision.read",
        Some(&space),
    )
    .await?;
    let policy = Policy::new(principal.clone(), None).map_err(AppError::bad_request)?;
    state
        .store
        .get_page(&policy, &space, &slug, None)
        .await?
        .ok_or_else(|| AppError::not_found("page not found"))?;
    let revision = state
        .store
        .get_revision(&principal.tenant_id, &revision_id)
        .await?
        .ok_or_else(|| AppError::not_found("revision not found"))?;
    if revision.space_id != space || revision.slug != slug {
        return Err(AppError::not_found("revision not found"));
    }
    Ok(Json(json!({
        "revision_id": revision.revision_id,
        "space": revision.space_id,
        "slug": revision.slug,
        "lang": revision.lang,
        "title": revision.title,
        "markdown": revision.markdown,
        "author": revision.author,
        "version": revision.version,
        "created_at": revision.created_at,
        "content_hash": revision.content_hash,
    })))
}

async fn restore_conversation_v1(
    State(state): State<AppState>,
    AxumPath(conversation_id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Read,
        "conversation.restore",
        None,
    )
    .await?;
    v1_session_write_guard(&state, &headers, session)?;
    let conversation = state
        .store
        .get_conversation(&principal.tenant_id, &conversation_id)
        .await?
        .ok_or_else(|| AppError::not_found("conversation not found"))?;
    if conversation.owner_id != principal.principal_id && !principal.tenant_role.is_admin() {
        return Err(AppError::forbidden("conversation belongs to another owner"));
    }
    let restored = state
        .store
        .restore_conversation(&principal.tenant_id, &conversation_id)
        .await?;
    Ok(Json(
        json!({"conversation_id": conversation_id, "restored": restored, "status": "active"}),
    ))
}

fn billing_error(err: crate::billing::BillingError) -> AppError {
    AppError::new(err.status(), err.code(), err.to_string())
}

fn payment_error(err: crate::payment::PaymentError) -> AppError {
    use axum::http::StatusCode as S;
    let status = match err {
        crate::payment::PaymentError::NotConfigured => S::SERVICE_UNAVAILABLE,
        crate::payment::PaymentError::Unavailable => S::SERVICE_UNAVAILABLE,
        crate::payment::PaymentError::Invalid(_) => S::BAD_REQUEST,
    };
    AppError::new(status, err.code(), err.to_string())
}

fn local_billing(state: &AppState) -> crate::billing::Billing {
    crate::billing::Billing::new(state.store.clone(), state.config.billing.clone())
}

async fn ensure_plans(state: &AppState) -> Result<(), AppError> {
    if state.store.billing_list_plans().await?.is_empty() {
        crate::billing::seed_default_plans(&state.store, &state.config.billing.currency)
            .await
            .map_err(billing_error)?;
    }
    Ok(())
}

async fn billing_plans_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (_principal, _session, _limits) =
        v1_auth(&state, &headers, TokenScope::Read, "billing.plans", None).await?;
    require_feature(&state, Feature::Billing)?;
    ensure_plans(&state).await?;
    let plans = state.store.billing_list_plans().await?;
    Ok(Json(json!({"plans": plans})))
}

async fn billing_wallet_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, _session, _limits) =
        v1_auth(&state, &headers, TokenScope::Read, "billing.wallet", None).await?;
    require_feature(&state, Feature::Billing)?;
    let billing = local_billing(&state);
    let wallet = billing
        .wallet(&principal.tenant_id)
        .await
        .map_err(billing_error)?;
    let entitlement = billing.entitlement(&principal.tenant_id).await;
    Ok(Json(json!({"wallet": wallet, "entitlement": entitlement})))
}

async fn billing_ledger_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let (principal, _session, _limits) =
        v1_auth(&state, &headers, TokenScope::Read, "billing.ledger", None).await?;
    require_feature(&state, Feature::Billing)?;
    let limit = query
        .get("limit")
        .and_then(|value| value.parse().ok())
        .unwrap_or(100);
    let entries = state
        .store
        .billing_list_ledger(&principal.tenant_id, limit)
        .await?;
    Ok(Json(json!({"entries": entries})))
}

async fn billing_invoices_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let (principal, _session, _limits) =
        v1_auth(&state, &headers, TokenScope::Read, "billing.invoices", None).await?;
    require_feature(&state, Feature::Billing)?;
    let limit = query
        .get("limit")
        .and_then(|value| value.parse().ok())
        .unwrap_or(100);
    let invoices = state
        .store
        .billing_list_invoices(&principal.tenant_id, limit)
        .await?;
    Ok(Json(json!({"invoices": invoices})))
}

#[derive(Debug, Deserialize)]
struct TopupBody {
    amount_micros: i64,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    idempotency_key: Option<String>,
}

async fn billing_topup_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<TopupBody>,
) -> Result<Json<Value>, AppError> {
    let (principal, session, _limits) =
        v1_auth(&state, &headers, TokenScope::Write, "billing.topup", None).await?;
    v1_session_write_guard(&state, &headers, session)?;
    require_feature(&state, Feature::Billing)?;
    if !principal.tenant_role.is_admin() {
        return Err(AppError::forbidden(
            "tenant admin required for manual topups",
        ));
    }
    let channel = body.channel.clone().unwrap_or_else(|| "manual".to_string());
    if !matches!(channel.as_str(), "manual" | "mock") {
        return Err(AppError::bad_request(
            "manual topup channel must be manual or mock",
        ));
    }
    let key = body
        .idempotency_key
        .clone()
        .unwrap_or_else(|| format!("manual:{}", crate::util::random_hex(12)));
    let billing = local_billing(&state);
    let topup = billing
        .topup(
            &principal.tenant_id,
            body.amount_micros,
            &channel,
            None,
            &key,
        )
        .await
        .map_err(billing_error)?;
    let settled = billing.settle_topup(&topup).await.map_err(billing_error)?;
    let wallet = billing
        .wallet(&principal.tenant_id)
        .await
        .map_err(billing_error)?;
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        None,
        &principal.principal_id,
        "user",
        "billing.topup",
        "topup",
        &settled.topup_id,
        "allow",
        201,
        None,
        client_ip(&headers).as_deref(),
        user_agent(&headers).as_deref(),
        json!({"amount_micros": body.amount_micros, "channel": channel}),
    )
    .await;
    Ok(Json(json!({"topup": settled, "wallet": wallet})))
}

#[derive(Debug, Deserialize)]
struct CheckoutBody {
    #[serde(default, alias = "plan")]
    plan_id: Option<String>,
    #[serde(default, alias = "amount_minor")]
    amount_micros: Option<i64>,
    #[serde(default)]
    currency: Option<String>,
}

async fn billing_checkout_v2(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CheckoutBody>,
) -> Result<Json<Value>, AppError> {
    let (principal, session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Write,
        "billing.checkout",
        None,
    )
    .await?;
    v1_session_write_guard(&state, &headers, session)?;
    require_feature(&state, Feature::Billing)?;
    ensure_plans(&state).await?;
    let plan_id = body
        .plan_id
        .clone()
        .ok_or_else(|| AppError::bad_request("plan_id is required"))?;
    let plan = state
        .store
        .billing_get_plan(&plan_id)
        .await?
        .ok_or_else(|| AppError::not_found("plan not found"))?;
    let amount = body.amount_micros.unwrap_or(plan.price_micros);
    if amount <= 0 {
        return Err(AppError::bad_request("plan is free; no checkout required"));
    }
    let provider =
        crate::payment::provider_for_billing(&state.config.payment, &state.config.billing)
            .map_err(payment_error)?;
    let session = provider
        .create_checkout(&crate::payment::CheckoutRequest {
            tenant_id: principal.tenant_id.clone(),
            space_id: None,
            plan: plan.plan_id.clone(),
            amount_minor: amount,
            currency: body
                .currency
                .clone()
                .unwrap_or_else(|| plan.currency.clone()),
        })
        .await
        .map_err(payment_error)?;
    let key = format!("{}:{}", session.provider, session.session_id);
    let billing = local_billing(&state);
    let topup = billing
        .topup(
            &principal.tenant_id,
            amount,
            provider.name(),
            Some(&session.session_id),
            &key,
        )
        .await
        .map_err(billing_error)?;
    Ok(Json(json!({
        "provider": provider.name(),
        "session_id": session.session_id,
        "checkout_url": session.checkout_url,
        "topup_id": topup.topup_id,
        "plan_id": plan.plan_id,
        "amount_micros": amount,
    })))
}

async fn billing_webhook_v1(
    State(state): State<AppState>,
    AxumPath(provider_name): AxumPath<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, AppError> {
    if !state.config.billing.enabled {
        return Err(AppError::new(
            StatusCode::FORBIDDEN,
            "billing_not_enabled",
            "billing is not enabled",
        ));
    }
    let mut payment_config = state.config.payment.clone();
    payment_config.provider = provider_name.clone();
    let provider = crate::payment::provider_for_billing(&payment_config, &state.config.billing)
        .map_err(payment_error)?;
    let header_pairs: Vec<(String, String)> = headers
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_string(),
                value.to_str().unwrap_or("").to_string(),
            )
        })
        .collect();
    let event = provider
        .verify_webhook(&crate::payment::WebhookRequest {
            headers: header_pairs,
            body: body.to_vec(),
        })
        .await
        .map_err(payment_error)?;
    let payload: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let record = crate::billing::payment_event_record(
        &event.provider,
        &event.event_id,
        &event.event_type,
        &payload,
    );
    if !state.store.billing_insert_payment_event(&record).await? {
        return Ok(Json(
            json!({"received": true, "duplicate": true, "event_id": event.event_id}),
        ));
    }
    let tenant = event
        .tenant_id
        .clone()
        .ok_or_else(|| AppError::bad_request("webhook event is missing tenant metadata"))?;
    let billing = local_billing(&state);
    let mut handled = "ignored";
    match event.event_type.as_str() {
        "checkout.session.completed" | "payment_intent.succeeded" | "topup.paid" | "mock.paid" => {
            let key = event
                .provider_ref
                .as_ref()
                .map(|reference| format!("{}:{}", event.provider, reference))
                .unwrap_or_else(|| format!("{}:{}", event.provider, event.event_id));
            let amount = event.amount_minor.unwrap_or(0);
            let topup = match state.store.billing_get_topup_by_key(&tenant, &key).await? {
                Some(topup) => topup,
                None => billing
                    .topup(
                        &tenant,
                        amount,
                        &event.provider,
                        event.provider_ref.as_deref(),
                        &key,
                    )
                    .await
                    .map_err(billing_error)?,
            };
            billing.settle_topup(&topup).await.map_err(billing_error)?;
            handled = "paid";
        }
        "charge.refunded" | "refund.created" => {
            let amount = event.amount_minor.unwrap_or(0);
            billing
                .reverse_topup_refund(
                    &tenant,
                    amount,
                    &event.event_type,
                    &format!("refund:{}", event.event_id),
                )
                .await
                .map_err(billing_error)?;
            handled = "refunded";
        }
        _ => {}
    }
    state
        .store
        .billing_mark_payment_event(&record.provider_event_id, "processed")
        .await?;
    Ok(Json(json!({
        "received": true,
        "duplicate": false,
        "event_id": event.event_id,
        "handled": handled,
    })))
}

#[derive(Debug, Deserialize)]
struct SubscribeBody {
    plan_id: String,
    #[serde(default)]
    trial: Option<bool>,
    #[serde(default)]
    trial_days: Option<u32>,
}

async fn billing_subscribe_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SubscribeBody>,
) -> Result<Json<Value>, AppError> {
    let (principal, session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Write,
        "billing.subscribe",
        None,
    )
    .await?;
    v1_session_write_guard(&state, &headers, session)?;
    require_feature(&state, Feature::Billing)?;
    if !principal.tenant_role.is_admin() {
        return Err(AppError::forbidden("tenant owner/admin required"));
    }
    ensure_plans(&state).await?;
    let trial_days = body
        .trial_days
        .or_else(|| {
            body.trial.map(|trial| {
                if trial {
                    state.config.billing.trial_days
                } else {
                    0
                }
            })
        })
        .unwrap_or(0);
    let billing = local_billing(&state);
    let subscription = billing
        .subscribe(&principal.tenant_id, &body.plan_id, trial_days)
        .await
        .map_err(billing_error)?;
    let entitlement = billing.entitlement(&principal.tenant_id).await;
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        None,
        &principal.principal_id,
        "user",
        "billing.subscribe",
        "subscription",
        &subscription.subscription_id,
        "allow",
        200,
        None,
        client_ip(&headers).as_deref(),
        user_agent(&headers).as_deref(),
        json!({"plan_id": body.plan_id, "trial_days": trial_days}),
    )
    .await;
    Ok(Json(
        json!({"subscription": subscription, "entitlement": entitlement}),
    ))
}

async fn billing_subscription_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, _session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Read,
        "billing.subscription",
        None,
    )
    .await?;
    require_feature(&state, Feature::Billing)?;
    let billing = local_billing(&state);
    Ok(Json(json!({
        "subscription": state.store.billing_get_subscription(&principal.tenant_id).await?,
        "entitlement": billing.entitlement(&principal.tenant_id).await,
    })))
}

#[derive(Debug, Deserialize)]
struct TransitionBody {
    status: String,
}

async fn billing_subscription_patch_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<TransitionBody>,
) -> Result<Json<Value>, AppError> {
    let (principal, session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Write,
        "billing.subscription.update",
        None,
    )
    .await?;
    v1_session_write_guard(&state, &headers, session)?;
    require_feature(&state, Feature::Billing)?;
    if !principal.tenant_role.is_admin() {
        return Err(AppError::forbidden("tenant owner/admin required"));
    }
    let status = crate::billing::SubscriptionStatus::parse(&body.status).map_err(billing_error)?;
    let billing = local_billing(&state);
    let subscription = billing
        .transition(&principal.tenant_id, status)
        .await
        .map_err(billing_error)?;
    let entitlement = billing.entitlement(&principal.tenant_id).await;
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        None,
        &principal.principal_id,
        "user",
        "billing.subscription.transition",
        "subscription",
        &subscription.subscription_id,
        "allow",
        200,
        None,
        client_ip(&headers).as_deref(),
        user_agent(&headers).as_deref(),
        json!({"status": status.as_str()}),
    )
    .await;
    Ok(Json(
        json!({"subscription": subscription, "entitlement": entitlement}),
    ))
}

async fn billing_entitlement_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, _session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Read,
        "billing.entitlement",
        None,
    )
    .await?;
    require_feature(&state, Feature::Billing)?;
    let billing = local_billing(&state);
    Ok(Json(
        json!({"entitlement": billing.entitlement(&principal.tenant_id).await}),
    ))
}

fn redact_connector_config(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| {
                    let lowered = key.to_ascii_lowercase();
                    let secret = lowered.contains("secret")
                        || lowered.contains("token")
                        || lowered.contains("api_key")
                        || lowered.contains("password")
                        || lowered.contains("authorization");
                    (
                        key.clone(),
                        if secret {
                            json!("***")
                        } else {
                            redact_connector_config(value)
                        },
                    )
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redact_connector_config).collect()),
        other => other.clone(),
    }
}

fn connector_json(record: &crate::store::ConnectorRecord) -> Value {
    let config: Value = serde_json::from_str(&record.config_json).unwrap_or(Value::Null);
    let secret_configured = record
        .secret_env
        .as_ref()
        .and_then(|name| std::env::var(name).ok())
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    json!({
        "connector_id": record.connector_id,
        "space_id": record.space_id,
        "kind": record.kind,
        "name": record.name,
        "config": redact_connector_config(&config),
        "secret_env": record.secret_env,
        "secret_configured": secret_configured,
        "status": record.status,
        "last_sync_at": record.last_sync_at,
        "created_at": record.created_at,
        "updated_at": record.updated_at,
    })
}

#[derive(Debug, Deserialize)]
struct ConnectorBody {
    #[serde(default)]
    connector_id: Option<String>,
    space_id: String,
    kind: String,
    name: String,
    #[serde(default)]
    config: Option<Value>,
    #[serde(default)]
    config_json: Option<String>,
    #[serde(default)]
    secret_env: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

async fn connectors_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, _guard) = admin_auth(&state, &headers, "connectors.list").await?;
    require_feature(&state, Feature::Connectors)?;
    let connectors = state.store.connector_list(&principal.tenant_id).await?;
    Ok(Json(
        json!({"connectors": connectors.iter().map(connector_json).collect::<Vec<_>>()}),
    ))
}

async fn connectors_create_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ConnectorBody>,
) -> Result<Json<Value>, AppError> {
    let (principal, _guard) = admin_auth(&state, &headers, "connectors.create").await?;
    require_feature(&state, Feature::Connectors)?;
    if !crate::connectors::SUPPORTED_KINDS.contains(&body.kind.as_str()) {
        return Err(AppError::bad_request(format!(
            "unsupported connector kind {:?}",
            body.kind
        )));
    }
    state
        .store
        .get_space(&principal.tenant_id, &body.space_id)
        .await?
        .ok_or_else(|| AppError::not_found("space not found"))?;
    let config_json = match (body.config, body.config_json) {
        (Some(config), _) => config.to_string(),
        (None, Some(config_json)) => config_json,
        (None, None) => "{}".to_string(),
    };
    serde_json::from_str::<Value>(&config_json)
        .map_err(|err| AppError::bad_request(format!("config must be valid JSON: {err}")))?;
    let now = now_rfc3339();
    let record = crate::store::ConnectorRecord {
        connector_id: body
            .connector_id
            .unwrap_or_else(|| format!("conn_{}", random_hex(10))),
        tenant_id: principal.tenant_id.clone(),
        space_id: body.space_id,
        kind: body.kind,
        name: body.name,
        config_json,
        secret_env: body.secret_env,
        status: body.status.unwrap_or_else(|| "active".to_string()),
        last_sync_at: None,
        created_at: now.clone(),
        updated_at: now,
    };
    crate::connectors::from_record(&record)
        .map_err(|err| AppError::bad_request(err.to_string()))?;
    state.store.connector_upsert(&record).await?;
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        Some(&record.space_id),
        &principal.principal_id,
        "user",
        "connector.create",
        "connector",
        &record.connector_id,
        "allow",
        201,
        None,
        client_ip(&headers).as_deref(),
        user_agent(&headers).as_deref(),
        json!({"kind": record.kind, "name": record.name}),
    )
    .await;
    Ok(Json(json!({"connector": connector_json(&record)})))
}

async fn connectors_patch_v1(
    State(state): State<AppState>,
    AxumPath(connector_id): AxumPath<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let (principal, _guard) = admin_auth(&state, &headers, "connectors.update").await?;
    require_feature(&state, Feature::Connectors)?;
    let mut record = state
        .store
        .connector_get(&principal.tenant_id, &connector_id)
        .await?
        .ok_or_else(|| AppError::not_found("connector not found"))?;
    if let Some(name) = body.get("name").and_then(Value::as_str) {
        record.name = name.to_string();
    }
    if let Some(space_id) = body.get("space_id").and_then(Value::as_str) {
        record.space_id = space_id.to_string();
    }
    if let Some(status) = body.get("status").and_then(Value::as_str) {
        record.status = status.to_string();
    }
    if let Some(secret_env) = body.get("secret_env") {
        record.secret_env = secret_env.as_str().map(str::to_string);
    }
    if let Some(config) = body.get("config").or_else(|| body.get("config_json")) {
        record.config_json = if let Some(text) = config.as_str() {
            text.to_string()
        } else {
            config.to_string()
        };
    }
    record.updated_at = now_rfc3339();
    crate::connectors::from_record(&record)
        .map_err(|err| AppError::bad_request(err.to_string()))?;
    state.store.connector_upsert(&record).await?;
    audit_admin_write(
        &state,
        &principal,
        "connector.update",
        "connector",
        &record.connector_id,
    )
    .await;
    Ok(Json(json!({"connector": connector_json(&record)})))
}

async fn connectors_delete_v1(
    State(state): State<AppState>,
    AxumPath(connector_id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, _guard) = admin_auth(&state, &headers, "connectors.delete").await?;
    require_feature(&state, Feature::Connectors)?;
    let removed = state
        .store
        .connector_delete(&principal.tenant_id, &connector_id)
        .await?;
    if removed {
        audit_admin_write(
            &state,
            &principal,
            "connector.delete",
            "connector",
            &connector_id,
        )
        .await;
    }
    Ok(Json(
        json!({"connector_id": connector_id, "removed": removed}),
    ))
}

async fn connectors_test_v1(
    State(state): State<AppState>,
    AxumPath(connector_id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, _guard) = admin_auth(&state, &headers, "connectors.test").await?;
    require_feature(&state, Feature::Connectors)?;
    let record = state
        .store
        .connector_get(&principal.tenant_id, &connector_id)
        .await?
        .ok_or_else(|| AppError::not_found("connector not found"))?;
    let connector = match crate::connectors::from_record(&record) {
        Ok(connector) => connector,
        Err(err) => {
            return Ok(Json(
                json!({"connector_id": connector_id, "ok": false, "status": "not_configured", "code": err.code(), "message": err.to_string()}),
            ));
        }
    };
    match connector.test().await {
        Ok(details) => Ok(Json(
            json!({"connector_id": connector_id, "ok": true, "status": "ok", "details": details}),
        )),
        Err(err) => Ok(Json(
            json!({"connector_id": connector_id, "ok": false, "status": "not_configured", "code": err.code(), "message": err.to_string()}),
        )),
    }
}

async fn connectors_sync_v1(
    State(state): State<AppState>,
    AxumPath(connector_id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let (principal, _guard) = admin_auth(&state, &headers, "connectors.sync").await?;
    require_feature(&state, Feature::Connectors)?;
    let record = state
        .store
        .connector_get(&principal.tenant_id, &connector_id)
        .await?
        .ok_or_else(|| AppError::not_found("connector not found"))?;
    let now = now_rfc3339();
    let job = crate::store::ConnectorJobRecord {
        job_id: format!("cjob_{}", random_hex(10)),
        connector_id: record.connector_id.clone(),
        status: "queued".to_string(),
        started_at: now.clone(),
        finished_at: None,
        stats_json: "{}".to_string(),
        error: None,
    };
    state.store.connector_job_create(&job).await?;
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        Some(&record.space_id),
        &principal.principal_id,
        "user",
        "connector.sync",
        "connector",
        &record.connector_id,
        "allow",
        202,
        None,
        client_ip(&headers).as_deref(),
        user_agent(&headers).as_deref(),
        json!({"job_id": job.job_id}),
    )
    .await;
    let store = state.store.clone();
    let config = state.config.clone();
    let providers = state.providers.clone();
    let job_id = job.job_id.clone();
    tokio::spawn(async move {
        let _ = crate::connectors::run_sync(store, config, providers, record, job).await;
    });
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"job_id": job_id, "status": "queued"})),
    )
        .into_response())
}

async fn connectors_jobs_v1(
    State(state): State<AppState>,
    AxumPath(connector_id): AxumPath<String>,
    headers: HeaderMap,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let (principal, _guard) = admin_auth(&state, &headers, "connectors.jobs").await?;
    require_feature(&state, Feature::Connectors)?;
    state
        .store
        .connector_get(&principal.tenant_id, &connector_id)
        .await?
        .ok_or_else(|| AppError::not_found("connector not found"))?;
    let limit = query
        .get("limit")
        .and_then(|value| value.parse().ok())
        .unwrap_or(50);
    let jobs = state.store.connector_job_list(&connector_id, limit).await?;
    Ok(Json(json!({"jobs": jobs})))
}

async fn external_refs_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let (principal, _guard) = admin_auth(&state, &headers, "external_refs.list").await?;
    require_feature(&state, Feature::Connectors)?;
    let limit = query
        .get("limit")
        .and_then(|value| value.parse().ok())
        .unwrap_or(200);
    let refs = state
        .store
        .external_ref_list(
            &principal.tenant_id,
            query.get("doc_id").map(String::as_str),
            query.get("provider").map(String::as_str),
            limit,
        )
        .await?;
    Ok(Json(json!({"external_refs": refs})))
}

pub(crate) fn parse_trusted_proxy(value: &str) -> Result<(std::net::IpAddr, u8), String> {
    let value = value.trim();
    let (ip_text, prefix) = match value.split_once('/') {
        Some((ip, prefix)) => (ip, Some(prefix)),
        None => (value, None),
    };
    let address: std::net::IpAddr = ip_text
        .parse()
        .map_err(|_| format!("invalid trusted proxy IP {value:?}"))?;
    let max = if address.is_ipv4() { 32 } else { 128 };
    let prefix = match prefix {
        Some(prefix) => prefix
            .parse::<u8>()
            .map_err(|_| format!("invalid trusted proxy prefix {value:?}"))?,
        None => max,
    };
    if prefix > max {
        return Err(format!("trusted proxy prefix out of range {value:?}"));
    }
    Ok((address, prefix))
}

pub(crate) fn ip_in_trusted_proxies(ip: std::net::IpAddr, proxies: &[String]) -> bool {
    proxies.iter().any(|proxy| {
        let Ok((network, prefix)) = parse_trusted_proxy(proxy) else {
            return false;
        };
        match (ip, network) {
            (std::net::IpAddr::V4(ip), std::net::IpAddr::V4(network)) => {
                let mask = if prefix == 0 {
                    0
                } else {
                    u32::MAX << (32 - prefix)
                };
                (u32::from(ip) & mask) == (u32::from(network) & mask)
            }
            (std::net::IpAddr::V6(ip), std::net::IpAddr::V6(network)) => {
                let ip = u128::from(ip);
                let network = u128::from(network);
                let mask = if prefix == 0 {
                    0
                } else {
                    u128::MAX << (128 - prefix)
                };
                (ip & mask) == (network & mask)
            }
            _ => false,
        }
    })
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(header_name, _)| header_name.as_str().eq_ignore_ascii_case(name))
        .and_then(|(_, value)| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn sso_mapped_role(
    config: &crate::config::SsoConfig,
    groups: &[String],
) -> Option<crate::governance::UserRole> {
    use crate::governance::UserRole;
    let mut best: Option<UserRole> = None;
    for group in groups {
        if let Some(role) = config.group_role_map.get(group) {
            if let Ok(role) = UserRole::parse(role) {
                let rank = |value: UserRole| match value {
                    UserRole::Owner => 3,
                    UserRole::Admin => 2,
                    UserRole::User => 1,
                };
                if best
                    .map(|current| rank(role) > rank(current))
                    .unwrap_or(true)
                {
                    best = Some(role);
                }
            }
        }
    }
    best
}

async fn sso_middleware(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    // Never accept a client-supplied copy of the internal identity marker.
    request.headers_mut().remove("x-lkb-sso-authenticated");
    if !state.config.sso.mode.eq_ignore_ascii_case("trusted_header") {
        return next.run(request).await;
    }
    let username_header = header_value(request.headers(), &state.config.sso.user_header);
    let email_header = header_value(request.headers(), &state.config.sso.email_header);
    let groups_header = header_value(request.headers(), &state.config.sso.groups_header);
    if username_header.is_none() && email_header.is_none() && groups_header.is_none() {
        return next.run(request).await;
    }
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip());
    let trusted = peer
        .map(|ip| ip_in_trusted_proxies(ip, &state.config.sso.trusted_proxies))
        .unwrap_or(false);
    let tenant = state.config.storage.tenant_id.clone();
    let ip_hint = client_ip(request.headers());
    if !trusted {
        crate::audit::record(
            &state.store,
            &tenant,
            None,
            "anonymous",
            "anonymous",
            "sso.denied",
            "user",
            username_header.as_deref().unwrap_or("unknown"),
            "denied",
            401,
            None,
            ip_hint.as_deref(),
            None,
            json!({"reason": "untrusted_peer"}),
        )
        .await;
        return AppError::unauthorized("SSO headers are only accepted from a trusted proxy")
            .into_response();
    }
    let username = match username_header.as_deref() {
        Some(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => {
            crate::audit::record(
                &state.store,
                &tenant,
                None,
                "anonymous",
                "anonymous",
                "sso.denied",
                "user",
                "unknown",
                "denied",
                401,
                None,
                ip_hint.as_deref(),
                None,
                json!({"reason": "missing_user"}),
            )
            .await;
            return AppError::unauthorized("SSO user header is missing").into_response();
        }
    };
    let email = email_header;
    if !state.config.sso.allowed_email_domains.is_empty() {
        let domain = email.as_deref().and_then(|email| {
            email
                .rsplit_once('@')
                .map(|(_, domain)| domain.to_ascii_lowercase())
        });
        let allowed = domain
            .map(|domain| {
                state
                    .config
                    .sso
                    .allowed_email_domains
                    .iter()
                    .any(|allowed| allowed.trim().eq_ignore_ascii_case(domain.as_str()))
            })
            .unwrap_or(false);
        if !allowed {
            crate::audit::record(
                &state.store,
                &tenant,
                None,
                "anonymous",
                "anonymous",
                "sso.denied",
                "user",
                &username,
                "denied",
                403,
                None,
                ip_hint.as_deref(),
                None,
                json!({"reason": "email_domain_not_allowed"}),
            )
            .await;
            return AppError::forbidden("SSO email domain is not allowed").into_response();
        }
    }
    let groups: Vec<String> = groups_header
        .as_deref()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let mut created = false;
    let mut user = None;
    if let Some(email) = email.as_deref() {
        user = state
            .store
            .get_user_by_email(&tenant, email)
            .await
            .ok()
            .flatten();
    }
    if user.is_none() {
        user = state
            .store
            .get_user_by_external_id(&tenant, "sso", &username)
            .await
            .ok()
            .flatten();
    }
    if user.is_none() {
        user = state
            .store
            .get_user(&tenant, &username)
            .await
            .ok()
            .flatten();
    }
    let mut user = match user {
        Some(user) => {
            if let (Some(existing), Some(incoming)) = (user.email.as_deref(), email.as_deref()) {
                if !existing.eq_ignore_ascii_case(incoming) {
                    crate::audit::record(
                        &state.store,
                        &tenant,
                        None,
                        "anonymous",
                        "anonymous",
                        "sso.denied",
                        "user",
                        &username,
                        "denied",
                        403,
                        None,
                        ip_hint.as_deref(),
                        None,
                        json!({"reason": "email_conflict"}),
                    )
                    .await;
                    return AppError::forbidden("SSO identity conflicts with an existing user")
                        .into_response();
                }
            }
            user
        }
        None => {
            if !state.config.sso.auto_provision {
                crate::audit::record(
                    &state.store,
                    &tenant,
                    None,
                    "anonymous",
                    "anonymous",
                    "sso.denied",
                    "user",
                    &username,
                    "denied",
                    401,
                    None,
                    ip_hint.as_deref(),
                    None,
                    json!({"reason": "auto_provision_disabled"}),
                )
                .await;
                return AppError::unauthorized("SSO user is not provisioned").into_response();
            }
            let role = sso_mapped_role(&state.config.sso, &groups)
                .or_else(|| crate::governance::UserRole::parse(&state.config.sso.default_role).ok())
                .unwrap_or(crate::governance::UserRole::User);
            let now = now_rfc3339();
            let record = crate::store::UserRecord {
                user_id: format!("usr_{}", random_hex(10)),
                tenant_id: tenant.clone(),
                username: username.clone(),
                password_hash: "sso-no-password".to_string(),
                display_name: username.clone(),
                created_at: now.clone(),
                disabled: false,
                role,
                email: email.clone(),
                external_id: Some(username.clone()),
                source: "sso".to_string(),
                last_sso_at: Some(now.clone()),
            };
            if let Err(err) = state.store.create_sso_user(&record).await {
                tracing::warn!(error = %err, "sso provision failed");
                return AppError::internal("cannot provision SSO user").into_response();
            }
            created = true;
            crate::audit::record(
                &state.store,
                &tenant,
                None,
                "anonymous",
                "anonymous",
                "sso.provision",
                "user",
                &record.user_id,
                "allow",
                201,
                None,
                ip_hint.as_deref(),
                None,
                json!({"username": record.username, "role": role.as_str()}),
            )
            .await;
            record
        }
    };
    if user.disabled {
        crate::audit::record(
            &state.store,
            &tenant,
            None,
            "anonymous",
            "anonymous",
            "sso.denied",
            "user",
            &user.user_id,
            "denied",
            401,
            None,
            ip_hint.as_deref(),
            None,
            json!({"reason": "user_disabled"}),
        )
        .await;
        return AppError::unauthorized("user is disabled").into_response();
    }
    if !created {
        if let Some(role) = sso_mapped_role(&state.config.sso, &groups) {
            if user.role != role {
                let _ = state
                    .store
                    .set_user_role(&tenant, &user.user_id, role)
                    .await;
                user.role = role;
            }
        }
        if let Some(email) = email.as_deref() {
            let _ = state
                .store
                .link_sso_identity(&tenant, &user.user_id, email, &username, &now_rfc3339())
                .await;
        }
        let _ = state
            .store
            .touch_sso_login(&tenant, &user.user_id, &now_rfc3339())
            .await;
    }
    crate::audit::record(
        &state.store,
        &tenant,
        None,
        &user.user_id,
        "user",
        "sso.login",
        "user",
        &user.user_id,
        "allow",
        200,
        None,
        ip_hint.as_deref(),
        None,
        json!({"username": user.username}),
    )
    .await;
    if let Some(old) = session_hash(request.headers(), &state) {
        let _ = state.store.delete_session(&old).await;
    }
    let raw_session = random_hex(32);
    let ttl_hours = state
        .config
        .sso
        .session_ttl_hours
        .unwrap_or(state.config.auth.session_ttl_hours);
    let now = chrono::Utc::now();
    let expires = now + chrono::Duration::hours(ttl_hours as i64);
    if state
        .store
        .create_session(&crate::store::SessionRecord {
            session_hash: sha256_hex_str(&raw_session),
            tenant_id: tenant.clone(),
            user_id: user.user_id.clone(),
            created_at: now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            expires_at: expires.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        })
        .await
        .is_err()
    {
        return AppError::internal("cannot create SSO session").into_response();
    }
    if let Ok(value) = HeaderValue::from_str(&user.user_id) {
        request.headers_mut().insert(
            header::HeaderName::from_static("x-lkb-sso-authenticated"),
            value,
        );
    }
    let secure = state
        .config
        .cookie_secure_for_host(host_header(request.headers()));
    let mut response = next.run(request).await;
    append_set_cookie(
        &mut response,
        build_cookie(
            &state.config.auth.cookie_name,
            &raw_session,
            ttl_hours as i64 * 3600,
            true,
            secure,
        ),
    );
    response
}

async fn auth_sso_status_v1(State(state): State<AppState>) -> Json<Value> {
    let mode = state.config.sso.mode.trim().to_ascii_lowercase();
    Json(json!({
        "enabled": mode == "trusted_header",
        "mode": if mode == "trusted_header" { "trusted_header" } else { "off" },
    }))
}

async fn openapi_json() -> Json<Value> {
    Json(crate::openapi::document())
}

async fn admin_users_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let (principal, _guard) = admin_auth(&state, &headers, "admin.users.list").await?;
    let offset = query
        .get("offset")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let limit = query
        .get("limit")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(100)
        .min(500);
    let all = state.store.list_users(&principal.tenant_id).await?;
    let total = all.len();
    let users: Vec<Value> = all
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|user| json!({
            "user_id": user.user_id, "username": user.username, "display_name": user.display_name,
            "role": user.role.as_str(), "disabled": user.disabled, "created_at": user.created_at,
        }))
        .collect();
    Ok(Json(
        json!({"users": users, "total": total, "offset": offset, "limit": limit}),
    ))
}

async fn admin_user_patch_v1(
    State(state): State<AppState>,
    AxumPath(user_id): AxumPath<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let audit_target = user_id.clone();
    let (principal, _guard) = admin_auth(&state, &headers, "admin.users.patch").await?;
    if let Some(role) = body.get("role").and_then(Value::as_str) {
        let role = crate::governance::UserRole::parse(role)
            .map_err(|_| AppError::bad_request("invalid role"))?;
        state
            .store
            .set_user_role(&principal.tenant_id, &user_id, role)
            .await?;
    }
    if let Some(disabled) = body.get("disabled").and_then(Value::as_bool) {
        state
            .store
            .set_user_disabled(&principal.tenant_id, &user_id, disabled)
            .await?;
    }
    audit_admin_write(
        &state,
        &principal,
        "user.admin.update",
        "user",
        &audit_target,
    )
    .await;
    Ok(Json(json!({"user_id": user_id, "updated": true})))
}

async fn admin_user_disable_v1(
    State(state): State<AppState>,
    AxumPath(user_id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let audit_target = user_id.clone();
    let (principal, _guard) = admin_auth(&state, &headers, "admin.users.disable").await?;
    state
        .store
        .set_user_disabled(&principal.tenant_id, &user_id, true)
        .await?;
    audit_admin_write(
        &state,
        &principal,
        "user.admin.disable",
        "user",
        &audit_target,
    )
    .await;
    Ok(Json(json!({"user_id": user_id, "disabled": true})))
}

async fn admin_roles_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (_principal, _guard) = admin_auth(&state, &headers, "admin.roles.list").await?;
    Ok(Json(json!({"roles": [
        {"role": "owner", "description": "tenant owner"},
        {"role": "admin", "description": "tenant administrator"},
        {"role": "user", "description": "tenant user"},
    ]})))
}

async fn admin_groups_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, _guard) = admin_auth(&state, &headers, "admin.groups.list").await?;
    let groups = state.store.list_groups(&principal.tenant_id).await?;
    Ok(Json(json!({"groups": groups})))
}

async fn admin_spaces_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, _guard) = admin_auth(&state, &headers, "admin.spaces.list").await?;
    let spaces = state.store.list_spaces(&principal.tenant_id).await?;
    Ok(Json(json!({"spaces": spaces})))
}

async fn admin_usage_aggregate_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let (principal, _guard) = admin_auth(&state, &headers, "admin.usage.aggregate").await?;
    let filter = UsageFilter {
        tenant_id: principal.tenant_id.clone(),
        space_id: query.get("space").cloned(),
        user_id: query.get("user").cloned(),
        alias: query.get("alias").cloned(),
        from: query.get("from").cloned(),
        to: query.get("to").cloned(),
        limit: query
            .get("limit")
            .and_then(|value| value.parse().ok())
            .unwrap_or(5000),
    };
    let totals = state.store.aggregate_usage_events(&filter).await?;
    let events = state.store.list_usage_events(&filter).await?;
    let group_by = query.get("group_by").map(String::as_str).unwrap_or("user");
    let mut groups: std::collections::BTreeMap<String, (i64, i64, i64, i64)> =
        std::collections::BTreeMap::new();
    for event in &events {
        let key = match group_by {
            "space" => event.space_id.clone().unwrap_or_default(),
            "alias" => event.alias.clone(),
            "period" | "day" => event.at.chars().take(10).collect(),
            "source" => event.source.clone(),
            _ => event.user_id.clone(),
        };
        let entry = groups.entry(key).or_insert((0, 0, 0, 0));
        entry.0 += 1;
        entry.1 += event.input_tokens;
        entry.2 += event.output_tokens;
        entry.3 += event.cost_micros;
    }
    let page_size = query
        .get("limit")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(100)
        .min(500);
    let offset = query
        .get("offset")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let total = groups.len();
    let items: Vec<Value> = groups
        .into_iter()
        .skip(offset)
        .take(page_size)
        .map(
            |(key, (requests, input_tokens, output_tokens, cost_micros))| {
                json!({
                    "key": key, "requests": requests, "input_tokens": input_tokens,
                    "output_tokens": output_tokens, "cost_micros": cost_micros,
                })
            },
        )
        .collect();
    Ok(Json(json!({
        "group_by": group_by, "totals": totals, "groups": items,
        "total": total, "offset": offset, "limit": page_size,
    })))
}

async fn list_audit_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Value>, AppError> {
    require_feature(&state, Feature::Audit)?;
    let (principal, _, _limits) =
        v1_auth(&state, &headers, TokenScope::Admin, "audit.read", None).await?;
    let filter = AuditFilter {
        tenant_id: principal.tenant_id.clone(),
        actor_id: query.actor,
        action: query.action,
        space_id: query.space,
        from: query.from,
        to: query.to,
        limit: query.limit.unwrap_or(100).clamp(1, 500),
    };
    let events = state.store.list_audit(&filter).await?;
    Ok(Json(json!({
        "events": events
            .into_iter()
            .map(|event| json!({
                "event_id": event.event_id,
                "tenant_id": event.tenant_id,
                "space_id": event.space_id,
                "actor_id": event.actor_id,
                "actor_type": event.actor_type,
                "action": event.action,
                "target_type": event.target_type,
                "target_id": event.target_id,
                "result": event.result,
                "status_code": event.status_code,
                "request_id": event.request_id,
                "ip": event.ip,
                "user_agent": event.user_agent,
                "detail": serde_json::from_str::<Value>(&event.detail_json)
                    .unwrap_or(Value::Null),
                "created_at": event.created_at,
            }))
            .collect::<Vec<_>>(),
    })))
}

// ---------------------------------------------------------------------------
// Model gateway / usage / quota admin API.
// ---------------------------------------------------------------------------

async fn admin_auth(
    state: &AppState,
    headers: &HeaderMap,
    action: &str,
) -> Result<(Principal, crate::limits::LimitGuard), AppError> {
    let (principal, _session, guard) =
        v1_auth(state, headers, TokenScope::Admin, action, None).await?;
    Ok((principal, guard))
}

fn provider_json(provider: &ProviderRecord) -> Value {
    json!({
        "provider_id": provider.provider_id,
        "kind": provider.kind,
        "base_url": provider.base_url,
        "key_configured": provider.api_key_env.as_deref().map(|name| !name.trim().is_empty()).unwrap_or(false),
        "status": provider.status,
        "priority": provider.priority,
        "weight": provider.weight,
        "limits_json": provider.limits_json,
        "timeouts_json": provider.timeouts_json,
        "created_at": provider.created_at,
        "updated_at": provider.updated_at,
    })
}

fn provider_model_json(model: &ProviderModelRecord) -> Value {
    json!({
        "provider_model_id": model.provider_model_id,
        "provider_id": model.provider_id,
        "upstream_model": model.upstream_model,
        "context_window": model.context_window,
        "pricing_json": model.pricing_json,
        "capabilities_json": model.capabilities_json,
        "status": model.status,
    })
}

fn alias_json(alias: &ModelAliasRecord) -> Value {
    json!({
        "public_id": alias.public_id,
        "display_name": alias.display_name,
        "capabilities_json": alias.capabilities_json,
        "status": alias.status,
        "access_policy_json": alias.access_policy_json,
        "price_json": alias.price_json,
        "version": alias.version,
        "created_at": alias.created_at,
        "updated_at": alias.updated_at,
    })
}

fn route_json(route: &RouteRecord) -> Value {
    json!({
        "route_id": route.route_id,
        "alias_public_id": route.alias_public_id,
        "provider_model_id": route.provider_model_id,
        "weight": route.weight,
        "priority": route.priority,
        "health": route.health,
        "circuit_state": route.circuit_state,
        "last_error_at": route.last_error_at,
        "circuit_open_until": route.circuit_open_until,
    })
}

fn quota_json_token(quota: &TokenQuotaRecord) -> Value {
    json!({
        "quota_id": quota.quota_id,
        "scope_type": quota.scope_type,
        "scope_id": quota.scope_id,
        "period": quota.period,
        "limit_tokens": quota.limit_tokens,
        "limit_requests": quota.limit_requests,
        "limit_cost_micros": quota.limit_cost_micros,
        "currency": quota.currency,
        "enforcement": quota.enforcement,
        "action": quota.action,
        "enabled": quota.enabled,
        "updated_at": quota.updated_at,
    })
}

#[derive(Debug, Deserialize)]
struct ProviderBody {
    provider_id: String,
    kind: String,
    base_url: String,
    api_key_env: Option<String>,
    #[serde(default = "default_active")]
    status: String,
    #[serde(default = "default_priority")]
    priority: i32,
    #[serde(default = "default_weight")]
    weight: i32,
    #[serde(default = "default_json_object")]
    limits_json: String,
    #[serde(default = "default_json_object")]
    timeouts_json: String,
}

#[derive(Debug, Deserialize)]
struct AliasBody {
    public_id: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default = "default_capabilities")]
    capabilities_json: String,
    #[serde(default = "default_active")]
    status: String,
    #[serde(default = "default_json_object")]
    access_policy_json: String,
    #[serde(default = "default_json_object")]
    price_json: String,
}

#[derive(Debug, Deserialize)]
struct RouteBody {
    route_id: String,
    alias_public_id: String,
    provider_model_id: String,
    #[serde(default = "default_weight")]
    weight: i32,
    #[serde(default = "default_priority")]
    priority: i32,
}

#[derive(Debug, Deserialize)]
struct ProviderModelBody {
    provider_model_id: String,
    provider_id: String,
    upstream_model: String,
    #[serde(default)]
    context_window: i64,
    #[serde(default = "default_json_object")]
    pricing_json: String,
    #[serde(default = "default_capabilities")]
    capabilities_json: String,
}

#[derive(Debug, Deserialize)]
struct TokenQuotaBody {
    #[serde(default)]
    quota_id: Option<String>,
    scope_type: String,
    scope_id: String,
    #[serde(default = "default_period")]
    period: String,
    limit_tokens: Option<i64>,
    limit_requests: Option<i64>,
    limit_cost_micros: Option<i64>,
    #[serde(default = "default_currency")]
    currency: String,
    #[serde(default = "default_hard")]
    enforcement: String,
    #[serde(default = "default_block")]
    action: String,
    #[serde(default = "default_true")]
    enabled: bool,
}

fn default_active() -> String {
    "active".to_string()
}
fn default_priority() -> i32 {
    100
}
fn default_weight() -> i32 {
    1
}
fn default_json_object() -> String {
    "{}".to_string()
}
fn default_capabilities() -> String {
    "[\"chat\"]".to_string()
}
fn default_period() -> String {
    "month".to_string()
}
fn default_currency() -> String {
    "USD".to_string()
}
fn default_hard() -> String {
    "hard".to_string()
}
fn default_block() -> String {
    "block".to_string()
}
fn default_true() -> bool {
    true
}

async fn admin_providers_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (_principal, _guard) = admin_auth(&state, &headers, "models.providers.list").await?;
    let providers = state
        .store
        .list_providers(&state.config.storage.tenant_id)
        .await?;
    let models = state
        .store
        .list_provider_models(&state.config.storage.tenant_id)
        .await?;
    Ok(Json(json!({
        "providers": providers.iter().map(provider_json).collect::<Vec<_>>(),
        "models": models.iter().map(provider_model_json).collect::<Vec<_>>(),
    })))
}

async fn admin_provider_create_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ProviderBody>,
) -> Result<Response, AppError> {
    let (principal, _guard) = admin_auth(&state, &headers, "models.provider.create").await?;
    let now = now_rfc3339();
    let provider = ProviderRecord {
        provider_id: body.provider_id,
        kind: body.kind,
        base_url: body.base_url,
        api_key_env: body.api_key_env,
        status: body.status,
        priority: body.priority,
        weight: body.weight,
        limits_json: body.limits_json,
        timeouts_json: body.timeouts_json,
        created_at: now.clone(),
        updated_at: now,
    };
    state
        .store
        .upsert_provider(&principal.tenant_id, &provider)
        .await?;
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        None,
        &principal.principal_id,
        "user",
        "model.provider.created",
        "provider",
        &provider.provider_id,
        "allow",
        201,
        None,
        None,
        None,
        json!({}),
    )
    .await;
    Ok((
        StatusCode::CREATED,
        Json(json!({"provider": provider_json(&provider)})),
    )
        .into_response())
}

async fn admin_provider_patch_v1(
    State(state): State<AppState>,
    AxumPath(provider_id): AxumPath<String>,
    headers: HeaderMap,
    Json(body): Json<ProviderBody>,
) -> Result<Json<Value>, AppError> {
    let audit_target = provider_id.clone();
    let (principal, _guard) = admin_auth(&state, &headers, "models.provider.update").await?;
    if body.provider_id != provider_id {
        return Err(AppError::bad_request("provider_id mismatch"));
    }
    let existing = state
        .store
        .get_provider(&principal.tenant_id, &provider_id)
        .await?;
    let now = now_rfc3339();
    let provider = ProviderRecord {
        provider_id,
        kind: body.kind,
        base_url: body.base_url,
        api_key_env: body
            .api_key_env
            .or_else(|| existing.as_ref().and_then(|p| p.api_key_env.clone())),
        status: body.status,
        priority: body.priority,
        weight: body.weight,
        limits_json: body.limits_json,
        timeouts_json: body.timeouts_json,
        created_at: existing
            .map(|p| p.created_at)
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    state
        .store
        .upsert_provider(&principal.tenant_id, &provider)
        .await?;
    audit_admin_write(
        &state,
        &principal,
        "provider.update",
        "provider",
        &audit_target,
    )
    .await;
    Ok(Json(json!({"provider": provider_json(&provider)})))
}

async fn admin_provider_delete_v1(
    State(state): State<AppState>,
    AxumPath(provider_id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let audit_target = provider_id.clone();
    let (principal, _guard) = admin_auth(&state, &headers, "models.provider.delete").await?;
    let removed = state
        .store
        .delete_provider(&principal.tenant_id, &provider_id)
        .await?;
    audit_admin_write(
        &state,
        &principal,
        "provider.delete",
        "provider",
        &audit_target,
    )
    .await;
    Ok(Json(json!({"removed": removed})))
}

async fn admin_provider_models_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (_principal, _guard) = admin_auth(&state, &headers, "models.model.list").await?;
    let models = state
        .store
        .list_provider_models(&state.config.storage.tenant_id)
        .await?;
    Ok(Json(
        json!({"models": models.iter().map(provider_model_json).collect::<Vec<_>>()}),
    ))
}

async fn admin_provider_model_create_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ProviderModelBody>,
) -> Result<Response, AppError> {
    let (principal, _guard) = admin_auth(&state, &headers, "models.model.create").await?;
    let now = now_rfc3339();
    let model = ProviderModelRecord {
        provider_model_id: body.provider_model_id,
        provider_id: body.provider_id,
        upstream_model: body.upstream_model,
        context_window: body.context_window,
        pricing_json: body.pricing_json,
        capabilities_json: body.capabilities_json,
        status: "active".to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    state
        .store
        .upsert_provider_model(&principal.tenant_id, &model)
        .await?;
    audit_admin_write(
        &state,
        &principal,
        "provider_model.create",
        "provider_model",
        &model.provider_model_id,
    )
    .await;
    Ok((
        StatusCode::CREATED,
        Json(json!({"model": provider_model_json(&model)})),
    )
        .into_response())
}

async fn admin_aliases_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (_principal, _guard) = admin_auth(&state, &headers, "models.alias.list").await?;
    let aliases = state
        .store
        .list_model_aliases(&state.config.storage.tenant_id)
        .await?;
    Ok(Json(
        json!({"aliases": aliases.iter().map(alias_json).collect::<Vec<_>>()}),
    ))
}

async fn admin_alias_create_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<AliasBody>,
) -> Result<Response, AppError> {
    let (principal, _guard) = admin_auth(&state, &headers, "models.alias.create").await?;
    let now = now_rfc3339();
    let alias = ModelAliasRecord {
        public_id: body.public_id,
        display_name: body.display_name.unwrap_or_else(|| "Model".to_string()),
        capabilities_json: body.capabilities_json,
        status: body.status,
        access_policy_json: body.access_policy_json,
        price_json: body.price_json,
        version: 1,
        created_at: now.clone(),
        updated_at: now,
    };
    state
        .store
        .upsert_model_alias(&principal.tenant_id, &alias)
        .await?;
    audit_admin_write(
        &state,
        &principal,
        "alias.create",
        "model_alias",
        &alias.public_id,
    )
    .await;
    Ok((
        StatusCode::CREATED,
        Json(json!({"alias": alias_json(&alias)})),
    )
        .into_response())
}

async fn admin_alias_patch_v1(
    State(state): State<AppState>,
    AxumPath(public_id): AxumPath<String>,
    headers: HeaderMap,
    Json(body): Json<AliasBody>,
) -> Result<Json<Value>, AppError> {
    let audit_target = public_id.clone();
    let (principal, _guard) = admin_auth(&state, &headers, "models.alias.update").await?;
    if body.public_id != public_id {
        return Err(AppError::bad_request("public_id mismatch"));
    }
    let existing = state
        .store
        .get_model_alias(&principal.tenant_id, &public_id)
        .await?;
    let now = now_rfc3339();
    let alias = ModelAliasRecord {
        public_id,
        display_name: body.display_name.unwrap_or_else(|| {
            existing
                .as_ref()
                .map(|a| a.display_name.clone())
                .unwrap_or_else(|| "Model".to_string())
        }),
        capabilities_json: body.capabilities_json,
        status: body.status,
        access_policy_json: body.access_policy_json,
        price_json: body.price_json,
        version: existing.as_ref().map(|a| a.version + 1).unwrap_or(1),
        created_at: existing
            .as_ref()
            .map(|a| a.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    state
        .store
        .upsert_model_alias(&principal.tenant_id, &alias)
        .await?;
    audit_admin_write(
        &state,
        &principal,
        "alias.update",
        "model_alias",
        &audit_target,
    )
    .await;
    Ok(Json(json!({"alias": alias_json(&alias)})))
}

async fn admin_alias_delete_v1(
    State(state): State<AppState>,
    AxumPath(public_id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let audit_target = public_id.clone();
    let (principal, _guard) = admin_auth(&state, &headers, "models.alias.delete").await?;
    let removed = state
        .store
        .delete_model_alias(&principal.tenant_id, &public_id)
        .await?;
    audit_admin_write(
        &state,
        &principal,
        "alias.delete",
        "model_alias",
        &audit_target,
    )
    .await;
    Ok(Json(json!({"removed": removed})))
}

async fn admin_routes_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let (_principal, _guard) = admin_auth(&state, &headers, "models.route.list").await?;
    let alias = query.get("alias").map(|value| value.as_str());
    let routes = state
        .store
        .list_routes(&state.config.storage.tenant_id, alias)
        .await?;
    Ok(Json(
        json!({"routes": routes.iter().map(route_json).collect::<Vec<_>>()}),
    ))
}

async fn admin_route_create_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RouteBody>,
) -> Result<Response, AppError> {
    let (principal, _guard) = admin_auth(&state, &headers, "models.route.create").await?;
    let now = now_rfc3339();
    let route = RouteRecord {
        route_id: body.route_id,
        alias_public_id: body.alias_public_id,
        provider_model_id: body.provider_model_id,
        weight: body.weight,
        priority: body.priority,
        health: "healthy".to_string(),
        circuit_state: "closed".to_string(),
        last_error_at: None,
        circuit_open_until: None,
        created_at: now.clone(),
        updated_at: now,
    };
    state
        .store
        .upsert_route(&principal.tenant_id, &route)
        .await?;
    audit_admin_write(
        &state,
        &principal,
        "route.create",
        "model_route",
        &route.route_id,
    )
    .await;
    Ok((
        StatusCode::CREATED,
        Json(json!({"route": route_json(&route)})),
    )
        .into_response())
}

async fn admin_route_patch_v1(
    State(state): State<AppState>,
    AxumPath(route_id): AxumPath<String>,
    headers: HeaderMap,
    Json(body): Json<RouteBody>,
) -> Result<Json<Value>, AppError> {
    let audit_target = route_id.clone();
    let (principal, _guard) = admin_auth(&state, &headers, "models.route.update").await?;
    if body.route_id != route_id {
        return Err(AppError::bad_request("route_id mismatch"));
    }
    let existing = state
        .store
        .list_routes(&principal.tenant_id, Some(&body.alias_public_id))
        .await?
        .into_iter()
        .find(|route| route.route_id == route_id);
    let now = now_rfc3339();
    let route = RouteRecord {
        route_id,
        alias_public_id: body.alias_public_id,
        provider_model_id: body.provider_model_id,
        weight: body.weight,
        priority: body.priority,
        health: existing
            .as_ref()
            .map(|r| r.health.clone())
            .unwrap_or_else(|| "healthy".to_string()),
        circuit_state: existing
            .as_ref()
            .map(|r| r.circuit_state.clone())
            .unwrap_or_else(|| "closed".to_string()),
        last_error_at: existing.as_ref().and_then(|r| r.last_error_at.clone()),
        circuit_open_until: existing.as_ref().and_then(|r| r.circuit_open_until.clone()),
        created_at: existing
            .as_ref()
            .map(|r| r.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    state
        .store
        .upsert_route(&principal.tenant_id, &route)
        .await?;
    audit_admin_write(
        &state,
        &principal,
        "route.update",
        "model_route",
        &audit_target,
    )
    .await;
    Ok(Json(json!({"route": route_json(&route)})))
}

async fn admin_route_delete_v1(
    State(state): State<AppState>,
    AxumPath(route_id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let audit_target = route_id.clone();
    let (principal, _guard) = admin_auth(&state, &headers, "models.route.delete").await?;
    let removed = state
        .store
        .delete_route(&principal.tenant_id, &route_id)
        .await?;
    audit_admin_write(
        &state,
        &principal,
        "route.delete",
        "model_route",
        &audit_target,
    )
    .await;
    Ok(Json(json!({"removed": removed})))
}

async fn admin_usage_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let (principal, _guard) = admin_auth(&state, &headers, "models.usage").await?;
    let filter = UsageFilter {
        tenant_id: principal.tenant_id.clone(),
        space_id: query.get("space").cloned(),
        user_id: query.get("user").cloned(),
        alias: query.get("alias").cloned(),
        from: query.get("from").cloned(),
        to: query.get("to").cloned(),
        limit: query
            .get("limit")
            .and_then(|value| value.parse().ok())
            .unwrap_or(1000),
    };
    let events = state.store.list_usage_events(&filter).await?;
    let aggregate = state.store.aggregate_usage_events(&filter).await?;
    let totals = json!(aggregate);
    Ok(Json(json!({
        "totals": totals,
        "events": events.iter().map(|event| json!({
            "usage_event_id": event.usage_event_id,
            "request_id": event.request_id,
            "tenant_id": event.tenant_id,
            "space_id": event.space_id,
            "user_id": event.user_id,
            "role": event.role,
            "source": event.source,
            "alias": event.alias,
            "provider_model_id": event.provider_model_id,
            "input_tokens": event.input_tokens,
            "output_tokens": event.output_tokens,
            "cached_tokens": event.cached_tokens,
            "token_source": event.token_source,
            "cost_micros": event.cost_micros,
            "currency": event.currency,
            "provider_cost_micros": event.provider_cost_micros,
            "latency_ms": event.latency_ms,
            "status": event.status,
            "cache_hit": event.cache_hit,
            "at": event.at,
        })).collect::<Vec<_>>(),
    })))
}

async fn admin_quotas_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, _guard) = admin_auth(&state, &headers, "models.quota.list").await?;
    let quotas = state.store.list_token_quotas(&principal.tenant_id).await?;
    let counters = state
        .store
        .list_quota_counters(&principal.tenant_id)
        .await?;
    Ok(Json(json!({
        "quotas": quotas.iter().map(quota_json_token).collect::<Vec<_>>(),
        "counters": counters,
    })))
}

async fn admin_quota_create_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<TokenQuotaBody>,
) -> Result<Response, AppError> {
    let (principal, _guard) = admin_auth(&state, &headers, "models.quota.create").await?;
    if body.limit_tokens.is_none()
        && body.limit_requests.is_none()
        && body.limit_cost_micros.is_none()
    {
        return Err(AppError::bad_request(
            "at least one quota limit is required",
        ));
    }
    let now = now_rfc3339();
    let quota = TokenQuotaRecord {
        quota_id: body
            .quota_id
            .unwrap_or_else(|| format!("tq_{}", random_hex(8))),
        tenant_id: principal.tenant_id.clone(),
        scope_type: body.scope_type,
        scope_id: body.scope_id,
        period: body.period,
        limit_tokens: body.limit_tokens,
        limit_requests: body.limit_requests,
        limit_cost_micros: body.limit_cost_micros,
        currency: body.currency,
        enforcement: body.enforcement,
        action: body.action,
        enabled: body.enabled,
        created_at: now.clone(),
        updated_at: now,
    };
    state.store.upsert_token_quota(&quota).await?;
    crate::audit::record(
        &state.store,
        &principal.tenant_id,
        None,
        &principal.principal_id,
        "user",
        "quota.model.updated",
        "token_quota",
        &quota.quota_id,
        "allow",
        201,
        None,
        None,
        None,
        json!({}),
    )
    .await;
    Ok((
        StatusCode::CREATED,
        Json(json!({"quota": quota_json_token(&quota)})),
    )
        .into_response())
}

async fn admin_quota_delete_v1(
    State(state): State<AppState>,
    AxumPath(quota_id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let audit_target = quota_id.clone();
    let (principal, _guard) = admin_auth(&state, &headers, "models.quota.delete").await?;
    let removed = state
        .store
        .delete_token_quota(&principal.tenant_id, &quota_id)
        .await?;
    audit_admin_write(
        &state,
        &principal,
        "quota.delete",
        "token_quota",
        &audit_target,
    )
    .await;
    Ok(Json(json!({"removed": removed})))
}

#[derive(Debug, Deserialize)]
struct GatewayChatBody {
    model: String,
    messages: Vec<crate::gateway::ChatMessage>,
    #[serde(default = "default_max_output")]
    max_tokens: i64,
    #[serde(default)]
    temperature: f64,
}
fn default_max_output() -> i64 {
    1024
}

async fn gateway_chat_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<GatewayChatBody>,
) -> Result<Json<Value>, AppError> {
    let (principal, _session, _guard) =
        v1_auth(&state, &headers, TokenScope::Write, "gateway.chat", None).await?;
    let request_id = v1_request_id();
    let gateway = crate::gateway::Gateway::new(state.store.clone());
    let response = gateway
        .chat(&crate::gateway::GatewayRequest {
            request_id,
            principal,
            space_id: None,
            role: "user".to_string(),
            alias: body.model,
            messages: body.messages,
            max_output_tokens: body.max_tokens.min(8192),
            temperature: body.temperature,
            source: "kb".to_string(),
            rewritten_query: None,
        })
        .await
        .map_err(|err| err.to_app_error())?;
    Ok(Json(json!({
        "model": response.model,
        "choices": [{"message": {"role": "assistant", "content": response.content}}],
        "usage": {
            "input_tokens": response.usage.input_tokens,
            "output_tokens": response.usage.output_tokens,
            "cached_tokens": response.usage.cached_tokens,
            "token_source": response.usage.token_source,
            "cost_micros": response.usage.cost_micros,
            "currency": response.usage.currency,
        },
        "provider_cost_micros": response.provider_cost_micros,
        "latency_ms": response.latency_ms,
    })))
}

#[derive(Debug, Deserialize)]
struct QueryRequest {
    query: String,
    space: Option<String>,
    lang: Option<String>,
    conversation_id: Option<String>,
    mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConversationCreate {
    space: Option<String>,
    title: Option<String>,
}

fn query_mode(value: Option<&str>) -> Result<&'static str, AppError> {
    match value.unwrap_or("both").trim().to_ascii_lowercase().as_str() {
        "search" => Ok("search"),
        "answer" => Ok("answer"),
        "both" => Ok("both"),
        _ => Err(AppError::bad_request("mode must be search, answer or both")),
    }
}

fn summary_json(summary: &crate::store::DocumentSummaryRecord) -> Value {
    json!({
        "doc_id": summary.doc_id,
        "revision_id": summary.revision_id,
        "lang": summary.lang,
        "model": summary.model,
        "prompt_version": summary.prompt_version,
        "content_hash": summary.content_hash,
        "status": summary.status,
        "summary": serde_json::from_str::<Value>(&summary.summary_json).unwrap_or(Value::Null),
        "usage_event_id": summary.usage_event_id,
        "generated_at": summary.generated_at,
        "error": summary.error,
    })
}

async fn page_summary_v1(
    State(state): State<AppState>,
    AxumPath((space, slug)): AxumPath<(String, String)>,
    Query(query): Query<LangQuery>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, _limits) = v1_auth(&state, &headers, TokenScope::Read, "page.summary", None)
        .await
        .map(|(principal, _session, guard)| (principal, guard))?;
    let _limits = _limits;
    require_space_role(
        &state,
        &principal,
        &space,
        SpaceRole::Viewer,
        "page.summary",
        &headers,
    )
    .await?;
    let lang = lang_arg(query.lang);
    let policy = policy_for(principal.clone())?;
    let view = state
        .store
        .get_page(&policy, &space, &slug, lang.as_deref())
        .await?
        .ok_or_else(|| AppError::not_found("page not found"))?;
    let logical = format!("wiki/{}/{}/{}.md", space, view.page.lang, slug);
    let canonical = doc_id::canonical_relative_path(&logical).map_err(AppError::bad_request)?;
    let doc_id = doc_id::derive_doc_id(&canonical, &view.page.content_hash);
    let summary = crate::summary::latest_summary(
        &state.store,
        &principal.tenant_id,
        &space,
        &doc_id,
        &view.page.content_hash,
        &state.config,
    )
    .await;
    match summary {
        Some(summary) => Ok(Json(json!({
            "space_id": space,
            "slug": slug,
            "source_revision": view.page.content_hash,
            "cache_hit": false,
            "summary": summary_json(&summary),
        }))),
        None => Ok(Json(json!({
            "space_id": space,
            "slug": slug,
            "source_revision": view.page.content_hash,
            "status": "pending",
            "summary": null,
            "degraded": [Degraded::new(
                "summary_generation_failed",
                crate::contract::DegradedCategory::Quality,
                "summary",
                "L0",
                "L0",
                crate::contract::DegradedReason::Unavailable,
                "static summary is not ready",
                "pending",
            )],
        }))),
    }
}

async fn query_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<QueryRequest>,
) -> Result<Json<Value>, AppError> {
    run_query(State(state), headers, Json(request), None)
        .await
        .map(Json)
}

fn sse_frame(event: &str, data: &Value) -> Bytes {
    Bytes::from(format!("event: {event}\ndata: {data}\n\n"))
}

async fn emit_stream(tx: &Option<mpsc::Sender<Bytes>>, event: &str, data: &Value) {
    if let Some(tx) = tx {
        let _ = tx.send(sse_frame(event, data)).await;
    }
}

async fn run_query(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<QueryRequest>,
    stream: Option<mpsc::Sender<Bytes>>,
) -> Result<Value, AppError> {
    let (principal, session, _limits) =
        v1_auth(&state, &headers, TokenScope::Read, "query", None).await?;
    v1_session_write_guard(&state, &headers, session)?;
    state.limits.check_query(&request.query, 8)?;
    let mode = query_mode(request.mode.as_deref())?;
    let space = match request.space {
        Some(space) => {
            if !principal
                .allowed_space_ids
                .iter()
                .any(|allowed| allowed == &space)
            {
                return Err(AppError::forbidden("space is outside the principal scope"));
            }
            space
        }
        None => principal
            .allowed_space_ids
            .first()
            .cloned()
            .ok_or_else(|| AppError::forbidden("no space available"))?,
    };
    let has_history = request.conversation_id.is_some();
    let conversation = if let Some(id) = request.conversation_id {
        let conversation = state
            .store
            .get_conversation(&principal.tenant_id, &id)
            .await?
            .ok_or_else(|| AppError::not_found("conversation not found"))?;
        if conversation.owner_id != principal.principal_id || conversation.status != "active" {
            return Err(AppError::forbidden("conversation belongs to another owner"));
        }
        if conversation.space_id != space {
            return Err(AppError::forbidden("conversation space mismatch"));
        }
        conversation
    } else {
        let now = now_rfc3339();
        let conversation = ConversationRecord {
            conversation_id: format!("conv_{}", random_hex(10)),
            tenant_id: principal.tenant_id.clone(),
            space_id: space.clone(),
            owner_id: principal.principal_id.clone(),
            title: crate::util::truncate_chars(&request.query, 80),
            external_session_id: None,
            status: "active".to_string(),
            created_at: now.clone(),
            updated_at: now,
        };
        state.store.create_conversation(&conversation).await?;
        conversation
    };
    let (effective_query, context_hash, rewrite_error) = if has_history {
        crate::conversation::rewrite_query(
            &state.store,
            &state.config,
            &principal,
            &conversation.conversation_id,
            &request.query,
        )
        .await
    } else {
        (request.query.clone(), String::new(), None)
    };
    let mut scoped = principal.clone();
    scoped.allowed_space_ids = vec![space.clone()];
    let policy = Policy::new(scoped, None)
        .map_err(AppError::bad_request)?
        .with_lang(lang_arg(request.lang));
    let request_id = v1_request_id();
    let user_message_id = format!("msg_{}", random_hex(10));
    let assistant_message_id = format!("msg_{}", random_hex(10));
    emit_stream(
        &stream,
        "meta",
        &json!({
            "conversation_id": conversation.conversation_id,
            "message_id": assistant_message_id,
            "mode": mode,
            "rewritten_query": effective_query,
        }),
    )
    .await;
    let mut tuning = SearchTuning::from_retrieval(&state.config.retrieval);
    tuning.lang = policy.lang.clone();
    tuning.billing_local_ledger = state.config.billing.engine.eq_ignore_ascii_case("local");
    let results = if mode == "answer" {
        None
    } else {
        Some(
            search_with_tuning(
                &state.store,
                &state.providers,
                &policy,
                &effective_query,
                SearchMode::Auto,
                10,
                &tuning,
            )
            .await?,
        )
    };
    emit_stream(&stream, "results", &json!(&results)).await;
    let answer = if mode == "search" {
        None
    } else {
        let mut answer = if let Some(stream_tx) = &stream {
            let (delta_tx, mut delta_rx) = mpsc::channel::<String>(64);
            let sse_tx = stream_tx.clone();
            let forwarder = tokio::spawn(async move {
                while let Some(delta) = delta_rx.recv().await {
                    if sse_tx
                        .send(sse_frame("token", &json!({"content": delta})))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            });
            let answer = crate::answer::build_answer_stream_with_ledger(
                &state.store,
                &state.providers,
                &policy,
                &effective_query,
                &AnswerOptions {
                    mode: SearchMode::Auto,
                    limit: 8,
                    tuning,
                },
                delta_tx,
                state.config.billing.engine.eq_ignore_ascii_case("local"),
            )
            .await?;
            let _ = forwarder.await;
            answer
        } else {
            build_answer_with_ledger(
                &state.store,
                &state.providers,
                &policy,
                &effective_query,
                &AnswerOptions {
                    mode: SearchMode::Auto,
                    limit: 8,
                    tuning,
                },
                state.config.billing.engine.eq_ignore_ascii_case("local"),
            )
            .await?
        };
        answer.conversation_id = Some(conversation.conversation_id.clone());
        answer.summary_kind = Some("dynamic".to_string());
        answer.summary_source = Some(match answer.mode {
            crate::contract::AnswerMode::Generative => "llm".to_string(),
            _ => "extractive".to_string(),
        });
        Some(answer)
    };
    if let Some(answer) = &answer {
        for citation in &answer.citations {
            emit_stream(
                &stream,
                "citation",
                &serde_json::to_value(citation).unwrap_or(Value::Null),
            )
            .await;
        }
        emit_stream(
            &stream,
            "usage",
            &serde_json::to_value(&answer.usage).unwrap_or(Value::Null),
        )
        .await;
        emit_stream(
            &stream,
            "degraded",
            &serde_json::to_value(&answer.degraded).unwrap_or(Value::Null),
        )
        .await;
    }
    state
        .store
        .insert_message(&MessageRecord {
            message_id: user_message_id.clone(),
            conversation_id: conversation.conversation_id.clone(),
            role: "user".to_string(),
            content: request.query.clone(),
            citations_json: "[]".to_string(),
            model: None,
            usage_json: None,
            degraded_json: None,
            request_id: request_id.clone(),
            mode: mode.to_string(),
            rewritten_query: if effective_query != request.query {
                Some(effective_query.clone())
            } else {
                None
            },
            conversation_context_hash: if context_hash.is_empty() {
                None
            } else {
                Some(context_hash.clone())
            },
            created_at: now_rfc3339(),
        })
        .await?;
    let (content, citations, model, usage, degraded) = if let Some(answer) = &answer {
        (
            answer.answer.clone().unwrap_or_default(),
            serde_json::to_string(&answer.citations).unwrap_or_else(|_| "[]".to_string()),
            answer.model.clone(),
            serde_json::to_string(&answer.usage).unwrap_or_else(|_| "null".to_string()),
            serde_json::to_string(&answer.degraded).unwrap_or_else(|_| "[]".to_string()),
        )
    } else {
        let hit = results
            .as_ref()
            .and_then(|results| results.hits.first())
            .map(|hit| hit.title.clone())
            .unwrap_or_default();
        let citations = results
            .as_ref()
            .map(|results| {
                serde_json::to_string(
                    &results
                        .hits
                        .iter()
                        .map(|hit| {
                            json!({
                                "doc_id": hit.doc_id,
                                "chunk_id": hit.chunk_id,
                                "path": hit.path,
                                "line_start": hit.line_start,
                                "line_end": hit.line_end,
                                "quote": crate::util::truncate_chars(&hit.quote, 500),
                            })
                        })
                        .collect::<Vec<_>>(),
                )
                .unwrap_or_else(|_| "[]".to_string())
            })
            .unwrap_or_else(|| "[]".to_string());
        (hit, citations, None, "null".to_string(), "[]".to_string())
    };
    state
        .store
        .insert_message(&MessageRecord {
            message_id: assistant_message_id.clone(),
            conversation_id: conversation.conversation_id.clone(),
            role: "assistant".to_string(),
            content,
            citations_json: citations,
            model,
            usage_json: Some(usage),
            degraded_json: Some(degraded),
            request_id: request_id.clone(),
            mode: mode.to_string(),
            rewritten_query: if effective_query != request.query {
                Some(effective_query.clone())
            } else {
                None
            },
            conversation_context_hash: if context_hash.is_empty() {
                None
            } else {
                Some(context_hash.clone())
            },
            created_at: now_rfc3339(),
        })
        .await?;
    state
        .store
        .update_conversation(
            &principal.tenant_id,
            &conversation.conversation_id,
            None,
            Some("active"),
        )
        .await?;
    let mut answer_json = answer.map(|answer| serde_json::to_value(answer).unwrap_or(Value::Null));
    if let Some(answer) = answer_json.as_mut() {
        answer["message_id"] = json!(assistant_message_id);
    }
    let value = json!({
        "conversation_id": conversation.conversation_id,
        "message_id": assistant_message_id,
        "user_message_id": user_message_id,
        "mode": mode,
        "rewritten_query": effective_query,
        "conversation_context_hash": context_hash,
        "degraded": if let Some(error) = rewrite_error { vec![json!({"code": error})] } else { Vec::<Value>::new() },
        "results": results,
        "answer": answer_json,
    });
    emit_stream(
        &stream,
        "done",
        &json!({
            "conversation_id": value["conversation_id"],
            "message_id": value["message_id"],
        }),
    )
    .await;
    Ok(value)
}

async fn query_stream_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<QueryRequest>,
) -> Result<Response, AppError> {
    let (tx, rx) = mpsc::channel::<Bytes>(64);
    let task_tx = tx.clone();
    tokio::spawn(async move {
        if let Err(err) =
            run_query(State(state), headers, Json(request), Some(task_tx.clone())).await
        {
            let _ = task_tx
                .send(sse_frame(
                    "error",
                    &json!({"code": err.code(), "message": err.message()}),
                ))
                .await;
        }
        drop(task_tx);
    });
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from_stream(
            ReceiverStream::new(rx).map(Ok::<_, std::convert::Infallible>),
        ))
        .map_err(|err| AppError::internal(format!("stream response failed: {err}")))?;
    Ok(response)
}

async fn list_conversations_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let (principal, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Read,
        "conversation.list",
        None,
    )
    .await
    .map(|(principal, _session, guard)| (principal, guard))?;
    let limit = query
        .get("limit")
        .and_then(|value| value.parse().ok())
        .unwrap_or(50);
    let cursor = query.get("cursor");
    let external = query.get("external_session_id");
    let mut conversations = state
        .store
        .list_conversations(&principal.tenant_id, &principal.principal_id, 500)
        .await?;
    if let Some(external) = external {
        conversations.retain(|conversation| {
            conversation.external_session_id.as_deref() == Some(external.as_str())
        });
    }
    if let Some(cursor) = cursor {
        conversations.retain(|conversation| conversation.updated_at.as_str() < cursor.as_str());
    }
    let next_cursor = if conversations.len() > limit as usize {
        conversations
            .get(limit as usize - 1)
            .map(|conversation| conversation.updated_at.clone())
    } else {
        None
    };
    conversations.truncate(limit as usize);
    Ok(Json(
        json!({"conversations": conversations, "next_cursor": next_cursor}),
    ))
}

async fn create_conversation_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ConversationCreate>,
) -> Result<Response, AppError> {
    let (principal, session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Read,
        "conversation.create",
        None,
    )
    .await?;
    v1_session_write_guard(&state, &headers, session)?;
    let space = request
        .space
        .or_else(|| principal.allowed_space_ids.first().cloned())
        .ok_or_else(|| AppError::forbidden("no space available"))?;
    require_space_role(
        &state,
        &principal,
        &space,
        SpaceRole::Viewer,
        "conversation.create",
        &headers,
    )
    .await?;
    let now = now_rfc3339();
    let conversation = ConversationRecord {
        conversation_id: format!("conv_{}", random_hex(10)),
        tenant_id: principal.tenant_id.clone(),
        space_id: space,
        owner_id: principal.principal_id.clone(),
        title: request.title.unwrap_or_else(|| "Conversation".to_string()),
        external_session_id: None,
        status: "active".to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    state.store.create_conversation(&conversation).await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"conversation": conversation})),
    )
        .into_response())
}

#[derive(Debug, Deserialize)]
struct ExternalSessionRequest {
    external_session_id: String,
}

async fn bind_external_session_v1(
    State(state): State<AppState>,
    AxumPath(conversation_id): AxumPath<String>,
    headers: HeaderMap,
    Json(request): Json<ExternalSessionRequest>,
) -> Result<Json<Value>, AppError> {
    let (principal, session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Read,
        "conversation.external_session",
        None,
    )
    .await?;
    v1_session_write_guard(&state, &headers, session)?;
    let conversation = state
        .store
        .get_conversation(&principal.tenant_id, &conversation_id)
        .await?
        .ok_or_else(|| AppError::not_found("conversation not found"))?;
    if conversation.owner_id != principal.principal_id {
        return Err(AppError::forbidden("conversation belongs to another owner"));
    }
    state
        .store
        .bind_conversation_external_session(
            &principal.tenant_id,
            &conversation_id,
            &request.external_session_id,
        )
        .await?;
    Ok(Json(
        json!({"conversation_id": conversation_id, "external_session_id": request.external_session_id}),
    ))
}

async fn get_conversation_v1(
    State(state): State<AppState>,
    AxumPath(conversation_id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, _limits) =
        v1_auth(&state, &headers, TokenScope::Read, "conversation.get", None)
            .await
            .map(|(principal, _session, guard)| (principal, guard))?;
    let conversation = state
        .store
        .get_conversation(&principal.tenant_id, &conversation_id)
        .await?
        .ok_or_else(|| AppError::not_found("conversation not found"))?;
    if conversation.owner_id != principal.principal_id || conversation.status == "deleted" {
        return Err(AppError::forbidden("conversation belongs to another owner"));
    }
    let messages = state.store.list_messages(&conversation_id, 1000).await?;
    Ok(Json(
        json!({"conversation": conversation, "messages": messages}),
    ))
}

async fn delete_conversation_v1(
    State(state): State<AppState>,
    AxumPath(conversation_id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let (principal, session, _limits) = v1_auth(
        &state,
        &headers,
        TokenScope::Read,
        "conversation.delete",
        None,
    )
    .await?;
    v1_session_write_guard(&state, &headers, session)?;
    let conversation = state
        .store
        .get_conversation(&principal.tenant_id, &conversation_id)
        .await?
        .ok_or_else(|| AppError::not_found("conversation not found"))?;
    if conversation.owner_id != principal.principal_id {
        return Err(AppError::forbidden("conversation belongs to another owner"));
    }
    state
        .store
        .update_conversation(
            &principal.tenant_id,
            &conversation_id,
            None,
            Some("deleted"),
        )
        .await?;
    Ok(Json(
        json!({"conversation_id": conversation_id, "status": "deleted"}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_check_rejects_cross_origin() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("localhost:7331"));
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://evil.example"),
        );
        assert!(check_origin(&headers).is_err());
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://localhost:7331"),
        );
        assert!(check_origin(&headers).is_ok());
    }

    #[test]
    fn limiter_locks_after_five_failures() {
        let limiter = LoginLimiter::default();
        for _ in 0..5 {
            limiter.record_failure("ip|user");
        }
        assert!(!limiter.allowed("ip|user"));
        limiter.clear("ip|user");
        assert!(limiter.allowed("ip|user"));
    }

    #[test]
    fn session_storage_only_keeps_a_hash() {
        let raw = "session-id-that-must-not-be-stored";
        let stored = sha256_hex_str(raw);
        assert_ne!(stored, raw);
        assert_eq!(stored.len(), 64);
        assert_eq!(stored, sha256_hex_str(raw));
    }
}

    use crate::config::Backend;
    use crate::governance::{SOLO_SPACE, SOLO_TENANT};
    use axum::body::to_bytes;
    use tower::ServiceExt;

    fn write_web_file(root: &std::path::Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    async fn static_app(web_root: &std::path::Path) -> (Router, tempfile::TempDir) {
        let db = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.backend = Backend::Sqlite;
        config.storage.sqlite_path = db.path().join("lkb.sqlite");
        config.storage.tenant_id = SOLO_TENANT.to_string();
        config.storage.default_space = SOLO_SPACE.to_string();
        config.server.web_dir = Some(web_root.to_path_buf());
        let store = crate::store::open(&config).await.unwrap();
        crate::store::bootstrap_space(&store, &config)
            .await
            .unwrap();
        let state = AppState::new(
            store,
            Arc::new(config),
            Arc::new(crate::providers::ProviderSet::with_none()),
        );
        (router(state), db)
    }

    async fn static_get(app: &Router, uri: &str) -> Response {
        app.clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn static_files_serve_index_assets_and_head() {
        let web = tempfile::tempdir().unwrap();
        let index_html = "<!doctype html><div id=\"root\">spa</div>";
        write_web_file(web.path(), "index.html", index_html);
        write_web_file(web.path(), "assets/app-abc123.js", "console.log('app')");
        let (app, _db) = static_app(web.path()).await;

        let index = static_get(&app, "/").await;
        assert_eq!(index.status(), StatusCode::OK);
        assert_eq!(
            index.headers()[header::CONTENT_TYPE].to_str().unwrap(),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            index.headers()[header::CACHE_CONTROL].to_str().unwrap(),
            "no-cache"
        );
        let body = to_bytes(index.into_body(), usize::MAX).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("spa"));

        let asset = static_get(&app, "/assets/app-abc123.js").await;
        assert_eq!(asset.status(), StatusCode::OK);
        assert_eq!(
            asset.headers()[header::CONTENT_TYPE].to_str().unwrap(),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            asset.headers()[header::CACHE_CONTROL].to_str().unwrap(),
            "public, max-age=31536000, immutable"
        );

        let head = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::HEAD)
                    .uri("/index.html")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(head.status(), StatusCode::OK);
        assert_eq!(
            head.headers()[header::CONTENT_LENGTH].to_str().unwrap(),
            index_html.len().to_string()
        );
        let head_body = to_bytes(head.into_body(), usize::MAX).await.unwrap();
        assert!(head_body.is_empty());
    }

    #[tokio::test]
    async fn spa_fallback_only_for_extensionless_paths() {
        let web = tempfile::tempdir().unwrap();
        write_web_file(web.path(), "index.html", "<div id=\"root\">spa</div>");
        let (app, _db) = static_app(web.path()).await;

        let deep = static_get(&app, "/wiki/some/deep/link").await;
        assert_eq!(deep.status(), StatusCode::OK);
        let body = to_bytes(deep.into_body(), usize::MAX).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("spa"));

        // A missing asset with an extension stays a 404; it must not silently
        // turn into the HTML shell.
        let missing = static_get(&app, "/assets/missing-xyz.js").await;
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn spa_fallback_without_index_is_a_plain_404() {
        let web = tempfile::tempdir().unwrap();
        write_web_file(web.path(), "assets/app.js", "console.log('app')");
        let (app, _db) = static_app(web.path()).await;

        let deep = static_get(&app, "/wiki/some/page").await;
        assert_eq!(deep.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn api_routes_and_health_are_not_swallowed_by_static_hosting() {
        let web = tempfile::tempdir().unwrap();
        write_web_file(web.path(), "index.html", "<div id=\"root\">spa</div>");
        let (app, _db) = static_app(web.path()).await;

        let health = static_get(&app, "/health").await;
        assert_eq!(health.status(), StatusCode::OK);
        assert_eq!(
            health.headers()[header::CONTENT_TYPE].to_str().unwrap(),
            "application/json"
        );

        let openapi = static_get(&app, "/openapi.json").await;
        assert_eq!(openapi.status(), StatusCode::OK);
        let body = to_bytes(openapi.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("\"openapi\""), "unexpected body: {text}");
        assert!(!text.contains("<div id=\"root\">"), "served the SPA shell");

        let unknown_api = static_get(&app, "/api/v1/not-a-route").await;
        assert_eq!(unknown_api.status(), StatusCode::NOT_FOUND);
        let body = to_bytes(unknown_api.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("not_found"), "unexpected body: {text}");
    }

    #[tokio::test]
    async fn traversal_outside_the_web_dir_is_rejected() {
        let parent = tempfile::tempdir().unwrap();
        let web = parent.path().join("web");
        std::fs::create_dir_all(&web).unwrap();
        write_web_file(&web, "index.html", "<div>spa</div>");
        std::fs::write(parent.path().join("secret.txt"), "top secret").unwrap();
        let (app, _db) = static_app(&web).await;

        for uri in [
            "/../secret.txt",
            "/%2e%2e/secret.txt",
            "/%2E%2E/secret.txt",
            "/..%2fsecret.txt",
            "/%2e%2e%2fsecret.txt",
            "/assets/%2e%2e/%2e%2e/secret.txt",
            "/..\\secret.txt",
        ] {
            let response = static_get(&app, uri).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "uri {uri}");
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            assert!(
                !String::from_utf8_lossy(&body).contains("top secret"),
                "uri {uri} leaked the file"
            );
        }
    }
