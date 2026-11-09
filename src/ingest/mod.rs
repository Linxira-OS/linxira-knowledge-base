pub mod chunker;
pub mod doc_id;
pub mod html;

use crate::error::StoreError;
use crate::governance::Acl;
use crate::providers::{EmbeddingProvider, EventExtractor, ExtractionInput};
use crate::store::{
    ChunkRecord, DocumentRecord, EntityRecord, EventEntityRecord, EventRecord, SpaceRecord, Store,
    UpsertOutcome,
};
use crate::util::{html_escape, now_rfc3339, normalize_rfc3339, sha256_hex};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportFailure {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportReport {
    pub path: String,
    pub space_id: String,
    pub imported: usize,
    pub skipped: usize,
    pub skipped_details: Vec<ImportFailure>,
    pub failed: Vec<ImportFailure>,
    pub warnings: Vec<String>,
}

impl ImportReport {
    pub fn human(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("import path: {}\n", self.path));
        out.push_str(&format!("space: {}\n", self.space_id));
        out.push_str(&format!("imported: {}\n", self.imported));
        out.push_str(&format!("skipped: {}\n", self.skipped));
        out.push_str(&format!("failed: {}\n", self.failed.len()));
        for item in &self.skipped_details {
            out.push_str(&format!("  skip {}: {}\n", item.path, item.reason));
        }
        for item in &self.failed {
            out.push_str(&format!("  fail {}: {}\n", item.path, item.reason));
        }
        for warning in &self.warnings {
            out.push_str(&format!("  warning: {warning}\n"));
        }
        out
    }
}

#[derive(Debug, Clone)]
pub struct ImportOptions {
    pub tenant_id: String,
    pub space_id: String,
    pub acl: Option<Acl>,
    pub max_chunk_chars: usize,
    /// True only for the solo profile; team/enterprise require explicit ACL
    /// when a space does not exist yet.
    pub allow_implicit_allow_all: bool,
    pub owner_principal_id: String,
    pub embed_batch_size: usize,
    /// Fallback language when a source has no frontmatter `lang`.
    pub lang: Option<String>,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            tenant_id: crate::governance::SOLO_TENANT.to_string(),
            space_id: crate::governance::SOLO_SPACE.to_string(),
            acl: None,
            max_chunk_chars: chunker::DEFAULT_CHUNK_CHARS,
            allow_implicit_allow_all: true,
            owner_principal_id: "local".to_string(),
            embed_batch_size: 32,
            lang: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourceItem {
    pub relative_path: String,
    pub title: Option<String>,
    pub lang: Option<String>,
    /// Source content stored in the document registry and exported.
    pub content: String,
    /// Text used for chunking (HTML is reduced to text).
    pub text: String,
    pub mime: String,
    pub source_uri: String,
    pub content_hash: Option<String>,
    pub raw_hash: Option<String>,
    pub acl: Option<Acl>,
    pub space_id: Option<String>,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub status: Option<String>,
    pub superseded_by: Option<String>,
    pub version: Option<u32>,
    pub expected_doc_id: Option<String>,
}

#[derive(Debug, Default)]
pub struct ScanResult {
    pub items: Vec<SourceItem>,
    pub skipped: Vec<ImportFailure>,
    pub failed: Vec<ImportFailure>,
}

#[async_trait]
pub trait Source {
    fn name(&self) -> &'static str;
    async fn scan(&self) -> Result<ScanResult, String>;
}

pub struct FilesystemSource {
    root: PathBuf,
}

impl FilesystemSource {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl Source for FilesystemSource {
    fn name(&self) -> &'static str {
        "filesystem"
    }

    async fn scan(&self) -> Result<ScanResult, String> {
        let mut result = ScanResult::default();
        let root = self.root.clone();
        walk(&root, &root, &mut result)?;
        result
            .items
            .sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        Ok(result)
    }
}

pub struct JsonlSource {
    path: PathBuf,
}

impl JsonlSource {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

#[async_trait]
impl Source for JsonlSource {
    fn name(&self) -> &'static str {
        "jsonl"
    }

