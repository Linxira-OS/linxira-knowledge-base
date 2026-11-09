//! External knowledge connectors. The source system remains the authority:
//! we keep text snapshots, locators and ACL mirrors, never binaries or source
//! credentials. Connectors only speak HTTP/files.

use crate::config::Config;
use crate::governance::Acl;
use crate::providers::ProviderSet;
use crate::store::{
    ConnectorJobRecord, ConnectorRecord, DocumentRecord, ExternalRefRecord, Store,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

pub const SUPPORTED_KINDS: &[&str] = &[
    "filesystem_export",
    "jsonl",
    "http_json",
    "feishu",
    "sharepoint",
    "onedrive",
    "confluence",
    "mediawiki",
];

#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("connector {0} is not configured for this build")]
    NotConfigured(String),
    #[error("invalid connector configuration: {0}")]
    Invalid(String),
    #[error("connector I/O failed: {0}")]
    Io(String),
    #[error("connector HTTP request failed: {0}")]
    Http(String),
}

impl ConnectorError {
    pub fn code(&self) -> &'static str {
        match self {
            ConnectorError::NotConfigured(_) => "connector_not_configured",
            ConnectorError::Invalid(_) => "connector_invalid_config",
            ConnectorError::Io(_) => "connector_io_error",
            ConnectorError::Http(_) => "connector_http_error",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorItem {
    pub external_id: String,
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub acl: Option<Value>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub mime: Option<String>,
    #[serde(default)]
    pub lang: Option<String>,
}

#[async_trait]
pub trait Connector: Send + Sync {
    fn kind(&self) -> &'static str;
    async fn test(&self) -> Result<Value, ConnectorError>;
    async fn list_items(
        &self,
        cursor: Option<String>,
    ) -> Result<(Vec<ConnectorItem>, Option<String>), ConnectorError>;
    async fn fetch_item(&self, external_id: &str) -> Result<ConnectorItem, ConnectorError> {
        let (items, _) = self.list_items(None).await?;
        items
            .into_iter()
            .find(|item| item.external_id == external_id)
            .ok_or_else(|| ConnectorError::Invalid(format!("item {external_id:?} not found")))
    }
    fn deep_link(&self, item: &ConnectorItem) -> String {
        item.url.clone()
    }
    fn acl_snapshot(&self, item: &ConnectorItem) -> Acl {
        item.acl
            .as_ref()
            .and_then(|value| Acl::from_json(&value.to_string()).ok())
            .unwrap_or_else(|| Acl::allow_list(Vec::new(), Vec::new()))
    }
}

fn parse_items(value: &Value) -> Result<Vec<ConnectorItem>, ConnectorError> {
    let array = if let Some(array) = value.as_array() {
        array.clone()
    } else if let Some(array) = value.get("items").and_then(Value::as_array) {
        array.clone()
    } else {
        return Err(ConnectorError::Invalid("expected an items array".to_string()));
    };
    array
        .into_iter()
        .map(|item| {
            let external_id = item
                .get("external_id")
                .or_else(|| item.get("id"))
                .or_else(|| item.get("path"))
                .or_else(|| item.get("url"))
                .and_then(Value::as_str)
                .ok_or_else(|| ConnectorError::Invalid("item is missing external_id/id/path/url".to_string()))?
                .to_string();
            let url = item.get("url").and_then(Value::as_str).unwrap_or("").to_string();
            let title = item
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or(external_id.as_str())
                .to_string();
            Ok(ConnectorItem {
                external_id,
                url,
                title,
                revision: item.get("revision").and_then(|value| value.as_str()).map(str::to_string),
                acl: item.get("acl").cloned(),
                content: item.get("content").and_then(Value::as_str).map(str::to_string),
                mime: item.get("mime").and_then(Value::as_str).map(str::to_string),
                lang: item.get("lang").and_then(Value::as_str).map(str::to_string),
            })
        })
        .collect()
}

struct FilesystemExport {
    items: Vec<ConnectorItem>,
}

impl FilesystemExport {
    fn from_config(config: &Value) -> Result<Self, ConnectorError> {
        let dir = config
            .get("dir")
            .and_then(Value::as_str)
            .ok_or_else(|| ConnectorError::Invalid("filesystem_export requires config.dir".to_string()))?;
        let manifest = config.get("manifest").and_then(Value::as_str).unwrap_or("manifest.json");
        let path = std::path::Path::new(dir).join(manifest);
        let text = std::fs::read_to_string(&path)
            .map_err(|err| ConnectorError::Io(format!("{}: {err}", path.display())))?;
        let value: Value = serde_json::from_str(&text)
            .map_err(|err| ConnectorError::Invalid(format!("manifest json: {err}")))?;
        let items = parse_items(&value)?;
        Ok(Self { items })
    }
}

#[async_trait]
impl Connector for FilesystemExport {
    fn kind(&self) -> &'static str {
        "filesystem_export"
    }

    async fn test(&self) -> Result<Value, ConnectorError> {
        Ok(json!({"ok": true, "items": self.items.len()}))
    }

    async fn list_items(
        &self,
        cursor: Option<String>,
    ) -> Result<(Vec<ConnectorItem>, Option<String>), ConnectorError> {
        let start: usize = cursor.and_then(|value| value.parse().ok()).unwrap_or(0);
        let page: Vec<ConnectorItem> = self.items.iter().skip(start).take(100).cloned().collect();
        let next = if start + page.len() < self.items.len() {
            Some((start + page.len()).to_string())
        } else {
            None
        };
        Ok((page, next))
    }
}

struct JsonlConnector {
    items: Vec<ConnectorItem>,
}

impl JsonlConnector {
    fn from_config(config: &Value) -> Result<Self, ConnectorError> {
        let path = config
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| ConnectorError::Invalid("jsonl requires config.path".to_string()))?;
        let text = std::fs::read_to_string(path)
            .map_err(|err| ConnectorError::Io(format!("{path}: {err}")))?;
        let mut items = Vec::new();
        for (index, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let value: Value = serde_json::from_str(line)
                .map_err(|err| ConnectorError::Invalid(format!("line {}: {err}", index + 1)))?;
            items.extend(parse_items(&Value::Array(vec![value]))?);
        }
        Ok(Self { items })
    }
}

