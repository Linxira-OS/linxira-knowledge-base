// SPDX-License-Identifier: MIT OR Apache-2.0
use lkb::config::{Backend, Config, ProfileName};
use lkb::governance::{
    Acl, Policy, Principal, PrincipalKind, TokenScope, UserRole, SOLO_SPACE, SOLO_TENANT,
};
use lkb::ingest::{import_path, ImportOptions};
use lkb::providers::fake::{FakeEmbedding, FakeEventExtractor};
use lkb::providers::{EventExtractor, ProviderSet};
use lkb::search::{search, search_with_tuning, SearchMode, SearchTuning};
use lkb::store::{self, Store};
use std::path::Path;
use std::sync::Arc;

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

async fn open_store(config: &Config) -> Arc<dyn Store> {
    let store = store::open(config).await.unwrap();
    store::bootstrap_space(&store, config).await.unwrap();
    store
}

fn providers_with_extractor(extractor: Arc<dyn EventExtractor>) -> ProviderSet {
    let mut providers = ProviderSet::with_none();
    providers.extractor = Some(extractor);
    providers.extractor_requested = true;
    providers
}

fn policy_for(principal: Principal) -> Policy {
    Policy::new(principal, None).unwrap()
}

async fn write_source(dir: &Path, name: &str, content: &str) {
    std::fs::write(dir.join(name), content).unwrap();
}

#[tokio::test]
async fn event_mode_hits_and_two_hop_expands() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("lkb.sqlite"));
    let store = open_store(&config).await;
    let source = temp.path().join("docs");
    std::fs::create_dir_all(&source).unwrap();
    write_source(
        &source,
        "alpha.md",
        "# Alpha\n\nAlpha links Beta. [[Alpha:concept]] [[Beta:concept]]\n",
    )
    .await;
    write_source(
        &source,
        "beta.md",
        "# Beta\n\nBeta links Gamma. [[Beta:concept]] [[Gamma:concept]]\n",
    )
    .await;
    let extractor = FakeEventExtractor::new();
    let report = import_path(
        &store,
        &source,
        &ImportOptions::default(),
        None,
        Some(&extractor),
    )
    .await
    .unwrap();
    assert!(report.failed.is_empty(), "{}", report.human());
    assert!(report.warnings.is_empty(), "{}", report.human());
    assert!(store.has_events(&Policy::solo()).await.unwrap());

    let providers = providers_with_extractor(Arc::new(FakeEventExtractor::new()));
    let depth_one = search(
        &store,
        &providers,
        &Policy::solo(),
        "Alpha",
        SearchMode::Event,
        10,
    )
    .await
    .unwrap();
    assert_eq!(depth_one.mode_used, SearchMode::Event);
    assert!(
        depth_one.hits.iter().any(|hit| hit.path.contains("alpha")),
        "hits: {:?}",
        depth_one
            .hits
            .iter()
            .map(|hit| &hit.path)
            .collect::<Vec<_>>()
    );
    assert!(depth_one.hits.iter().all(|hit| hit.event_id.is_some()));

    let tuning = SearchTuning {
        event_depth: 2,
        ..SearchTuning::default()
    };
    let depth_two = search_with_tuning(
        &store,
        &providers,
        &Policy::solo(),
        "Alpha",
        SearchMode::Event,
        10,
        &tuning,
    )
    .await
    .unwrap();
    assert!(
        depth_two.hits.iter().any(|hit| hit.path.contains("beta")),
        "depth2 hits: {:?}",
        depth_two
            .hits
            .iter()
            .map(|hit| &hit.path)
            .collect::<Vec<_>>()
    );

    let auto = search(
        &store,
        &providers,
        &Policy::solo(),
        "Alpha",
        SearchMode::Auto,
        10,
    )
    .await
    .unwrap();
    assert_eq!(auto.mode_used, SearchMode::Hybrid);

    let no_seed = search(
        &store,
        &providers,
        &Policy::solo(),
        "NoSuchEntityToken",
        SearchMode::Event,
        5,
    )
    .await
    .unwrap();
    assert!(
        no_seed
            .degraded
            .iter()
            .any(|item| item.code == "event_no_seed"),
        "degraded: {:?}",
        no_seed.degraded
    );
}