    async fn scan(&self) -> Result<ScanResult, String> {
        let text = std::fs::read_to_string(&self.path)
            .map_err(|err| format!("cannot read {}: {err}", self.path.display()))?;
        let mut result = ScanResult::default();
        for (index, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            match parse_jsonl_item(line) {
                Ok(item) => result.items.push(item),
                Err(reason) => result.failed.push(ImportFailure {
                    path: format!("{}:{}", self.path.display(), index + 1),
                    reason,
                }),
            }
        }
        Ok(result)
    }
}

fn walk(root: &Path, current: &Path, result: &mut ScanResult) -> Result<(), String> {
    let mut entries = std::fs::read_dir(current)
        .map_err(|err| format!("cannot read directory {}: {err}", current.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("cannot read directory {}: {err}", current.display()))?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|err| format!("cannot stat {}: {err}", path.display()))?;
        if file_type.is_symlink() {
            result.skipped.push(ImportFailure {
                path: relative_display(root, &path),
                reason: "symlink skipped".to_string(),
            });
            continue;
        }
        if file_type.is_dir() {
            if skip_directory(&name) {
                continue;
            }
            walk(root, &path, result)?;
            continue;
        }
        if name.starts_with('.') {
            result.skipped.push(ImportFailure {
                path: relative_display(root, &path),
                reason: "hidden file skipped".to_string(),
            });
            continue;
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match extension.as_str() {
            "md" | "markdown" | "txt" | "html" | "htm" => {
                match read_file_item(root, &path, &extension) {
                    Ok(item) => result.items.push(item),
                    Err(reason) => result.failed.push(ImportFailure {
                        path: relative_display(root, &path),
                        reason,
                    }),
                }
            }
            "pdf" => {
                #[cfg(feature = "pdf")]
                match read_pdf_item(root, &path) {
                    Ok(item) => result.items.push(item),
                    Err(reason) => result.failed.push(ImportFailure {
                        path: relative_display(root, &path),
                        reason,
                    }),
                }
                #[cfg(not(feature = "pdf"))]
                result.skipped.push(ImportFailure {
                    path: relative_display(root, &path),
                    reason: "PDF support is not built (enable the `pdf` feature)".to_string(),
                });
            }
            _ => result.skipped.push(ImportFailure {
                path: relative_display(root, &path),
                reason: format!("unsupported extension {extension:?}"),
            }),
        }
    }
    Ok(())
}

fn skip_directory(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name,
            "target" | "node_modules" | ".lkb" | "data" | "blobs"
        )
}

fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn read_file_item(root: &Path, path: &Path, extension: &str) -> Result<SourceItem, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|err| format!("cannot stat {}: {err}", path.display()))?;
    if metadata.len() > 32 * 1024 * 1024 {
        return Err("file is larger than 32 MiB".to_string());
    }
    let bytes = std::fs::read(path)
        .map_err(|err| format!("cannot read {}: {err}", path.display()))?;
    let raw_hash = sha256_hex(&bytes);
    let content = String::from_utf8(bytes)
        .map_err(|_| "file is not valid UTF-8".to_string())?;
    let relative_path = relative_display(root, path);
    let mime = mime_for(extension);
    let (content, text, title, lang) = if extension == "md" || extension == "markdown" {
        let parsed = crate::frontmatter::parse(&content);
        let body = parsed.body;
        let title = parsed
            .title
            .or_else(|| first_heading(&body))
            .or_else(|| stem(path));
        (body.clone(), body, title, parsed.lang)
    } else if extension == "html" || extension == "htm" {
        (
            content.clone(),
            html::extract_text(&content),
            html::extract_title(&content)
                .or_else(|| first_heading(&content))
                .or_else(|| stem(path)),
            None,
        )
    } else {
        (
            content.clone(),
            content.clone(),
            first_heading(&content).or_else(|| stem(path)),
            None,
        )
    };
    Ok(SourceItem {
        relative_path: relative_path.clone(),
        title,
        lang,
        content,
        text,
        mime,
        source_uri: relative_path,
        content_hash: Some(raw_hash.clone()),
        raw_hash: Some(raw_hash),
        acl: None,
        space_id: None,
        valid_from: None,
        valid_to: None,
        status: None,
        superseded_by: None,
        version: None,
        expected_doc_id: None,
    })
}

