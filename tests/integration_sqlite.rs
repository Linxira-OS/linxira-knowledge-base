// SPDX-License-Identifier: MIT OR Apache-2.0
use lkb::answer::{build_answer, AnswerOptions};
use lkb::config::{Backend, Config, ProfileName};
use lkb::contract::AnswerMode;
use lkb::governance::{Policy, Principal, SOLO_SPACE, SOLO_TENANT};
use lkb::ingest::{export_jsonl, import_path, ImportOptions};
use lkb::providers::{EmbeddingProvider, JudgeProvider, ProviderError, ProviderSet};
use lkb::search::{search, search_with_tuning, SearchMode, SearchTuning};
use lkb::store;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct FixedEmbedding;

#[async_trait::async_trait]
impl EmbeddingProvider for FixedEmbedding {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        Ok(texts.iter().map(|_| vec![1.0, 0.0]).collect())
    }

    fn id(&self) -> String {
        "fixed-embedding".to_string()
    }
}

struct CountingJudge {
    calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl JudgeProvider for CountingJudge {
    async fn rerank(&self, _query: &str, candidates: &[String]) -> Result<Vec<f32>, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok((0..candidates.len())
            .map(|index| (candidates.len() - index) as f32)
            .collect())
    }

    fn id(&self) -> String {
        "counting-judge".to_string()
    }
}

async fn set_all_chunk_embeddings(path: &Path, vector: &[f32]) {
    let sqlite = lkb::store::sqlite::SqliteStore::open(path)
        .await
        .expect("open sqlite for embedding seed");
    let json = serde_json::to_string(vector).unwrap();
    sqlx::query("UPDATE chunks SET embedding_json = ?")
        .bind(json)
        .execute(sqlite.pool())
        .await
        .expect("seed chunk embeddings");
}

fn test_config(path: &Path) -> Config {
    let mut config = Config {
        profile: Some(ProfileName::Solo),
        ..Default::default()
    };
    config.storage.backend = Backend::Sqlite;
    config.storage.sqlite_path = path.to_path_buf();
    config.storage.tenant_id = SOLO_TENANT.to_string();
    config.storage.default_space = SOLO_SPACE.to_string();
    config
}

async fn open_test_store(config: &Config) -> std::sync::Arc<dyn store::Store> {
    let store = store::open(config).await.expect("open sqlite store");
    store::bootstrap_space(&store, config)
        .await
        .expect("bootstrap solo space");
    store
}

#[tokio::test]
async fn sqlite_import_search_answer_export_roundtrip() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("lkb.sqlite"));
    let store = open_test_store(&config).await;

    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let report = import_path(&store, &examples, &ImportOptions::default(), None, None)
        .await
        .expect("import examples");
    assert!(report.imported >= 7, "report: {}", report.human());
    assert!(report.failed.is_empty(), "report: {}", report.human());

    let providers = ProviderSet::with_none();
    let policy = Policy::solo();
    let response = search(
        &store,
        &providers,
        &policy,
        "doc_id 如何派生",
        SearchMode::Bm25,
        5,
    )
    .await
    .expect("search");
    assert!(
        !response.hits.is_empty(),
        "degraded: {:?}",
        response.degraded
    );
    assert!(
        response
            .hits
            .iter()
            .any(|hit| hit.path.contains("06-文档身份与重建")),
        "hits: {:?}",
        response.hits.iter().map(|h| &h.path).collect::<Vec<_>>()
    );

    let answer = build_answer(
        &store,
        &providers,
        &policy,
        "doc_id 如何派生",
        &AnswerOptions::default(),
    )
    .await
    .expect("answer");
    assert_eq!(answer.mode, AnswerMode::Extractive);
    assert!(!answer.citations.is_empty());
    assert!(answer.knowledge_as_of.is_some());

    let mut exported = Vec::new();
    let count = export_jsonl(&store, SOLO_TENANT, Some(SOLO_SPACE), &mut exported)
        .await
        .expect("export");
    assert!(count >= 7);
    let first_ids: BTreeSet<String> = store
        .list_documents(SOLO_TENANT, Some(SOLO_SPACE))
        .await
        .unwrap()
        .into_iter()
        .map(|doc| doc.doc_id)
        .collect();

    let jsonl_path = temp.path().join("export.jsonl");
    std::fs::write(&jsonl_path, &exported).unwrap();

    let temp_two = tempfile::tempdir().unwrap();
    let config_two = test_config(&temp_two.path().join("lkb.sqlite"));
    let store_two = open_test_store(&config_two).await;
    let report_two = import_path(
        &store_two,
        &jsonl_path,
        &ImportOptions::default(),
        None,
        None,
    )
    .await
    .expect("re-import export");
    assert!(report_two.imported >= 7, "report: {}", report_two.human());
    assert!(
        report_two.failed.is_empty(),
        "report: {}",
        report_two.human()
    );

    let second_ids: BTreeSet<String> = store_two
        .list_documents(SOLO_TENANT, Some(SOLO_SPACE))
        .await
        .unwrap()
        .into_iter()
        .map(|doc| doc.doc_id)
        .collect();
    assert_eq!(first_ids, second_ids);

    let second_response = search(
        &store_two,
        &providers,
        &policy,
        "doc_id 如何派生",
        SearchMode::Bm25,
        5,
    )
    .await
    .expect("search after re-import");
    assert!(!second_response.hits.is_empty());
}

