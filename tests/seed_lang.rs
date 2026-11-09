use lkb::config::{Backend, Config, ProfileName};
use lkb::governance::{Policy, Principal, SOLO_TENANT};
use lkb::ingest::doc_id;
use lkb::providers::ProviderSet;
use lkb::search::{search_with_tuning, SearchMode, SearchTuning};
use lkb::seed::{auto_seed_if_empty, run_seed, SeedOptions};
use lkb::store::{self, RevisionRecord, SpaceRecord, Store};
use lkb::util::{now_rfc3339, sha256_hex};
use std::path::Path;
use std::sync::Arc;

fn test_config(path: &Path) -> Config {
    let mut config = Config::default();
    config.profile = Some(ProfileName::Solo);
    config.storage.backend = Backend::Sqlite;
    config.storage.sqlite_path = path.to_path_buf();
    config.storage.tenant_id = SOLO_TENANT.to_string();
    config.seed.space = "product".to_string();
    config
}

async fn open_store(config: &Config) -> Arc<dyn Store> {
    let store = store::open(config).await.unwrap();
    store::bootstrap_space(&store, config).await.unwrap();
    store
}

fn product_policy() -> Policy {
    let mut principal = Principal::solo();
    principal.allowed_space_ids = vec!["product".into()];
    Policy::new(principal, None).unwrap()
}