#[async_trait]
impl Connector for JsonlConnector {
    fn kind(&self) -> &'static str {
        "jsonl"
    }

    async fn test(&self) -> Result<Value, ConnectorError> {
        Ok(json!({"ok": true, "items": self.items.len()}))
    }

    async fn list_items(
        &self,
        cursor: Option<String>,
    ) -> Result<(Vec<ConnectorItem>, Option<String>), ConnectorError> {
        let start: usize = cursor.and_then(|value| value.parse().ok()).unwrap_or(0);
        let page: Vec<ConnectorItem> = self.items.iter().skip(start).take(100).cloned().collect();
        let next = if start + page.len() < self.items.len() {
            Some((start + page.len()).to_string())
        } else {
            None
        };
        Ok((page, next))
    }
}

struct HttpJsonConnector {
    base_url: String,
    api_key_env: Option<String>,
    client: reqwest::Client,
}

impl HttpJsonConnector {
    fn from_config(config: &Value) -> Result<Self, ConnectorError> {
        let base_url = config
            .get("base_url")
            .and_then(Value::as_str)
            .ok_or_else(|| ConnectorError::Invalid("http_json requires config.base_url".to_string()))?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key_env: config.get("api_key_env").and_then(Value::as_str).map(str::to_string),
            client: reqwest::Client::new(),
        })
    }

    async fn fetch(&self) -> Result<Vec<ConnectorItem>, ConnectorError> {
        let url = self.base_url.clone();
        let mut request = self.client.get(url).timeout(std::time::Duration::from_secs(15));
        if let Some(env_name) = &self.api_key_env {
            if let Ok(key) = std::env::var(env_name) {
                if !key.trim().is_empty() {
                    request = request.bearer_auth(key);
                }
            }
        }
        let response = request.send().await.map_err(|err| ConnectorError::Http(err.to_string()))?;
        let status = response.status();
        let value: Value = response.json().await.map_err(|err| ConnectorError::Http(err.to_string()))?;
        if !status.is_success() {
            return Err(ConnectorError::Http(format!("HTTP {status}")));
        }
        parse_items(&value)
    }
}

#[async_trait]
impl Connector for HttpJsonConnector {
    fn kind(&self) -> &'static str {
        "http_json"
    }

    async fn test(&self) -> Result<Value, ConnectorError> {
        let items = self.fetch().await?;
        Ok(json!({"ok": true, "items": items.len()}))
    }

    async fn list_items(
        &self,
        _cursor: Option<String>,
    ) -> Result<(Vec<ConnectorItem>, Option<String>), ConnectorError> {
        Ok((self.fetch().await?, None))
    }
}

struct StubConnector {
    kind: &'static str,
}

#[async_trait]
impl Connector for StubConnector {
    fn kind(&self) -> &'static str {
        self.kind
    }

    async fn test(&self) -> Result<Value, ConnectorError> {
        Err(ConnectorError::NotConfigured(self.kind.to_string()))
    }

    async fn list_items(
        &self,
        _cursor: Option<String>,
    ) -> Result<(Vec<ConnectorItem>, Option<String>), ConnectorError> {
        Err(ConnectorError::NotConfigured(self.kind.to_string()))
    }
}