#[cfg(feature = "pdf")]
fn read_pdf_item(root: &Path, path: &Path) -> Result<SourceItem, String> {
    let bytes = std::fs::read(path)
        .map_err(|err| format!("cannot read {}: {err}", path.display()))?;
    let raw_hash = sha256_hex(&bytes);
    let text = pdf_extract::extract_text(path)
        .map_err(|err| format!("PDF extraction failed: {err}"))?;
    let relative_path = relative_display(root, path);
    Ok(SourceItem {
        relative_path: relative_path.clone(),
        title: stem(path),
        lang: None,
        content: text.clone(),
        text,
        mime: "application/pdf".to_string(),
        source_uri: relative_path,
        content_hash: Some(raw_hash.clone()),
        raw_hash: Some(raw_hash),
        acl: None,
        space_id: None,
        valid_from: None,
        valid_to: None,
        status: None,
        superseded_by: None,
        version: None,
        expected_doc_id: None,
    })
}

fn mime_for(extension: &str) -> String {
    match extension {
        "md" | "markdown" => "text/markdown",
        "html" | "htm" => "text/html",
        "txt" => "text/plain",
        "jsonl" => "application/jsonl",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn stem(path: &Path) -> Option<String> {
    path.file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .filter(|value| !value.trim().is_empty())
}

fn first_heading(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("# ") {
            let value = rest.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
        None
    })
}

fn parse_jsonl_item(line: &str) -> Result<SourceItem, String> {
    let value: serde_json::Value =
        serde_json::from_str(line).map_err(|err| format!("invalid JSON: {err}"))?;
    let path = value
        .get("path")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "missing string field `path`".to_string())?;
    let content = value
        .get("content")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "missing string field `content`".to_string())?
        .to_string();
    let metadata = value.get("metadata");
    let extension = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mime = value
        .get("mime")
        .and_then(serde_json::Value::as_str)
        .map(|value| value.to_string())
        .unwrap_or_else(|| mime_for(&extension));
    let (text, extracted_title) = if mime.contains("html") {
        (
            html::extract_text(&content),
            html::extract_title(&content),
        )
    } else {
        (content.clone(), None)
    };
    let title = value
        .get("title")
        .and_then(serde_json::Value::as_str)
        .map(|value| value.to_string())
        .or(extracted_title)
        .or_else(|| first_heading(&content))
        .or_else(|| Path::new(path).file_stem().map(|s| s.to_string_lossy().to_string()));
    let acl = value
        .get("acl")
        .or_else(|| metadata.and_then(|m| m.get("acl")))
        .map(parse_acl_value)
        .transpose()?;
    let lang = value
        .get("lang")
        .or_else(|| metadata.and_then(|m| m.get("lang")))
        .and_then(serde_json::Value::as_str)
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    let space_id = value
        .get("space")
        .or_else(|| value.get("space_id"))
        .or_else(|| metadata.and_then(|m| m.get("space")))
        .or_else(|| metadata.and_then(|m| m.get("space_id")))
        .and_then(serde_json::Value::as_str)
        .map(|value| value.to_string());
    let valid_from = value
        .get("valid_from")
        .or_else(|| metadata.and_then(|m| m.get("valid_from")))
        .and_then(serde_json::Value::as_str)
        .map(|value| normalize_rfc3339(value))
        .transpose()?;
    let valid_to = value
        .get("valid_to")
        .or_else(|| metadata.and_then(|m| m.get("valid_to")))
        .and_then(serde_json::Value::as_str)
        .map(|value| normalize_rfc3339(value))
        .transpose()?;
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .map(|value| value as u32);
    Ok(SourceItem {
        relative_path: path.to_string(),
        title,
        lang,
        content,
        text,
        mime,
        source_uri: value
            .get("source_uri")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(path)
            .to_string(),
        content_hash: value
            .get("content_hash")
            .and_then(serde_json::Value::as_str)
            .map(|value| value.to_string()),
        raw_hash: value
            .get("raw_hash")
            .and_then(serde_json::Value::as_str)
            .map(|value| value.to_string()),
        acl,
        space_id,
        valid_from,
        valid_to,
        status: value
            .get("status")
            .and_then(serde_json::Value::as_str)
            .map(|value| value.to_string()),
        superseded_by: value
            .get("superseded_by")
            .and_then(serde_json::Value::as_str)
            .map(|value| value.to_string()),
        version,
        expected_doc_id: value
            .get("doc_id")
            .and_then(serde_json::Value::as_str)
            .map(|value| value.to_string()),
    })
}