#[tokio::test]
async fn seed_24_pages_idempotent_and_language_filtered() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("lkb.sqlite"));
    let store = open_store(&config).await;

    let first = run_seed(
        &store,
        &config,
        &SeedOptions {
            space_id: "product".into(),
            lang: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(first.imported, 24, "{}", first.human());
    assert_eq!(first.failed, 0, "{}", first.human());
    assert_eq!(first.by_lang.get("zh").copied(), Some(12));
    assert_eq!(first.by_lang.get("en").copied(), Some(12));
    let pages = store.list_pages(&product_policy(), 100).await.unwrap();
    assert_eq!(pages.len(), 24);
    let first_en = pages
        .iter()
        .find(|page| page.lang == "en" && page.slug == "00-index")
        .expect("seeded English index page");
    assert_eq!(first_en.order_index, 0);
    assert!(!first_en.summary.trim().is_empty());
    let en_orders: Vec<i32> = pages
        .iter()
        .filter(|page| page.lang == "en")
        .map(|page| page.order_index)
        .collect();
    assert!(en_orders.windows(2).all(|pair| pair[0] <= pair[1]));
    assert_eq!(
        store
            .list_documents(SOLO_TENANT, Some("product"))
            .await
            .unwrap()
            .len(),
        24
    );
    let meta = store
        .get_seed_meta(SOLO_TENANT, "product")
        .await
        .unwrap()
        .expect("seed meta");
    assert_eq!(meta.seed_version, lkb::seed::SEED_VERSION);

    let second = run_seed(
        &store,
        &config,
        &SeedOptions {
            space_id: "product".into(),
            lang: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(second.imported, 0, "{}", second.human());
    assert_eq!(second.skipped, 24, "{}", second.human());
    assert_eq!(second.failed, 0, "{}", second.human());
    assert_eq!(second.unchanged, 24, "{}", second.human());

    let providers = ProviderSet::with_none();
    let policy = product_policy();
    let zh = search_with_tuning(
        &store,
        &providers,
        &policy,
        "产品概览",
        SearchMode::Bm25,
        10,
        &SearchTuning {
            lang: Some("zh".into()),
            ..SearchTuning::default()
        },
    )
    .await
    .unwrap();
    assert!(!zh.hits.is_empty());
    assert!(zh.hits.iter().all(|hit| hit.governance.lang == "zh"));
    assert!(zh.hits.iter().all(|hit| hit.path.contains("/zh/")));

    let en = search_with_tuning(
        &store,
        &providers,
        &policy,
        "Overview",
        SearchMode::Bm25,
        10,
        &SearchTuning {
            lang: Some("en".into()),
            ..SearchTuning::default()
        },
    )
    .await
    .unwrap();
    assert!(!en.hits.is_empty());
    assert!(en.hits.iter().all(|hit| hit.governance.lang == "en"));
    assert!(en.hits.iter().all(|hit| hit.path.contains("/en/")));
}

#[tokio::test]
async fn user_modified_page_is_not_overwritten() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("lkb.sqlite"));
    let store = open_store(&config).await;
    run_seed(
        &store,
        &config,
        &SeedOptions {
            space_id: "product".into(),
            lang: None,
        },
    )
    .await
    .unwrap();

    let mut view = store
        .get_page_unfiltered(SOLO_TENANT, "product", "00-index", Some("zh"))
        .await
        .unwrap()
        .expect("seed page");
    let markdown = "# 用户修改\n\n用户内容\n".to_string();
    let hash = sha256_hex(markdown.as_bytes());
    let version = view.page.version + 1;
    let now = now_rfc3339();
    let revision_id = doc_id::derive_revision_id(
        SOLO_TENANT,
        "product",
        "00-index",
        "zh",
        &hash,
        version,
    );
    view.page.version = version;
    view.page.current_revision_id = revision_id.clone();
    view.page.content_hash = hash.clone();
    view.page.updated_at = now.clone();
    let revision = RevisionRecord {
        tenant_id: SOLO_TENANT.into(),
        revision_id,
        space_id: "product".into(),
        slug: "00-index".into(),
        lang: "zh".into(),
        title: "用户修改".into(),
        markdown,
        content_hash: hash,
        author: "alice".into(),
        created_at: now.clone(),
        version,
        acl_json: view.page.acl_json.clone(),
        acl_principals: view.page.acl_principals.clone(),
        valid_from: now.clone(),
        valid_to: None,
        superseded_by: None,
        source_url: None,
        source_revision: None,
    };
    store.save_page(&view.page, &revision).await.unwrap();

    let report = run_seed(
        &store,
        &config,
        &SeedOptions {
            space_id: "product".into(),
            lang: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(report.imported, 0, "{}", report.human());
    assert!(report.user_modified >= 1, "{}", report.human());
    let after = store
        .get_page_unfiltered(SOLO_TENANT, "product", "00-index", Some("zh"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.page.current_revision_id, revision.revision_id);
    assert_eq!(after.revision.author, "alice");
}

#[tokio::test]
async fn auto_seed_runs_only_on_empty_solo_database() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("lkb.sqlite"));
    let store = open_store(&config).await;
    let report = auto_seed_if_empty(&store, &config).await.expect("auto seed");
    assert_eq!(report.imported, 24, "{}", report.human());
    assert!(auto_seed_if_empty(&store, &config).await.is_none());
}

#[tokio::test]
async fn seed_failure_is_reported_and_does_not_break_storage() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("lkb.sqlite"));
    let store = open_store(&config).await;
    let space = SpaceRecord {
        tenant_id: SOLO_TENANT.into(),
        space_id: "product".into(),
        name: "Product".into(),
        visibility: "internal".into(),
        default_acl_json: "not-json".into(),
        owner_principal_id: "system:seed".into(),
        owner_type: "user".into(),
        owner_id: "system:seed".into(),
        status: "active".into(),
        settings: "{}".into(),
        archived_at: None,
        deleted_at: None,
        created_at: now_rfc3339(),
    };
    store.ensure_space(&space).await.unwrap();
    let report = auto_seed_if_empty(&store, &config).await.expect("report");
    assert!(report.failed > 0, "{}", report.human());
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("seed_import_failed")),
        "{}",
        report.human()
    );
    store.health().await.unwrap();
}

#[tokio::test]
async fn seed_enqueues_summary_jobs() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("seed-summary.sqlite"));
    let store = open_store(&config).await;
    let report = run_seed(&store, &config, &SeedOptions { space_id: "product".into(), lang: Some("zh".into()) }).await.unwrap();
    assert!(report.imported > 0);
    let jobs = store.list_summary_jobs("queued", 200).await.unwrap();
    assert!(jobs.len() >= report.imported as usize);
}
