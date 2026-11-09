// SPDX-License-Identifier: MIT OR Apache-2.0
use assert_cmd::Command;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn lkb() -> Command {
    Command::cargo_bin("lkb").expect("lkb binary")
}

fn database_path(dir: &Path) -> PathBuf {
    dir.join("lkb.sqlite")
}

/// Filesystem path lines must never carry the brand name; project URLs may.
fn assert_no_brand_in_path_lines(text: &str) {
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        let is_path_line = lower.starts_with("config.file = ")
            || lower.starts_with("config initialized: ")
            || lower.starts_with("config already exists: ")
            || lower.starts_with("paths.");
        if is_path_line {
            assert!(!lower.contains("linxira"), "brand in path line: {line}");
        }
    }
}

#[test]
fn config_check_prints_masked_effective_config() {
    let temp = tempfile::tempdir().unwrap();
    let config_dir = temp.path().join("cfg");
    let data_dir = temp.path().join("data");
    let log_dir = temp.path().join("logs");
    let output = lkb()
        .args(["config", "check"])
        .current_dir(temp.path())
        .env_remove("LKB_CONFIG_FILE")
        .env("LKB_CONFIG_DIR", &config_dir)
        .env("LKB_DATA_DIR", &data_dir)
        .env("LKB_LOG_DIR", &log_dir)
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
    // Resolved paths: the OS-default config file and the env-overridden dirs.
    assert!(stdout.contains(&format!(
        "config.file = {}",
        config_dir.join("lkb.toml").display()
    )));
    assert!(stdout.contains("config.file_exists = false"));
    assert!(stdout.contains(&format!("paths.config_dir = {}", config_dir.display())));
    assert!(stdout.contains(&format!("paths.data_dir = {}", data_dir.display())));
    assert!(stdout.contains(&format!("paths.log_dir = {}", log_dir.display())));
    assert!(stdout.contains(&format!(
        "paths.sqlite_path = {}",
        data_dir.join("lkb.sqlite").display()
    )));
    // No filesystem path value may carry the brand name (project URLs are not paths).
    assert_no_brand_in_path_lines(&stdout);
}

#[test]
fn config_file_resolution_priority_is_flag_env_cwd_default() {
    let temp = tempfile::tempdir().unwrap();
    let config_dir = temp.path().join("cfg");
    let cwd = temp.path().join("cwd");
    std::fs::create_dir_all(&cwd).unwrap();
    let local = cwd.join("lkb.toml");
    std::fs::write(&local, "profile = \"solo\"\n").unwrap();

    let config_file_line = |output: &std::process::Output| -> String {
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        stdout
            .lines()
            .find_map(|line| line.strip_prefix("config.file = ").map(str::to_string))
            .unwrap_or_else(|| panic!("no config.file line in {stdout}"))
    };

    // A current-directory `lkb.toml` wins over the OS default.
    let output = lkb()
        .args(["config", "check"])
        .current_dir(&cwd)
        .env_remove("LKB_CONFIG_FILE")
        .env("LKB_CONFIG_DIR", &config_dir)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(config_file_line(&output), local.display().to_string());
    assert!(String::from_utf8_lossy(&output.stdout).contains("config.file_exists = true"));

    // `LKB_CONFIG_FILE` wins over the current-directory file.
    let env_file = temp.path().join("from-env.toml");
    std::fs::write(&env_file, "profile = \"solo\"\n").unwrap();
    let output = lkb()
        .args(["config", "check"])
        .current_dir(&cwd)
        .env("LKB_CONFIG_FILE", &env_file)
        .env("LKB_CONFIG_DIR", &config_dir)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(config_file_line(&output), env_file.display().to_string());

    // Explicit `--config` wins over `LKB_CONFIG_FILE`.
    let explicit = temp.path().join("explicit.toml");
    let output = lkb()
        .arg("--config")
        .arg(&explicit)
        .args(["config", "check"])
        .current_dir(&cwd)
        .env("LKB_CONFIG_FILE", &env_file)
        .env("LKB_CONFIG_DIR", &config_dir)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(config_file_line(&output), explicit.display().to_string());
    assert!(String::from_utf8_lossy(&output.stdout).contains("config.file_exists = false"));
}

#[test]
fn config_init_writes_resolved_default_config_and_keeps_user_edits() {
    let temp = tempfile::tempdir().unwrap();
    let config_dir = temp.path().join("cfg");
    let data_dir = temp.path().join("data");
    let cwd = temp.path().join("cwd");
    std::fs::create_dir_all(&cwd).unwrap();
    let config_file = config_dir.join("lkb.toml");

    let run = |args: &[&str]| {
        lkb()
            .args(args)
            .current_dir(&cwd)
            .env_remove("LKB_CONFIG_FILE")
            .env_remove("LKB_SQLITE_PATH")
            .env_remove("LKB_PROFILE")
            .env("LKB_CONFIG_DIR", &config_dir)
            .env("LKB_DATA_DIR", &data_dir)
            .output()
            .unwrap()
    };

    // Nothing is created by `check`; it only reports the missing file.
    let check = run(&["config", "check"]);
    assert!(check.status.success());
    assert!(String::from_utf8_lossy(&check.stdout).contains("config.file_exists = false"));

    let init = run(&["config", "init"]);
    assert!(
        init.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    let stdout = String::from_utf8_lossy(&init.stdout);
    assert!(stdout.contains(&format!("config initialized: {}", config_file.display())));
    assert!(stdout.contains(&format!("paths.data_dir = {}", data_dir.display())));
    assert_no_brand_in_path_lines(&stdout);

    let written = std::fs::read_to_string(&config_file).unwrap();
    assert!(written.contains("profile = \"solo\""));
    assert!(written.contains("backend = \"sqlite\""));
    assert!(written.contains("sqlite_path"));

    // `config check` now loads the generated file and its sqlite path.
    let check = run(&["config", "check"]);
    assert!(check.status.success());
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(stdout.contains(&format!("config.file = {}", config_file.display())));
    assert!(stdout.contains("config.file_exists = true"));
    assert!(stdout.contains(&format!(
        "storage.sqlite_path = {}",
        data_dir.join("lkb.sqlite").display()
    )));

    // A second `init` never overwrites user edits.
    let edited = format!("{written}\n# user edit\n");
    std::fs::write(&config_file, &edited).unwrap();
    let again = run(&["config", "init"]);
    assert!(again.status.success());
    assert!(String::from_utf8_lossy(&again.stdout).contains("left unchanged"));
    assert_eq!(std::fs::read_to_string(&config_file).unwrap(), edited);
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
