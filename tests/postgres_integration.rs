// SPDX-License-Identifier: MIT OR Apache-2.0
use lkb::governance::{Acl, Policy, Principal, PrincipalKind, TokenScope, UserRole};
use lkb::providers::ProviderSet;
use lkb::search::{search, SearchMode};
use lkb::store::postgres::PostgresStore;
use lkb::store::Store;
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::test]
async fn postgres_roundtrip_is_skipped_without_test_url() {
    let url = match std::env::var("LKB_TEST_PG_URL") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => {
            eprintln!(
                "SKIP postgres integration test: LKB_TEST_PG_URL is not set; \
                 refusing to pretend it passed"
            );
            return;
        }
    };

    let store: Arc<dyn Store> = Arc::new(
        PostgresStore::open(&url)
            .await
            .expect("connect to LKB_TEST_PG_URL"),
    );
    store.migrate().await.expect("migrate");
    store.health().await.expect("health");

    let tenant = format!("t_pg_{}", lkb::util::random_hex(4));
    let space = format!("s_pg_{}", lkb::util::random_hex(4));
    let acl = Acl::allow_all();
    let ex = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let options = lkb::ingest::ImportOptions {
        tenant_id: tenant.clone(),
        space_id: space.clone(),
        acl: Some(acl),
        allow_implicit_allow_all: false,
        ..Default::default()
    };
    let report = lkb::ingest::import_path(&store, &ex, &options, None, None)
        .await
        .expect("import examples into postgres");
    assert!(report.imported > 0, "report: {}", report.human());

    let principal = Principal {
        principal_id: "pg_tester".into(),
        display_name: "PG tester".into(),
        tenant_id: tenant,
        allowed_space_ids: vec![space],
        groups: Vec::new(),
        allow_all: true,
        kind: PrincipalKind::Human,
        scope: TokenScope::Read,
        tenant_role: UserRole::User,
        token_id: None,
    };
    let policy = Policy::new(principal, None).unwrap();
    let providers = ProviderSet::with_none();
    let response = search(&store, &providers, &policy, "doc_id", SearchMode::Bm25, 5)
        .await
        .expect("postgres search");
    assert!(
        !response.hits.is_empty(),
        "expected keyword hits, degraded: {:?}",
        response.degraded
    );
}