#[tokio::test]
async fn event_governance_filters_acl_tenant_and_expiry() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("lkb.sqlite"));
    let store = open_store(&config).await;
    let source = temp.path().join("secure");
    std::fs::create_dir_all(&source).unwrap();
    write_source(
        &source,
        "secret.md",
        "# Secret\n\nSecret link [[Secret:concept]]\n",
    )
    .await;
    let extractor = FakeEventExtractor::new();
    let options = ImportOptions {
        acl: Some(Acl::allow_list(vec!["alice".into()], Vec::new())),
        ..Default::default()
    };
    let report = import_path(&store, &source, &options, None, Some(&extractor))
        .await
        .unwrap();
    assert!(report.failed.is_empty(), "{}", report.human());

    let providers = providers_with_extractor(Arc::new(FakeEventExtractor::new()));
    let alice = policy_for(Principal::local_user(
        "alice",
        SOLO_TENANT,
        vec![SOLO_SPACE.into()],
        Vec::new(),
    ));
    let alice_hits = search(&store, &providers, &alice, "Secret", SearchMode::Event, 5)
        .await
        .unwrap();
    assert!(!alice_hits.hits.is_empty(), "alice should see the event");

    let bob = policy_for(Principal::local_user(
        "bob",
        SOLO_TENANT,
        vec![SOLO_SPACE.into()],
        Vec::new(),
    ));
    let bob_hits = search(&store, &providers, &bob, "Secret", SearchMode::Event, 5)
        .await
        .unwrap();
    assert!(bob_hits.hits.is_empty(), "bob must not see alice's event");

    let other_tenant = policy_for(Principal {
        principal_id: "other".into(),
        display_name: "Other".into(),
        tenant_id: "t_other".into(),
        allowed_space_ids: vec![SOLO_SPACE.into()],
        groups: Vec::new(),
        allow_all: false,
        kind: PrincipalKind::Human,
        scope: TokenScope::Read,
        tenant_role: UserRole::User,
        token_id: None,
    });
    let tenant_hits = search(
        &store,
        &providers,
        &other_tenant,
        "Secret",
        SearchMode::Event,
        5,
    )
    .await
    .unwrap();
    assert!(tenant_hits.hits.is_empty(), "cross-tenant event leak");

    let other_space = policy_for(Principal::local_user(
        "alice",
        SOLO_TENANT,
        vec!["s_other".into()],
        Vec::new(),
    ));
    let space_hits = search(
        &store,
        &providers,
        &other_space,
        "Secret",
        SearchMode::Event,
        5,
    )
    .await
    .unwrap();
    assert!(space_hits.hits.is_empty(), "cross-space event leak");

    let expired = temp.path().join("expired.jsonl");
    std::fs::write(
        &expired,
        "{\"path\":\"expired.md\",\"title\":\"Expired\",\"content\":\"Old event [[Old:concept]]\",\"valid_from\":\"2020-01-01T00:00:00Z\",\"valid_to\":\"2021-01-01T00:00:00Z\"}\n",
    )
    .unwrap();
    let report = import_path(
        &store,
        &expired,
        &ImportOptions {
            acl: Some(Acl::allow_all()),
            ..Default::default()
        },
        None,
        Some(&extractor),
    )
    .await
    .unwrap();
    assert_eq!(report.imported, 1);
    let expired_hits = search(
        &store,
        &providers,
        &Policy::solo(),
        "Old",
        SearchMode::Event,
        5,
    )
    .await
    .unwrap();
    assert!(
        expired_hits.hits.is_empty(),
        "expired event must not be visible: {:?}",
        expired_hits.hits
    );
}

#[tokio::test]
async fn failing_extraction_is_reported_and_degraded() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("lkb.sqlite"));
    let store = open_store(&config).await;
    let source = temp.path().join("docs");
    std::fs::create_dir_all(&source).unwrap();
    write_source(&source, "alpha.md", "# Alpha\n\nAlpha [[Alpha:concept]]\n").await;
    let failing = FakeEventExtractor::failing();
    let report = import_path(
        &store,
        &source,
        &ImportOptions::default(),
        None,
        Some(&failing),
    )
    .await
    .unwrap();
    assert!(report.imported >= 1);
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("event extraction failed")),
        "warnings: {:?}",
        report.warnings
    );
    assert!(!store.has_events(&Policy::solo()).await.unwrap());

    let providers = providers_with_extractor(Arc::new(FakeEventExtractor::failing()));
    let response = search(
        &store,
        &providers,
        &Policy::solo(),
        "Alpha",
        SearchMode::Event,
        5,
    )
    .await
    .unwrap();
    assert!(
        response
            .degraded
            .iter()
            .any(|item| item.code == "event_extraction_unavailable"),
        "degraded: {:?}",
        response.degraded
    );
    assert!(
        !response.hits.is_empty(),
        "event mode should fall back to BM25"
    );
}

#[tokio::test]
async fn hybrid_fuses_bm25_dense_and_event() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("lkb.sqlite"));
    let store = open_store(&config).await;
    let source = temp.path().join("docs");
    std::fs::create_dir_all(&source).unwrap();
    write_source(
        &source,
        "alpha.md",
        "# Alpha\n\nAlpha links Beta. [[Alpha:concept]] [[Beta:concept]]\n",
    )
    .await;
    let embedding = FakeEmbedding::new(24);
    let extractor = FakeEventExtractor::new();
    let report = import_path(
        &store,
        &source,
        &ImportOptions::default(),
        Some(&embedding),
        Some(&extractor),
    )
    .await
    .unwrap();
    assert!(report.failed.is_empty(), "{}", report.human());

    let mut providers = ProviderSet::with_none();
    providers.embedding = Some(Arc::new(FakeEmbedding::new(24)));
    providers.embedding_requested = true;
    providers.extractor = Some(Arc::new(FakeEventExtractor::new()));
    providers.extractor_requested = true;
    let response = search(
        &store,
        &providers,
        &Policy::solo(),
        "Alpha",
        SearchMode::Hybrid,
        5,
    )
    .await
    .unwrap();
    assert_eq!(response.mode_used, SearchMode::Hybrid);
    assert!(!response.hits.is_empty());
    assert!(
        response.hits[0].path.contains("alpha"),
        "top hit: {:?}",
        response.hits[0].path
    );
    assert!(
        !response
            .degraded
            .iter()
            .any(|item| item.code == "event_not_enabled"),
        "degraded: {:?}",
        response.degraded
    );
}
