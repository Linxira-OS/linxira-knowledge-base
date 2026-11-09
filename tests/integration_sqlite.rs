use lkb::answer::{build_answer, AnswerOptions};
use lkb::config::{Backend, Config, ProfileName};
use lkb::contract::AnswerMode;
use lkb::governance::{Policy, SOLO_SPACE, SOLO_TENANT};
use lkb::ingest::{export_jsonl, import_path, ImportOptions};
use lkb::providers::ProviderSet;
use lkb::search::{search, SearchMode};
use lkb::store;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn test_config(path: &Path) -> Config {
    let mut config = Config::default();
    config.profile = Some(ProfileName::Solo);
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
    let report = import_path(
        &store,
        &examples,
        &ImportOptions::default(),
        None,
        None,
    )
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
    assert!(!response.hits.is_empty(), "degraded: {:?}", response.degraded);
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
    let count = export_jsonl(
        &store,
        SOLO_TENANT,
        Some(SOLO_SPACE),
        &mut exported,
    )
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
    assert!(report_two.failed.is_empty(), "report: {}", report_two.human());

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
