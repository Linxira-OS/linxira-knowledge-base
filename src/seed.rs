//! Built-in product documentation seed (`docs/`).
//!
//! The 24 Markdown files are embedded at compile time. Seeding writes normal
//! Page/Revision rows and then uses the same Document/Chunk path as the Wiki.

use crate::config::{Config, ProfileName};
use crate::error::StoreError;
use crate::governance::Acl;
use crate::ingest::{chunker, doc_id};
use crate::store::{
    ChunkRecord, DocumentRecord, PageRecord, RevisionRecord, SeedMeta, SpaceRecord, Store,
};
use crate::util::{now_rfc3339, sha256_hex};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

pub const SEED_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "-seed-v1");
pub const SEED_AUTHOR: &str = "system:seed";

struct SeedFile {
    path: &'static str,
    content: &'static str,
}

const SEED_FILES: &[SeedFile] = &[
    SeedFile {
        path: "docs/zh/00-index.md",
        content: include_str!("../docs/zh/00-index.md"),
    },
    SeedFile {
        path: "docs/zh/01-overview.md",
        content: include_str!("../docs/zh/01-overview.md"),
    },
    SeedFile {
        path: "docs/zh/02-concepts.md",
        content: include_str!("../docs/zh/02-concepts.md"),
    },
    SeedFile {
        path: "docs/zh/03-quickstart.md",
        content: include_str!("../docs/zh/03-quickstart.md"),
    },
    SeedFile {
        path: "docs/zh/04-wiki.md",
        content: include_str!("../docs/zh/04-wiki.md"),
    },
    SeedFile {
        path: "docs/zh/05-search-ai.md",
        content: include_str!("../docs/zh/05-search-ai.md"),
    },
    SeedFile {
        path: "docs/zh/06-auth-security.md",
        content: include_str!("../docs/zh/06-auth-security.md"),
    },
    SeedFile {
        path: "docs/zh/07-configuration.md",
        content: include_str!("../docs/zh/07-configuration.md"),
    },
    SeedFile {
        path: "docs/zh/08-deployment.md",
        content: include_str!("../docs/zh/08-deployment.md"),
    },
    SeedFile {
        path: "docs/zh/09-connectors.md",
        content: include_str!("../docs/zh/09-connectors.md"),
    },
    SeedFile {
        path: "docs/zh/10-architecture.md",
        content: include_str!("../docs/zh/10-architecture.md"),
    },
    SeedFile {
        path: "docs/zh/11-faq.md",
        content: include_str!("../docs/zh/11-faq.md"),
    },
    SeedFile {
        path: "docs/en/00-index.md",
        content: include_str!("../docs/en/00-index.md"),
    },
    SeedFile {
        path: "docs/en/01-overview.md",
        content: include_str!("../docs/en/01-overview.md"),
    },
    SeedFile {
        path: "docs/en/02-concepts.md",
        content: include_str!("../docs/en/02-concepts.md"),
    },
    SeedFile {
        path: "docs/en/03-quickstart.md",
        content: include_str!("../docs/en/03-quickstart.md"),
    },
    SeedFile {
        path: "docs/en/04-wiki.md",
        content: include_str!("../docs/en/04-wiki.md"),
    },
    SeedFile {
        path: "docs/en/05-search-ai.md",
        content: include_str!("../docs/en/05-search-ai.md"),
    },
    SeedFile {
        path: "docs/en/06-auth-security.md",
        content: include_str!("../docs/en/06-auth-security.md"),
    },
    SeedFile {
        path: "docs/en/07-configuration.md",
        content: include_str!("../docs/en/07-configuration.md"),
    },
    SeedFile {
        path: "docs/en/08-deployment.md",
        content: include_str!("../docs/en/08-deployment.md"),
    },
    SeedFile {
        path: "docs/en/09-connectors.md",
        content: include_str!("../docs/en/09-connectors.md"),
    },
    SeedFile {
        path: "docs/en/10-architecture.md",
        content: include_str!("../docs/en/10-architecture.md"),
    },
    SeedFile {
        path: "docs/en/11-faq.md",
        content: include_str!("../docs/en/11-faq.md"),
    },
];

