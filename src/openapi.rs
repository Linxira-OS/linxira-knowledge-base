//! Static OpenAPI 3.1 document. It is intentionally data-only so tests can
//! assert that documented paths exist in the router. Only implemented
//! endpoints are listed, so the document never claims a path the router does
//! not serve.

use serde_json::{json, Map, Value};

fn op(summary: &str, scope: &str, method: &str) -> Value {
    json!({
        "summary": summary,
        "security": if scope == "public" { json!([]) } else { json!([{"bearerAuth": []}]) },
        "responses": {"200": {"description": "OK"}},
        "x-lkb-scope": scope,
        "x-lkb-method": method,
    })
}

pub fn document() -> Value {
    let mut paths = Map::new();
    let mut add = |path: &str, method: &str, summary: &str, scope: &str| {
        let entry = paths
            .entry(path.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        entry
            .as_object_mut()
            .expect("path item")
            .insert(method.to_lowercase(), op(summary, scope, method));
    };
    add("/openapi.json", "get", "OpenAPI document", "public");
    add("/health", "get", "Health", "public");
    add("/metrics", "get", "Prometheus metrics", "public");
    add("/api/v1/query", "post", "Unified search/answer/both", "kb.read");
    add("/api/v1/query/stream", "post", "Streaming query (SSE)", "kb.read");
    add("/api/v1/search", "get", "Search", "kb.read");
    add("/api/v1/answer", "post", "Answer", "kb.read");
    add("/api/v1/answer/stream", "get", "Streaming answer (SSE)", "kb.read");
    add("/api/v1/pages/{space}/{slug}", "get", "Read page JSON", "kb.read");
    add("/api/v1/pages/{space}/{slug}", "put", "Save page (If-Match)", "kb.write");
    add("/api/v1/pages/{space}/{slug}", "delete", "Archive page", "kb.write");
    add("/api/v1/pages/{space}/{slug}/history", "get", "Page revisions", "kb.read");
    add("/api/v1/pages/{space}/{slug}/restore", "post", "Restore page", "kb.write");
    add("/api/v1/pages/{space}/{slug}/summary", "get", "Page summary", "kb.read");
    add("/api/v1/spaces", "get", "List spaces", "kb.read");
    add("/api/v1/spaces", "post", "Create space", "kb.write");
    add("/api/v1/spaces/{space_id}", "get", "Get space", "kb.read");
    add("/api/v1/spaces/{space_id}", "patch", "Update space", "kb.admin");
    add("/api/v1/spaces/{space_id}", "delete", "Delete space", "kb.admin");
    add("/api/v1/spaces/{space_id}/usage", "get", "Space usage", "kb.read");
    add("/api/v1/spaces/{space_id}/quota", "put", "Set space quota", "kb.admin");
    add("/api/v1/spaces/{space_id}/members", "get", "List members", "kb.read");
    add("/api/v1/spaces/{space_id}/members", "post", "Add member", "kb.admin");
    add("/api/v1/groups", "get", "List groups", "kb.read");
    add("/api/v1/groups", "post", "Create group", "kb.write");
    add("/api/v1/groups/{group_id}", "get", "Get group", "kb.read");
    add("/api/v1/groups/{group_id}", "patch", "Update group", "kb.admin");
    add("/api/v1/groups/{group_id}", "delete", "Delete group", "kb.admin");
    add("/api/v1/auth/sso/status", "get", "Trusted-header SSO status", "public");
    add("/api/v1/auth/csrf", "get", "Issue CSRF cookie/token", "public");
    add("/api/v1/auth/login", "post", "JSON login (session cookie)", "public");
    add("/api/v1/auth/me", "get", "Current principal", "kb.read");
    add("/api/v1/auth/logout", "post", "Logout", "kb.read");
    add("/api/v1/pages", "get", "List visible pages", "kb.read");
    add("/api/v1/pages/{space}/{slug}/revisions/{revision_id}", "get", "Read one revision", "kb.read");
    add("/api/v1/conversations", "get", "List conversations", "kb.read");
    add("/api/v1/conversations", "post", "Create conversation", "kb.read");
    add("/api/v1/conversations/{conversation_id}", "get", "Get conversation", "kb.read");
    add("/api/v1/conversations/{conversation_id}", "delete", "Delete conversation", "kb.write");
    add("/api/v1/conversations/{conversation_id}/external-session", "post", "Bind external session", "kb.write");
    add("/api/v1/conversations/{conversation_id}/restore", "post", "Restore conversation", "kb.write");
    add("/api/v1/features", "get", "Feature flags", "kb.read");
    add("/api/v1/gateway/chat", "post", "Gateway chat", "kb.read");
    add("/api/v1/billing/checkout", "post", "Billing checkout", "kb.write");
    add("/api/v1/billing/plans", "get", "Billing plans", "kb.read");
    add("/api/v1/billing/wallet", "get", "Wallet and entitlement", "kb.read");
    add("/api/v1/billing/ledger", "get", "Wallet ledger", "kb.read");
    add("/api/v1/billing/invoices", "get", "Invoices", "kb.read");
    add("/api/v1/billing/topup", "post", "Manual/mock topup", "kb.write");
    add("/api/v1/billing/subscribe", "post", "Subscribe to a plan", "kb.write");
    add("/api/v1/billing/subscription", "get", "Current subscription", "kb.read");
    add("/api/v1/billing/subscription", "patch", "Transition subscription", "kb.write");
    add("/api/v1/billing/entitlement", "get", "Entitlement snapshot", "kb.read");
    add("/api/v1/billing/webhook/{provider}", "post", "Payment webhook (signed)", "public");
    add("/api/v1/admin/users", "get", "Admin: list users", "kb.admin");
    add("/api/v1/admin/users/{user_id}", "patch", "Admin: update user", "kb.admin");
    add("/api/v1/admin/users/{user_id}/disable", "post", "Admin: disable user", "kb.admin");
    add("/api/v1/admin/roles", "get", "Admin: list roles", "kb.admin");
    add("/api/v1/admin/groups", "get", "Admin: list groups", "kb.admin");
    add("/api/v1/admin/spaces", "get", "Admin: list spaces", "kb.admin");
    add("/api/v1/admin/providers", "get", "Admin: providers", "kb.admin");
    add("/api/v1/admin/providers", "post", "Admin: create provider", "kb.admin");
    add("/api/v1/admin/providers/{provider_id}", "patch", "Admin: update provider", "kb.admin");
    add("/api/v1/admin/provider-models", "get", "Admin: provider models", "kb.admin");
    add("/api/v1/admin/aliases", "get", "Admin: aliases", "kb.admin");
    add("/api/v1/admin/aliases", "post", "Admin: create alias", "kb.admin");
    add("/api/v1/admin/routes", "get", "Admin: routes", "kb.admin");
    add("/api/v1/admin/routes", "post", "Admin: create route", "kb.admin");
    add("/api/v1/admin/quotas", "get", "Admin: quotas", "kb.admin");
    add("/api/v1/admin/quotas", "post", "Admin: create quota", "kb.admin");
    add("/api/v1/admin/usage", "get", "Admin: usage events", "kb.admin");
    add("/api/v1/admin/usage/aggregate", "get", "Admin: usage aggregates", "kb.admin");
    add("/api/v1/connectors", "get", "List connectors", "kb.admin");
    add("/api/v1/connectors", "post", "Create connector", "kb.admin");
    add("/api/v1/connectors/{connector_id}", "patch", "Update connector", "kb.admin");
    add("/api/v1/connectors/{connector_id}", "delete", "Delete connector", "kb.admin");
    add("/api/v1/connectors/{connector_id}/test", "post", "Test connector", "kb.admin");
    add("/api/v1/connectors/{connector_id}/sync", "post", "Queue connector sync", "kb.admin");
    add("/api/v1/connectors/{connector_id}/jobs", "get", "Connector jobs", "kb.admin");
    add("/api/v1/external-refs", "get", "External references", "kb.admin");
    add("/api/v1/admin/audit", "get", "Admin: audit events", "kb.admin");
    add("/api/v1/admin/quota-overrides", "post", "Admin: quota override", "kb.admin");

    let schemas = json!({
        "Answer": {"type": "object", "required": ["mode"], "properties": {
            "answer": {"type": ["string", "null"]}, "citations": {"type": "array", "items": {"$ref": "#/components/schemas/Citation"}},
            "confidence": {"type": "object"}, "conflicts": {"type": "array"}, "degraded": {"type": "array", "items": {"$ref": "#/components/schemas/Degraded"}},
            "knowledge_as_of": {"type": ["string", "null"]}, "mode": {"type": "string"}, "model": {"type": ["string", "null"]},
            "usage": {"type": ["object", "null"]}, "cache_hit": {"type": "boolean"}, "schema_version": {"type": "string"}}},
        "Citation": {"type": "object", "required": ["doc_id", "chunk_id"], "properties": {
            "doc_id": {"type": "string"}, "chunk_id": {"type": "string"}, "path": {"type": ["string", "null"]},
            "line_start": {"type": ["integer", "null"]}, "line_end": {"type": ["integer", "null"]}, "quote": {"type": ["string", "null"]}}},
        "Degraded": {"type": "object", "required": ["code"], "properties": {"code": {"type": "string"}, "category": {"type": "string"}, "message": {"type": ["string", "null"]}}},
        "Space": {"type": "object", "properties": {"space_id": {"type": "string"}, "name": {"type": "string"}, "status": {"type": "string"}, "storage_quota_bytes": {"type": "integer"}}},
        "User": {"type": "object", "properties": {"user_id": {"type": "string"}, "username": {"type": "string"}, "display_name": {"type": "string"}, "role": {"type": "string"}, "disabled": {"type": "boolean"}}},
        "Quota": {"type": "object", "properties": {"quota_id": {"type": "string"}, "scope_type": {"type": "string"}, "scope_id": {"type": "string"}, "period": {"type": "string"}, "limit_tokens": {"type": "integer"}, "enforcement": {"type": "string"}}},
        "Usage": {"type": "object", "properties": {"usage_event_id": {"type": "string"}, "source": {"type": "string"}, "alias": {"type": "string"}, "input_tokens": {"type": "integer"}, "output_tokens": {"type": "integer"}, "cost_micros": {"type": "integer"}, "at": {"type": "string"}}},
        "Conversation": {"type": "object", "properties": {"conversation_id": {"type": "string"}, "space_id": {"type": "string"}, "owner_id": {"type": "string"}, "title": {"type": "string"}, "external_session_id": {"type": ["string", "null"]}, "status": {"type": "string"}, "updated_at": {"type": "string"}}},
        "Summary": {"type": "object", "properties": {"doc_id": {"type": "string"}, "revision_id": {"type": "string"}, "lang": {"type": "string"}, "model": {"type": "string"}, "status": {"type": "string"}, "summary": {"type": "object"}, "generated_at": {"type": ["string", "null"]}}}
    });

    json!({
        "openapi": "3.1.0",
        "info": {"title": "lkb API", "version": env!("CARGO_PKG_VERSION"), "description": "Layered knowledge base REST API."},
        "servers": [{"url": "/"}],
        "components": {
            "securitySchemes": {"bearerAuth": {"type": "http", "scheme": "bearer"}},
            "schemas": schemas
        },
        "paths": paths
    })
}