#[tokio::test]
async fn doc_id_changes_when_content_changes_and_old_record_links_forward() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("lkb.sqlite"));
    let store = open_test_store(&config).await;

    let source = temp.path().join("source");
    std::fs::create_dir_all(&source).unwrap();
    let file = source.join("note.md");
    std::fs::write(&file, "# Note\n\nfirst version\n").unwrap();

    let first = import_path(&store, &source, &ImportOptions::default(), None, None)
        .await
        .unwrap();
    assert_eq!(first.imported, 1);

    let docs_first = store
        .list_documents(SOLO_TENANT, Some(SOLO_SPACE))
        .await
        .unwrap();
    assert_eq!(docs_first.len(), 1);
    let first_id = docs_first[0].doc_id.clone();

    std::fs::write(&file, "# Note\n\nsecond version\n").unwrap();
    let second = import_path(&store, &source, &ImportOptions::default(), None, None)
        .await
        .unwrap();
    assert_eq!(second.imported, 1);

    let docs = store
        .list_documents(SOLO_TENANT, Some(SOLO_SPACE))
        .await
        .unwrap();
    assert_eq!(docs.len(), 2);
    let active: Vec<_> = docs.iter().filter(|doc| doc.status == "active").collect();
    let superseded: Vec<_> = docs
        .iter()
        .filter(|doc| doc.status == "superseded")
        .collect();
    assert_eq!(active.len(), 1);
    assert_eq!(superseded.len(), 1);
    assert_ne!(active[0].doc_id, first_id);
    assert_eq!(
        superseded[0].superseded_by.as_deref(),
        Some(active[0].doc_id.as_str())
    );
    assert!(superseded[0].valid_to.is_some());
}

async fn import_examples(store: &Arc<dyn store::Store>) {
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let report = import_path(store, &examples, &ImportOptions::default(), None, None)
        .await
        .expect("import examples");
    assert!(report.imported >= 7, "report: {}", report.human());
    assert!(report.failed.is_empty(), "report: {}", report.human());
}

#[tokio::test]
async fn auto_uses_hybrid_when_embedding_vectors_exist() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("lkb.sqlite");
    let config = test_config(&db_path);
    let store = open_test_store(&config).await;
    import_examples(&store).await;
    set_all_chunk_embeddings(&db_path, &[1.0, 0.0]).await;

    let mut providers = ProviderSet::with_none();
    providers.embedding = Some(Arc::new(FixedEmbedding));
    let policy = Policy::solo();
    let response = search(
        &store,
        &providers,
        &policy,
        "doc_id 如何派生",
        SearchMode::Auto,
        5,
    )
    .await
    .expect("auto search");
    assert_eq!(response.mode_requested, SearchMode::Auto);
    assert_eq!(response.mode_used, SearchMode::Hybrid);
    assert!(
        !response.hits.is_empty(),
        "degraded: {:?}",
        response.degraded
    );
    assert!(
        !response
            .degraded
            .iter()
            .any(|item| item.code == "index_missing"),
        "degraded: {:?}",
        response.degraded
    );
    assert!(
        !response
            .degraded
            .iter()
            .any(|item| item.code == "decision_unavailable"),
        "hybrid does not require a judge: {:?}",
        response.degraded
    );

    // The probe is scoped by Policy visibility: another tenant sees nothing.
    let other_tenant = Policy::new(
        Principal::local_user("bob", "t_other", vec!["s_default".into()], vec![]),
        None,
    )
    .unwrap();
    assert!(!store.has_embeddings(&other_tenant).await.unwrap());
}

#[tokio::test]
async fn auto_without_vectors_falls_back_to_bm25_and_reports_index_missing() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("lkb.sqlite"));
    let store = open_test_store(&config).await;
    import_examples(&store).await;

    let mut providers = ProviderSet::with_none();
    providers.embedding = Some(Arc::new(FixedEmbedding));
    let policy = Policy::solo();
    let response = search(
        &store,
        &providers,
        &policy,
        "doc_id 如何派生",
        SearchMode::Auto,
        5,
    )
    .await
    .expect("auto search");
    assert_eq!(response.mode_used, SearchMode::Bm25);
    assert!(!response.hits.is_empty());
    assert!(
        response
            .degraded
            .iter()
            .any(|item| item.code == "index_missing"),
        "degraded: {:?}",
        response.degraded
    );
}

#[tokio::test]
async fn hybrid_judge_runs_after_rrf_and_can_be_disabled() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("lkb.sqlite");
    let config = test_config(&db_path);
    let store = open_test_store(&config).await;
    import_examples(&store).await;
    set_all_chunk_embeddings(&db_path, &[1.0, 0.0]).await;

    let calls = Arc::new(AtomicUsize::new(0));
    let mut providers = ProviderSet::with_none();
    providers.embedding = Some(Arc::new(FixedEmbedding));
    providers.judge = Some(Arc::new(CountingJudge {
        calls: calls.clone(),
    }));
    let policy = Policy::solo();

    let response = search_with_tuning(
        &store,
        &providers,
        &policy,
        "doc_id 如何派生",
        SearchMode::Hybrid,
        5,
        &SearchTuning::default(),
    )
    .await
    .expect("hybrid search");
    assert!(!response.hits.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 1, "judge runs after RRF");
    assert!(response.hits.iter().any(|hit| hit.rerank_score.is_some()));
    assert!(
        !response
            .degraded
            .iter()
            .any(|item| item.code == "decision_unavailable"),
        "degraded: {:?}",
        response.degraded
    );

    let disabled = SearchTuning {
        judge_after_hybrid: false,
        ..Default::default()
    };
    let response = search_with_tuning(
        &store,
        &providers,
        &policy,
        "doc_id 如何派生",
        SearchMode::Hybrid,
        5,
        &disabled,
    )
    .await
    .expect("hybrid search without judge");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "judge_after_hybrid = false must skip the judge"
    );
    assert!(response.hits.iter().all(|hit| hit.rerank_score.is_none()));
}