fn parse_acl_value(value: &serde_json::Value) -> Result<Acl, String> {
    match value {
        serde_json::Value::String(text) => Acl::from_user_input(text),
        other => serde_json::from_value(other.clone()).map_err(|err| format!("invalid ACL: {err}")),
    }
}

async fn resolve_space_acl(
    store: &Arc<dyn Store>,
    tenant_id: &str,
    space_id: &str,
    requested: Option<&Acl>,
    allow_implicit_allow_all: bool,
    owner: &str,
) -> Result<Acl, StoreError> {
    if let Some(space) = store.get_space(tenant_id, space_id).await? {
        return match requested {
            Some(acl) => Ok(acl.clone()),
            None => space.default_acl(),
        };
    }
    let acl = match requested {
        Some(acl) => acl.clone(),
        None if allow_implicit_allow_all => Acl::allow_all(),
        None => {
            return Err(StoreError::Invalid(format!(
                "space {space_id:?} does not exist; provide --acl-json for team/enterprise import"
            )))
        }
    };
    let record = SpaceRecord::new(
        tenant_id,
        space_id,
        space_id,
        &acl,
        owner,
        &now_rfc3339(),
    );
    store.ensure_space(&record).await?;
    Ok(acl)
}

pub async fn import_path(
    store: &Arc<dyn Store>,
    path: &Path,
    options: &ImportOptions,
    embedding: Option<&dyn EmbeddingProvider>,
    extractor: Option<&dyn EventExtractor>,
) -> Result<ImportReport, StoreError> {
    let source: Box<dyn Source> = if path.is_dir() {
        Box::new(FilesystemSource::new(path.to_path_buf()))
    } else {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if extension == "jsonl" {
            Box::new(JsonlSource::new(path.to_path_buf()))
        } else {
            return Err(StoreError::Invalid(format!(
                "import path must be a directory or a .jsonl file: {}",
                path.display()
            )));
        }
    };

    let scan = source
        .scan()
        .await
        .map_err(StoreError::Invalid)?;
    let initial_skipped = scan.skipped.len();
    let mut report = ImportReport {
        path: path.display().to_string(),
        space_id: options.space_id.clone(),
        imported: 0,
        skipped: initial_skipped,
        skipped_details: scan.skipped,
        failed: scan.failed,
        warnings: Vec::new(),
    };

    for item in scan.items {
        let item_space = item
            .space_id
            .clone()
            .unwrap_or_else(|| options.space_id.clone());
        if item_space.trim().is_empty()
            || item_space.contains('/')
            || item_space.contains('\\')
            || item_space == ".."
            || item_space == "."
        {
            report.failed.push(ImportFailure {
                path: item.relative_path.clone(),
                reason: format!("invalid space_id {item_space:?}"),
            });
            continue;
        }
        let acl = match resolve_space_acl(
            store,
            &options.tenant_id,
            &item_space,
            item.acl.as_ref().or(options.acl.as_ref()),
            options.allow_implicit_allow_all,
            &options.owner_principal_id,
        )
        .await
        {
            Ok(acl) => acl,
            Err(err) => {
                report.failed.push(ImportFailure {
                    path: item.relative_path.clone(),
                    reason: err.to_string(),
                });
                continue;
            }
        };

        let canonical = match doc_id::canonical_relative_path(&item.relative_path) {
            Ok(value) => value,
            Err(reason) => {
                report.failed.push(ImportFailure {
                    path: item.relative_path.clone(),
                    reason,
                });
                continue;
            }
        };
        let lang = item
            .lang
            .clone()
            .or_else(|| options.lang.clone())
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "zh".to_string());
        let content_hash = item
            .content_hash
            .clone()
            .unwrap_or_else(|| sha256_hex(item.content.as_bytes()));
        let raw_hash = item.raw_hash.clone().unwrap_or_else(|| content_hash.clone());
        let doc_id = doc_id::derive_doc_id(&canonical, &content_hash);
        let now = now_rfc3339();
        let valid_from = item.valid_from.clone().unwrap_or_else(|| now.clone());
        if let Some(valid_to) = &item.valid_to {
            if valid_to.as_str() <= valid_from.as_str() {
                report.failed.push(ImportFailure {
                    path: item.relative_path.clone(),
                    reason: format!(
                        "valid_to {valid_to} must be later than valid_from {valid_from}"
                    ),
                });
                continue;
            }
        }
        let status = item
            .status
            .clone()
            .unwrap_or_else(|| "active".to_string());
        if !matches!(status.as_str(), "active" | "superseded" | "orphaned" | "deleted") {
            report.failed.push(ImportFailure {
                path: item.relative_path.clone(),
                reason: format!("invalid status {status:?}"),
            });
            continue;
        }
        let version = item.version.unwrap_or(1).max(1);
        let title = item
            .title
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| canonical.clone());
        let acl_json = acl.to_json();
        let acl_principals = acl.principals_key();

        let drafts = chunker::chunk_text(&item.text, options.max_chunk_chars);
        if drafts.is_empty() {
            report.warnings.push(format!(
                "{} produced no chunks (empty text)",
                item.relative_path
            ));
        }
        let mut chunks: Vec<ChunkRecord> = drafts
            .iter()
            .enumerate()
            .map(|(index, draft)| {
                let chunk_content_hash = sha256_hex(draft.text.as_bytes());
                ChunkRecord {
                    tenant_id: options.tenant_id.clone(),
                    chunk_id: doc_id::derive_chunk_id(
                        &options.tenant_id,
                        &doc_id,
                        index as u32,
                        &chunk_content_hash,
                    ),
                    doc_id: doc_id.clone(),
                    space_id: item_space.clone(),
                    seq: index as u32,
                    text: draft.text.clone(),
                    line_start: draft.line_start,
                    line_end: draft.line_end,
                    content_hash: chunk_content_hash,
                    acl_json: acl_json.clone(),
                    acl_principals: acl_principals.clone(),
                    lang: lang.clone(),
                    valid_from: valid_from.clone(),
                    valid_to: item.valid_to.clone(),
                    superseded_by: item.superseded_by.clone(),
                    version,
                    embedding: None,
                }
            })
            .collect();

        if let Some(provider) = embedding {
            attach_embeddings(&mut chunks, provider, options.embed_batch_size, &mut report.warnings).await;
        }

        let quota_delta = crate::quota::QuotaDelta {
            source_bytes: item.content.as_bytes().len() as i64,
            index_bytes: chunks.iter().map(|chunk| chunk.text.len() as i64).sum(),
            history_bytes: 0,
            documents: 1,
            chunks: chunks.len() as i64,
        };
        let space_record = match store.get_space(&options.tenant_id, &item_space).await? {
            Some(space) => space,
            None => {
                report.failed.push(ImportFailure {
                    path: item.relative_path.clone(),
                    reason: format!("space {item_space:?} does not exist"),
                });
                continue;
            }
        };
        if space_record.status != "active" {
            report.failed.push(ImportFailure {
                path: item.relative_path.clone(),
                reason: format!("space_not_active: space is {}", space_record.status),
            });
            continue;
        }
        let _quota_guard = crate::quota::write_guard().await;
        if let Err(err) = crate::quota::reserve(store, &space_record, &quota_delta).await {
            report.failed.push(ImportFailure {
                path: item.relative_path.clone(),
                reason: format!("{}: {}", err.code, err.message),
            });
            continue;
        }

        let document = DocumentRecord {
            tenant_id: options.tenant_id.clone(),
            doc_id: doc_id.clone(),
            space_id: item_space,
            logical_path: canonical,
            source_uri: item.source_uri,
            title,
            mime: item.mime,
            raw_hash,
            content_hash,
            content: item.content,
            acl_json,
            acl_principals,
            lang,
            valid_from,
            valid_to: item.valid_to,
            version,
            superseded_by: item.superseded_by,
            status,
            source_role: "standalone".to_string(),
            canonicalization_version: doc_id::CANONICALIZATION_VERSION.to_string(),
            extractor_version: doc_id::EXTRACTOR_VERSION.to_string(),
            chunker_version: doc_id::CHUNKER_VERSION.to_string(),
            created_at: now.clone(),
            updated_at: now,
        };

        let outcome = store
            .upsert_document(&document, &chunks, item.expected_doc_id.as_deref())
            .await;
        if outcome.is_ok() {
            crate::summary::enqueue(store, &document, &document.content_hash).await;
            if let Some(extractor) = extractor {
                extract_and_store_events(store, extractor, &document, &chunks, &mut report.warnings)
                    .await;
            }
        }
        crate::quota::release(store, &space_record, &quota_delta).await;
        let _ = crate::quota::refresh(store, &options.tenant_id, &space_record.space_id).await;
        match outcome {
            Ok(UpsertOutcome::Imported) => report.imported += 1,
            Ok(UpsertOutcome::Skipped(reason)) => {
                report.skipped += 1;
                report.skipped_details.push(ImportFailure {
                    path: item.relative_path,
                    reason,
                });
            }
            Err(err) => report.failed.push(ImportFailure {
                path: item.relative_path,
                reason: err.to_string(),
            }),
        }
    }

    Ok(report)
}

