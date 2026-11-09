use assert_cmd::Command;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn lkb() -> Command {
    Command::cargo_bin("lkb").expect("lkb binary")
}

fn database_path(dir: &Path) -> PathBuf {
    dir.join("lkb.sqlite")
}

#[test]
fn config_check_prints_masked_effective_config() {
    let temp = tempfile::tempdir().unwrap();
    let output = lkb()
        .arg("config")
        .arg("check")
        .env("LKB_SQLITE_PATH", database_path(temp.path()))
        .env("LKB_PROFILE", "solo")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("profile = solo"));
    assert!(stdout.contains("storage.backend = sqlite"));
    assert!(stdout.contains("auth.mode = none"));
}

#[test]
fn cli_user_import_search_export_and_tokens() {
    let temp = tempfile::tempdir().unwrap();
    let db = database_path(temp.path());
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");

    let user = lkb()
        .args(["user", "add", "alice"])
        .env("LKB_SQLITE_PATH", &db)
        .env("LKB_PROFILE", "solo")
        .env("LKB_USER_PASSWORD", "correct horse battery staple")
        .output()
        .unwrap();
    assert!(
        user.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&user.stderr)
    );
    assert!(String::from_utf8_lossy(&user.stdout).contains("user added: alice"));

    let import = lkb()
        .args(["import", examples.to_str().unwrap(), "--json"])
        .env("LKB_SQLITE_PATH", &db)
        .env("LKB_PROFILE", "solo")
        .output()
        .unwrap();
    assert!(
        import.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&import.stderr)
    );
    let report: Value = serde_json::from_slice(&import.stdout).unwrap();
    assert!(report["imported"].as_u64().unwrap() >= 7, "{report}");

    let search = lkb()
        .args(["search", "doc_id 如何派生", "--json", "--k", "5"])
        .env("LKB_SQLITE_PATH", &db)
        .env("LKB_PROFILE", "solo")
        .output()
        .unwrap();
    assert!(
        search.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&search.stderr)
    );
    let response: Value = serde_json::from_slice(&search.stdout).unwrap();
    assert!(
        !response["hits"].as_array().unwrap().is_empty(),
        "response: {response}"
    );

    let export_path = temp.path().join("export.jsonl");
    let export = lkb()
        .args(["export", "--out", export_path.to_str().unwrap()])
        .env("LKB_SQLITE_PATH", &db)
        .env("LKB_PROFILE", "solo")
        .output()
        .unwrap();
    assert!(
        export.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&export.stderr)
    );
    let lines = std::fs::read_to_string(&export_path).unwrap();
    assert!(lines.lines().count() >= 7);

    let token = lkb()
        .args(["token", "create", "alice", "--scope", "read"])
        .env("LKB_SQLITE_PATH", &db)
        .env("LKB_PROFILE", "solo")
        .output()
        .unwrap();
    assert!(
        token.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&token.stderr)
    );
    let token_stdout = String::from_utf8_lossy(&token.stdout);
    let raw = token_stdout
        .lines()
        .find_map(|line| line.strip_prefix("token (shown once): "))
        .expect("raw token printed once")
        .to_string();
    assert!(raw.starts_with("lkb_"));

    let token_search = lkb()
        .args(["search", "doc_id", "--json", "--token", &raw])
        .env("LKB_SQLITE_PATH", &db)
        .env("LKB_PROFILE", "solo")
        .output()
        .unwrap();
    assert!(
        token_search.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&token_search.stderr)
    );
}