pub fn from_record(record: &ConnectorRecord) -> Result<Box<dyn Connector>, ConnectorError> {
    let config: Value = serde_json::from_str(&record.config_json)
        .map_err(|err| ConnectorError::Invalid(format!("config_json: {err}")))?;
    match record.kind.as_str() {
        "filesystem_export" => Ok(Box::new(FilesystemExport::from_config(&config)?)),
        "jsonl" => Ok(Box::new(JsonlConnector::from_config(&config)?)),
        "http_json" => Ok(Box::new(HttpJsonConnector::from_config(&config)?)),
        "feishu" => Ok(Box::new(StubConnector { kind: "feishu" })),
        "sharepoint" => Ok(Box::new(StubConnector { kind: "sharepoint" })),
        "onedrive" => Ok(Box::new(StubConnector { kind: "onedrive" })),
        "confluence" => Ok(Box::new(StubConnector { kind: "confluence" })),
        "mediawiki" => Ok(Box::new(StubConnector { kind: "mediawiki" })),
        other => Err(ConnectorError::Invalid(format!("unknown connector kind {other:?}"))),
    }
}

fn safe_external_path(external_id: &str) -> String {
    external_id
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') { ch } else { '_' })
        .collect()
}

/// Runs one sync job. Items are independent: one failure is recorded and the
/// rest continue. Content is indexed as text; no binary is stored.
pub async fn run_sync(
    store: Arc<dyn Store>,
    config: Arc<Config>,
    providers: Arc<ProviderSet>,
    connector: ConnectorRecord,
    job: ConnectorJobRecord,
) -> ConnectorJobRecord {
    let mut stats = json!({"items": 0, "indexed": 0, "refs_only": 0, "failed": 0, "errors": []});
    let mut errors: Vec<Value> = Vec::new();
    let connector_impl = match from_record(&connector) {
        Ok(connector_impl) => connector_impl,
        Err(err) => {
            let error = err.to_string();
            stats["failed"] = json!(1);
            errors.push(json!({"code": err.code(), "message": error}));
            stats["errors"] = json!(errors);
            let now = crate::util::now_rfc3339();
            let _ = store
                .connector_job_update(&job.job_id, "failed", Some(&now), &stats.to_string(), Some(&error))
                .await;
            return ConnectorJobRecord { status: "failed".into(), finished_at: Some(now), stats_json: stats.to_string(), error: Some(error), ..job };
        }
    };
    let mut cursor = None;
    let mut pages = 0;
    loop {
        let (items, next) = match connector_impl.list_items(cursor.clone()).await {
            Ok(result) => result,
            Err(err) => {
                let error = err.to_string();
                errors.push(json!({"code": err.code(), "message": error}));
                stats["failed"] = json!(errors.len());
                break;
            }
        };
        for item in items {
            stats["items"] = json!(stats["items"].as_i64().unwrap_or(0) + 1);
            let acl = connector_impl.acl_snapshot(&item);
            let acl_json = acl.to_json();
            let acl_principals = acl.principals_key();
            let now = crate::util::now_rfc3339();
            let mut reference = ExternalRefRecord {
                tenant_id: connector.tenant_id.clone(),
                space_id: connector.space_id.clone(),
                doc_id: String::new(),
                provider: connector.kind.clone(),
                external_id: item.external_id.clone(),
                url: connector_impl.deep_link(&item),
                revision: item.revision.clone(),
                acl_json: acl_json.clone(),
                fetched_at: now.clone(),
                status: "active".to_string(),
            };
            if let Some(content) = item.content.as_ref().filter(|content| !content.trim().is_empty()) {
                let relative = format!(
                    "connectors/{}/{}.md",
                    connector.kind,
                    safe_external_path(&item.external_id)
                );
                let canonical = match crate::ingest::doc_id::canonical_relative_path(&relative) {
                    Ok(canonical) => canonical,
                    Err(err) => {
                        errors.push(json!({"external_id": item.external_id, "code": "connector_invalid_path", "message": err}));
                        stats["failed"] = json!(stats["failed"].as_i64().unwrap_or(0) + 1);
                        continue;
                    }
                };
                let content_hash = crate::util::sha256_hex(content.as_bytes());
                let doc_id = crate::ingest::doc_id::derive_doc_id(&canonical, &content_hash);
                reference.doc_id = doc_id.clone();
                let drafts = crate::ingest::chunker::chunk_text(content, crate::ingest::chunker::DEFAULT_CHUNK_CHARS);
                let mut chunks = Vec::new();
                for (index, draft) in drafts.iter().enumerate() {
                    let chunk_hash = crate::util::sha256_hex(draft.text.as_bytes());
                    chunks.push(crate::store::ChunkRecord {
                        tenant_id: connector.tenant_id.clone(),
                        chunk_id: crate::ingest::doc_id::derive_chunk_id(&connector.tenant_id, &doc_id, index as u32, &chunk_hash),
                        doc_id: doc_id.clone(),
                        space_id: connector.space_id.clone(),
                        seq: index as u32,
                        text: draft.text.clone(),
                        line_start: draft.line_start,
                        line_end: draft.line_end,
                        content_hash: chunk_hash,
                        acl_json: acl_json.clone(),
                        acl_principals: acl_principals.clone(),
                        lang: item.lang.clone().unwrap_or_else(|| "zh".to_string()),
                        valid_from: now.clone(),
                        valid_to: None,
                        superseded_by: None,
                        version: 1,
                        embedding: None,
                    });
                }
                let mut chunks = chunks;
                if let Some(provider) = &providers.embedding {
                    let mut warnings = Vec::new();
                    crate::ingest::attach_embeddings(&mut chunks, provider.as_ref(), 32, &mut warnings).await;
                    for warning in warnings {
                        tracing::warn!(warning, "connector embedding");
                    }
                }
                let document = DocumentRecord {
                    tenant_id: connector.tenant_id.clone(),
                    doc_id: doc_id.clone(),
                    space_id: connector.space_id.clone(),
                    logical_path: canonical,
                    source_uri: reference.url.clone(),
                    title: item.title.clone(),
                    mime: item.mime.clone().unwrap_or_else(|| "text/markdown".to_string()),
                    raw_hash: content_hash.clone(),
                    content_hash: content_hash.clone(),
                    content: content.clone(),
                    acl_json,
                    acl_principals,
                    lang: item.lang.clone().unwrap_or_else(|| "zh".to_string()),
                    valid_from: now.clone(),
                    valid_to: None,
                    version: 1,
                    superseded_by: None,
                    status: "active".to_string(),
                    source_role: "external".to_string(),
                    canonicalization_version: crate::ingest::doc_id::CANONICALIZATION_VERSION.to_string(),
                    extractor_version: crate::ingest::doc_id::EXTRACTOR_VERSION.to_string(),
                    chunker_version: crate::ingest::doc_id::CHUNKER_VERSION.to_string(),
                    created_at: now.clone(),
                    updated_at: now.clone(),
                };
                match store.upsert_document(&document, &chunks, None).await {
                    Ok(_) => {
                        crate::summary::enqueue(&store, &document, &document.content_hash).await;
                        stats["indexed"] = json!(stats["indexed"].as_i64().unwrap_or(0) + 1);
                    }
                    Err(err) => {
                        reference.status = "error".to_string();
                        errors.push(json!({"external_id": item.external_id, "code": "connector_index_failed", "message": err.to_string()}));
                        stats["failed"] = json!(stats["failed"].as_i64().unwrap_or(0) + 1);
                    }
                }
            } else {
                stats["refs_only"] = json!(stats["refs_only"].as_i64().unwrap_or(0) + 1);
            }
            if let Err(err) = store.external_ref_upsert(&reference).await {
                errors.push(json!({"external_id": item.external_id, "code": "connector_ref_failed", "message": err.to_string()}));
                stats["failed"] = json!(stats["failed"].as_i64().unwrap_or(0) + 1);
            }
        }
        pages += 1;
        match next {
            Some(next) if pages < 50 && !next.is_empty() => cursor = Some(next),
            _ => break,
        }
    }
    stats["errors"] = json!(errors);
    let failed = stats["failed"].as_i64().unwrap_or(0);
    let indexed = stats["indexed"].as_i64().unwrap_or(0);
    let status = if failed > 0 && indexed == 0 && stats["refs_only"].as_i64().unwrap_or(0) == 0 {
        "failed"
    } else if failed > 0 {
        "partial"
    } else {
        "completed"
    };
    let first_error = errors
        .first()
        .and_then(|value| value.get("message"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let now = crate::util::now_rfc3339();
    let _ = store
        .connector_job_update(&job.job_id, status, Some(&now), &stats.to_string(), first_error.as_deref())
        .await;
    let _ = store
        .connector_mark_synced(&connector.tenant_id, &connector.connector_id, &now)
        .await;
    let _ = config;
    ConnectorJobRecord {
        status: status.to_string(),
        finished_at: Some(now),
        stats_json: stats.to_string(),
        error: first_error,
        ..job
    }
}