pub(crate) async fn extract_and_store_events(
    store: &Arc<dyn Store>,
    extractor: &dyn EventExtractor,
    document: &DocumentRecord,
    chunks: &[ChunkRecord],
    warnings: &mut Vec<String>,
) {
    let version = extractor.version();
    let model = extractor.model_id();
    for chunk in chunks {
        match store
            .has_current_event(&chunk.tenant_id, &chunk.space_id, &chunk.chunk_id, &version, &model)
            .await
        {
            Ok(true) => continue,
            Ok(false) => {}
            Err(err) => {
                warnings.push(format!(
                    "event extraction state check failed for {}: {err}",
                    chunk.chunk_id
                ));
                continue;
            }
        }
        let input = ExtractionInput {
            chunk_id: &chunk.chunk_id,
            doc_id: &chunk.doc_id,
            text: &chunk.text,
        };
        match extractor.extract(&input).await {
            Ok(extracted) => {
                let event_text = extracted.event.trim().to_string();
                if event_text.is_empty() {
                    warnings.push(format!(
                        "event extraction returned an empty event for {}",
                        chunk.chunk_id
                    ));
                    continue;
                }
                let now = now_rfc3339();
                let event_id = crate::event::derive_event_id(
                    &chunk.tenant_id,
                    &chunk.space_id,
                    &chunk.chunk_id,
                    0,
                    &version,
                    &model,
                );
                let mut entities = Vec::new();
                let mut links = Vec::new();
                let mut seen = std::collections::HashSet::new();
                for entity in extracted.entities.into_iter().take(32) {
                    let name = entity.name.trim().to_string();
                    let normalized = crate::event::normalize_entity_name(&name);
                    if normalized.is_empty() {
                        continue;
                    }
                    let entity_type = crate::event::normalize_entity_type(&entity.entity_type);
                    if !seen.insert((normalized.clone(), entity_type.clone())) {
                        continue;
                    }
                    let entity_id = crate::event::derive_entity_id(
                        &chunk.tenant_id,
                        &chunk.space_id,
                        &normalized,
                        &entity_type,
                    );
                    entities.push(EntityRecord {
                        entity_id: entity_id.clone(),
                        tenant_id: chunk.tenant_id.clone(),
                        space_id: chunk.space_id.clone(),
                        name,
                        normalized_name: normalized,
                        entity_type,
                        created_at: now.clone(),
                    });
                    links.push(EventEntityRecord {
                        event_id: event_id.clone(),
                        entity_id,
                        weight: 1.0,
                        tenant_id: chunk.tenant_id.clone(),
                        space_id: chunk.space_id.clone(),
                    });
                }
                if entities.is_empty() {
                    warnings.push(format!(
                        "event extraction produced no entities for {}; event skipped",
                        chunk.chunk_id
                    ));
                    continue;
                }
                let event = EventRecord {
                    event_id,
                    tenant_id: chunk.tenant_id.clone(),
                    space_id: chunk.space_id.clone(),
                    doc_id: chunk.doc_id.clone(),
                    chunk_id: chunk.chunk_id.clone(),
                    seq: 0,
                    text: event_text.clone(),
                    content_hash: sha256_hex(event_text.as_bytes()),
                    extractor_version: version.clone(),
                    extraction_model: model.clone(),
                    acl_json: chunk.acl_json.clone(),
                    acl_principals: chunk.acl_principals.clone(),
                    lang: chunk.lang.clone(),
                    valid_from: chunk.valid_from.clone(),
                    valid_to: chunk.valid_to.clone(),
                    version: 1,
                    superseded_by: None,
                    created_at: now,
                };
                if let Err(err) = store.upsert_event(&event, &entities, &links).await {
                    warnings.push(format!(
                        "event persistence failed for {} ({}): {err}",
                        document.logical_path, chunk.chunk_id
                    ));
                }
            }
            Err(err) => warnings.push(format!(
                "event extraction failed for {} ({}): {err}",
                document.logical_path, chunk.chunk_id
            )),
        }
    }
}