#[derive(Debug, Clone)]
pub struct SeedOptions {
    pub space_id: String,
    /// `zh`, `en`, or `all`.
    pub lang: Option<String>,
}

impl Default for SeedOptions {
    fn default() -> Self {
        Self {
            space_id: "product".to_string(),
            lang: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedReport {
    pub space_id: String,
    pub seed_version: String,
    pub source_manifest_hash: String,
    pub imported: usize,
    pub skipped: usize,
    pub failed: usize,
    pub unchanged: usize,
    pub user_modified: usize,
    pub by_lang: BTreeMap<String, usize>,
    pub warnings: Vec<String>,
}

impl SeedReport {
    pub fn human(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("space: {}\n", self.space_id));
        out.push_str(&format!("seed_version: {}\n", self.seed_version));
        out.push_str(&format!("manifest: {}\n", self.source_manifest_hash));
        out.push_str(&format!("imported: {}\n", self.imported));
        out.push_str(&format!("skipped: {}\n", self.skipped));
        out.push_str(&format!("failed: {}\n", self.failed));
        out.push_str(&format!("unchanged: {}\n", self.unchanged));
        out.push_str(&format!("user_modified: {}\n", self.user_modified));
        for (lang, count) in &self.by_lang {
            out.push_str(&format!("imported[{lang}]: {count}\n"));
        }
        for warning in &self.warnings {
            out.push_str(&format!("warning: {warning}\n"));
        }
        out
    }
}

#[derive(Debug, Clone)]
struct SeedPage {
    lang: String,
    slug: String,
    title: String,
    summary: String,
    order: i32,
    body: String,
    source_path: String,
    content_hash: String,
}

pub async fn run_seed(
    store: &Arc<dyn Store>,
    config: &Config,
    options: &SeedOptions,
) -> Result<SeedReport, StoreError> {
    let tenant_id = config.storage.tenant_id.clone();
    let space_id = options.space_id.clone();
    let lang_filter = normalize_lang_filter(options.lang.as_deref());
    let manifest_hash = manifest_hash();
    let mut report = SeedReport {
        space_id: space_id.clone(),
        seed_version: SEED_VERSION.to_string(),
        source_manifest_hash: manifest_hash.clone(),
        imported: 0,
        skipped: 0,
        failed: 0,
        unchanged: 0,
        user_modified: 0,
        by_lang: BTreeMap::new(),
        warnings: Vec::new(),
    };

    let pages = match parse_seed_pages(lang_filter.as_deref()) {
        Ok(pages) => pages,
        Err(errors) => {
            for error in errors {
                report.failed += 1;
                report.warnings.push(error);
            }
            report
                .warnings
                .push("seed_import_failed: invalid embedded documentation".to_string());
            return Ok(report);
        }
    };

    let space = match store.get_space(&tenant_id, &space_id).await? {
        Some(space) => space,
        None if config.profile_name() == ProfileName::Solo => {
            let space = SpaceRecord::new(
                &tenant_id,
                &space_id,
                "Product documentation",
                &Acl::allow_all(),
                SEED_AUTHOR,
                &now_rfc3339(),
            );
            store.ensure_space(&space).await?;
            space
        }
        None => {
            report.failed += 1;
            report.warnings.push(format!(
                "seed_import_failed: space {space_id:?} does not exist"
            ));
            return Ok(report);
        }
    };
    let default_acl = match space.default_acl() {
        Ok(acl) => acl,
        Err(err) => {
            report.failed += 1;
            report
                .warnings
                .push(format!("seed_import_failed: invalid space ACL: {err}"));
            return Ok(report);
        }
    };

    for page in pages {
        match seed_one_page(store, &tenant_id, &space_id, &default_acl, &page).await {
            Ok(SeedPageOutcome::Imported) => {
                report.imported += 1;
                *report.by_lang.entry(page.lang.clone()).or_insert(0) += 1;
            }
            Ok(SeedPageOutcome::Unchanged) => {
                report.skipped += 1;
                report.unchanged += 1;
            }
            Ok(SeedPageOutcome::UserModified) => {
                report.skipped += 1;
                report.user_modified += 1;
                report.warnings.push(format!(
                    "seed skipped user-modified page {}/{}",
                    page.source_path, page.slug
                ));
            }
            Err(err) => {
                report.failed += 1;
                report
                    .warnings
                    .push(format!("seed page {} failed: {err}", page.source_path));
            }
        }
    }

    if report.failed == 0 {
        let prior_revisions = store
            .get_seed_meta(&tenant_id, &space_id)
            .await
            .ok()
            .flatten()
            .map(|meta| meta.revision_count)
            .unwrap_or(0);
        let meta = SeedMeta {
            tenant_id,
            space_id,
            seed_version: SEED_VERSION.to_string(),
            source_manifest_hash: manifest_hash,
            applied_at: now_rfc3339(),
            page_count: report.imported as u32 + report.skipped as u32,
            revision_count: prior_revisions + report.imported as u32,
        };
        if let Err(err) = store.upsert_seed_meta(&meta).await {
            report
                .warnings
                .push(format!("seed_import_failed: cannot write seed_meta: {err}"));
        }
    } else {
        report
            .warnings
            .push("seed_import_failed: one or more pages failed".to_string());
    }
    Ok(report)
}

enum SeedPageOutcome {
    Imported,
    Unchanged,
    UserModified,
}

async fn seed_one_page(
    store: &Arc<dyn Store>,
    tenant_id: &str,
    space_id: &str,
    default_acl: &Acl,
    page: &SeedPage,
) -> Result<SeedPageOutcome, StoreError> {
    let existing = store
        .get_page_unfiltered(tenant_id, space_id, &page.slug, Some(&page.lang))
        .await?;
    let now = now_rfc3339();
    let (version, acl, created_at) = match &existing {
        None => (1, default_acl.clone(), now.clone()),
        Some(view) => {
            let revisions = store
                .list_revisions(tenant_id, space_id, &page.slug, Some(&page.lang))
                .await?;
            if revisions
                .iter()
                .any(|revision| revision.author != SEED_AUTHOR)
            {
                return Ok(SeedPageOutcome::UserModified);
            }
            if view.page.content_hash == page.content_hash
                && view.page.title == page.title
                && view.page.summary == page.summary
                && view.page.order_index == page.order
            {
                return Ok(SeedPageOutcome::Unchanged);
            }
            let acl = Acl::from_json(&view.page.acl_json).unwrap_or_else(|_| default_acl.clone());
            (view.page.version + 1, acl, view.page.created_at.clone())
        }
    };

    let revision_id = doc_id::derive_revision_id(
        tenant_id,
        space_id,
        &page.slug,
        &page.lang,
        &page.content_hash,
        version,
    );
    let page_record = PageRecord {
        tenant_id: tenant_id.to_string(),
        space_id: space_id.to_string(),
        slug: page.slug.clone(),
        lang: page.lang.clone(),
        title: page.title.clone(),
        summary: page.summary.clone(),
        order_index: page.order,
        current_revision_id: revision_id.clone(),
        content_hash: page.content_hash.clone(),
        acl_json: acl.to_json(),
        acl_principals: acl.principals_key(),
        valid_from: now.clone(),
        valid_to: None,
        version,
        superseded_by: None,
        created_at: created_at.clone(),
        updated_at: now.clone(),
    };
    let revision = RevisionRecord {
        tenant_id: tenant_id.to_string(),
        revision_id: revision_id.clone(),
        space_id: space_id.to_string(),
        slug: page.slug.clone(),
        lang: page.lang.clone(),
        title: page.title.clone(),
        markdown: page.body.clone(),
        content_hash: page.content_hash.clone(),
        author: SEED_AUTHOR.to_string(),
        created_at: now.clone(),
        version,
        acl_json: page_record.acl_json.clone(),
        acl_principals: page_record.acl_principals.clone(),
        valid_from: now.clone(),
        valid_to: None,
        superseded_by: None,
        source_url: Some(format!("seed://{}/{}", page.lang, page.slug)),
        source_revision: Some(SEED_VERSION.to_string()),
    };
    store.save_page(&page_record, &revision).await?;

    let logical_path = format!("wiki/{space_id}/{}/{}.md", page.lang, page.slug);
    let canonical = doc_id::canonical_relative_path(&logical_path).map_err(StoreError::Invalid)?;
    let doc_id = doc_id::derive_doc_id(&canonical, &page.content_hash);
    let drafts = chunker::chunk_text(&page.body, chunker::DEFAULT_CHUNK_CHARS);
    let chunks: Vec<ChunkRecord> = drafts
        .iter()
        .enumerate()
        .map(|(index, draft)| {
            let chunk_hash = sha256_hex(draft.text.as_bytes());
            ChunkRecord {
                tenant_id: tenant_id.to_string(),
                chunk_id: doc_id::derive_chunk_id(tenant_id, &doc_id, index as u32, &chunk_hash),
                doc_id: doc_id.clone(),
                space_id: space_id.to_string(),
                seq: index as u32,
                text: draft.text.clone(),
                line_start: draft.line_start,
                line_end: draft.line_end,
                content_hash: chunk_hash,
                acl_json: page_record.acl_json.clone(),
                acl_principals: page_record.acl_principals.clone(),
                lang: page.lang.clone(),
                valid_from: now.clone(),
                valid_to: None,
                superseded_by: None,
                version,
                embedding: None,
            }
        })
        .collect();
    let document = DocumentRecord {
        tenant_id: tenant_id.to_string(),
        doc_id,
        space_id: space_id.to_string(),
        logical_path: canonical,
        source_uri: format!("seed://{}/{}", page.lang, page.slug),
        title: page.title.clone(),
        mime: "text/markdown".to_string(),
        raw_hash: page.content_hash.clone(),
        content_hash: page.content_hash.clone(),
        content: page.body.clone(),
        acl_json: page_record.acl_json.clone(),
        acl_principals: page_record.acl_principals.clone(),
        lang: page.lang.clone(),
        valid_from: now.clone(),
        valid_to: None,
        version: 1,
        superseded_by: None,
        status: "active".to_string(),
        source_role: "transcription".to_string(),
        canonicalization_version: doc_id::CANONICALIZATION_VERSION.to_string(),
        extractor_version: doc_id::EXTRACTOR_VERSION.to_string(),
        chunker_version: doc_id::CHUNKER_VERSION.to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    store.upsert_document(&document, &chunks, None).await?;
    crate::summary::enqueue(store, &document, &document.content_hash).await;
    Ok(SeedPageOutcome::Imported)
}

fn normalize_lang_filter(lang: Option<&str>) -> Option<String> {
    match lang.map(|value| value.trim().to_ascii_lowercase()) {
        Some(value) if value == "all" || value.is_empty() => None,
        Some(value) => Some(value),
        None => None,
    }
}

fn parse_seed_pages(lang_filter: Option<&str>) -> Result<Vec<SeedPage>, Vec<String>> {
    let mut errors = Vec::new();
    let mut pages = Vec::new();
    let mut seen = HashSet::new();
    for file in SEED_FILES {
        let expected_lang = if file.path.starts_with("docs/zh/") {
            "zh"
        } else if file.path.starts_with("docs/en/") {
            "en"
        } else {
            errors.push(format!(
                "seed file outside docs/zh or docs/en: {}",
                file.path
            ));
            continue;
        };
        let parsed = crate::frontmatter::parse(file.content);
        let title = parsed.title.unwrap_or_default();
        let slug = parsed.slug.unwrap_or_default();
        let lang = parsed.lang.unwrap_or_default();
        let summary = parsed.summary.unwrap_or_default();
        if title.trim().is_empty()
            || slug.trim().is_empty()
            || summary.trim().is_empty()
            || parsed.order.is_none()
        {
            errors.push(format!("seed frontmatter incomplete: {}", file.path));
            continue;
        }
        if lang != expected_lang {
            errors.push(format!(
                "seed lang mismatch: {} declares {lang:?}, expected {expected_lang:?}",
                file.path
            ));
            continue;
        }
        if !valid_seed_slug(&slug) {
            errors.push(format!("seed slug invalid: {} -> {slug:?}", file.path));
            continue;
        }
        if let Some(filter) = lang_filter {
            if lang != filter {
                continue;
            }
        }
        if !seen.insert((lang.clone(), slug.clone())) {
            errors.push(format!("duplicate seed slug {lang}/{slug}"));
            continue;
        }
        pages.push(SeedPage {
            lang,
            slug,
            title,
            summary,
            order: parsed.order.unwrap_or(0),
            content_hash: sha256_hex(parsed.body.as_bytes()),
            body: parsed.body,
            source_path: file.path.to_string(),
        });
    }
    pages.sort_by(|a, b| {
        a.lang
            .cmp(&b.lang)
            .then_with(|| a.order.cmp(&b.order))
            .then_with(|| a.slug.cmp(&b.slug))
    });
    if errors.is_empty() {
        Ok(pages)
    } else {
        Err(errors)
    }
}

fn valid_seed_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 200
        && !slug.contains('\\')
        && !slug.contains("..")
        && slug.chars().all(|ch| {
            ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_' || ch == '/'
        })
}

pub fn manifest_hash() -> String {
    let mut hasher = sha2::Sha256::new();
    use sha2::Digest;
    for file in SEED_FILES {
        hasher.update(file.path.as_bytes());
        hasher.update([0u8]);
        hasher.update(file.content.as_bytes());
        hasher.update([0u8]);
    }
    hex::encode(hasher.finalize())
}

pub async fn auto_seed_if_empty(store: &Arc<dyn Store>, config: &Config) -> Option<SeedReport> {
    if !config.seed.enabled || !config.seed.auto || config.profile_name() != ProfileName::Solo {
        return None;
    }
    match store.is_database_empty(&config.storage.tenant_id).await {
        Ok(true) => {}
        Ok(false) => return None,
        Err(err) => {
            tracing::warn!(error = %err, "cannot check seed precondition");
            return None;
        }
    }
    let options = SeedOptions {
        space_id: config.seed.space.clone(),
        lang: None,
    };
    match run_seed(store, config, &options).await {
        Ok(report) => {
            if report.failed > 0 || !report.warnings.is_empty() {
                tracing::warn!(
                    failed = report.failed,
                    warnings = ?report.warnings,
                    "auto seed completed with warnings"
                );
            } else {
                tracing::info!(imported = report.imported, "auto seed completed");
            }
            Some(report)
        }
        Err(err) => {
            tracing::warn!(error = %err, "auto seed failed; serving without seed");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_manifest_has_both_languages_and_twelve_pages_each() {
        let pages = parse_seed_pages(None).unwrap();
        assert_eq!(pages.len(), 24);
        assert_eq!(pages.iter().filter(|p| p.lang == "zh").count(), 12);
        assert_eq!(pages.iter().filter(|p| p.lang == "en").count(), 12);
        assert!(!manifest_hash().is_empty());
    }

    #[test]
    fn language_filter_returns_twelve() {
        assert_eq!(parse_seed_pages(Some("zh")).unwrap().len(), 12);
        assert_eq!(parse_seed_pages(Some("en")).unwrap().len(), 12);
    }
}