pub(crate) async fn attach_embeddings(
    chunks: &mut [ChunkRecord],
    provider: &dyn EmbeddingProvider,
    batch_size: usize,
    warnings: &mut Vec<String>,
) {
    let batch_size = batch_size.max(1);
    for start in (0..chunks.len()).step_by(batch_size) {
        let end = (start + batch_size).min(chunks.len());
        let texts: Vec<String> = chunks[start..end]
            .iter()
            .map(|chunk| chunk.text.clone())
            .collect();
        match provider.embed(&texts).await {
            Ok(vectors) if vectors.len() == texts.len() => {
                for (chunk, vector) in chunks[start..end].iter_mut().zip(vectors) {
                    chunk.embedding = Some(vector);
                }
            }
            Ok(vectors) => {
                warnings.push(format!(
                    "embedding provider returned {} vectors for {} chunks; embeddings omitted",
                    vectors.len(),
                    texts.len()
                ));
                break;
            }
            Err(err) => {
                warnings.push(format!("embedding failed: {err}; keyword index remains usable"));
                break;
            }
        }
    }
}

pub async fn export_jsonl(
    store: &Arc<dyn Store>,
    tenant_id: &str,
    space_id: Option<&str>,
    writer: &mut dyn Write,
) -> anyhow::Result<usize> {
    let docs = store.list_documents(tenant_id, space_id).await?;
    let mut count = 0usize;
    for doc in docs {
        let acl: serde_json::Value =
            serde_json::from_str(&doc.acl_json).unwrap_or(serde_json::Value::Null);
        let record = serde_json::json!({
            "doc_id": doc.doc_id,
            "path": doc.logical_path,
            "title": doc.title,
            "content": doc.content,
            "space": doc.space_id,
            "lang": doc.lang,
            "mime": doc.mime,
            "content_hash": doc.content_hash,
            "raw_hash": doc.raw_hash,
            "acl": acl,
            "valid_from": doc.valid_from,
            "valid_to": doc.valid_to,
            "version": doc.version,
            "superseded_by": doc.superseded_by,
            "status": doc.status,
            "source_uri": doc.source_uri,
            "source_role": doc.source_role,
        });
        writeln!(writer, "{}", serde_json::to_string(&record)?)?;
        count += 1;
    }
    writer.flush()?;
    Ok(count)
}

pub fn report_json(report: &ImportReport) -> String {
    serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".to_string())
}

pub fn failure_html(path: &str, reason: &str) -> String {
    format!(
        "<p>{}: {}</p>",
        html_escape(path),
        html_escape(reason)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_report_renders_counts_and_reasons() {
        let report = ImportReport {
            path: "docs".into(),
            space_id: "s_default".into(),
            imported: 2,
            skipped: 1,
            skipped_details: vec![ImportFailure {
                path: "old.md".into(),
                reason: "doc_id already indexed".into(),
            }],
            failed: vec![ImportFailure {
                path: "bad.pdf".into(),
                reason: "PDF support is not built".into(),
            }],
            warnings: vec!["embedding failed".into()],
        };
        let human = report.human();
        assert!(human.contains("imported: 2"));
        assert!(human.contains("skipped: 1"));
        assert!(human.contains("old.md"));
        assert!(human.contains("bad.pdf"));
        assert!(serde_json::from_str::<serde_json::Value>(&report_json(&report)).is_ok());
    }
}
