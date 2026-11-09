//! First-run configuration for the desktop shell: paths, `lkb.toml`, the
//! PostgreSQL connection metadata and its secret.
//!
//! The generated `lkb.toml` is a normal kernel config (the `[desktop]` table
//! is ignored by `lkb serve`); the shell keeps everything it needs to rebuild
//! the connection URL next to it. Passwords never go into the TOML file:
//! they live in the OS credential store, or in `pg.env` (0600 on Unix) when
//! the credential store is unavailable.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

const KEYRING_SERVICE: &str = "lkb-desktop";
const KEYRING_USER: &str = "postgres";
/// Directory name used everywhere under the OS user directories.
const APP_DIR: &str = "lkb";
pub const DATABASE_URL_ENV: &str = "LKB_DATABASE_URL";
const SSL_MODES: &[&str] = &["disable", "allow", "prefer", "require", "verify-ca", "verify-full"];

/// Everything the shell keeps per user, per the unified `lkb` layout:
/// - Windows: config `%APPDATA%\lkb`, data `%LOCALAPPDATA%\lkb\data`, logs `%LOCALAPPDATA%\lkb\logs`
/// - Linux: config `$XDG_CONFIG_HOME/lkb`, data `$XDG_DATA_HOME/lkb`, logs `$XDG_STATE_HOME/lkb/logs`
/// - macOS: config and data `~/Library/Application Support/lkb`, logs `~/Library/Logs/lkb`
pub struct Paths {
    pub config_file: PathBuf,
    pub data_dir: PathBuf,
    pub sidecar_log: PathBuf,
    pub pg_env: PathBuf,
}

impl Paths {
    pub fn sqlite_path(&self) -> PathBuf {
        self.data_dir.join("lkb.sqlite")
    }
}

pub fn paths() -> Result<Paths, String> {
    let config_root = dirs::config_dir()
        .map(|base| base.join(APP_DIR))
        .ok_or_else(|| "无法定位系统配置目录".to_string())?;
    Ok(Paths {
        config_file: config_root.join("lkb.toml"),
        pg_env: config_root.join("pg.env"),
        data_dir: data_dir()?,
        sidecar_log: log_dir()?.join("sidecar.log"),
    })
}

/// Data root: `%LOCALAPPDATA%\lkb\data` on Windows, the XDG data directory
/// (`Application Support` on macOS) otherwise.
fn data_dir() -> Result<PathBuf, String> {
    let base = if cfg!(windows) {
        dirs::data_local_dir()
    } else {
        dirs::data_dir()
    };
    let root = base
        .ok_or_else(|| "无法定位系统数据目录".to_string())?
        .join(APP_DIR);
    Ok(if cfg!(windows) { root.join("data") } else { root })
}

/// Log root: `%LOCALAPPDATA%\lkb\logs` on Windows, `$XDG_STATE_HOME/lkb/logs`
/// on Linux and `~/Library/Logs/lkb` on macOS.
fn log_dir() -> Result<PathBuf, String> {
    let base = if cfg!(windows) {
        dirs::data_local_dir()
    } else if cfg!(target_os = "macos") {
        dirs::home_dir().map(|home| home.join("Library").join("Logs"))
    } else {
        dirs::state_dir()
    };
    let root = base.ok_or_else(|| "无法定位系统日志目录".to_string())?;
    Ok(if cfg!(target_os = "macos") {
        root.join(APP_DIR)
    } else {
        root.join(APP_DIR).join("logs")
    })
}

/// The sidecar invocation derived from the saved configuration.
pub struct LaunchPlan {
    pub config_file: PathBuf,
    pub log_file: PathBuf,
    pub web_dir: Option<PathBuf>,
    pub profile: String,
    pub database_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetupPayload {
    /// `sqlite` or `postgres`.
    pub mode: String,
    #[serde(default)]
    pub postgres: Option<PostgresPayload>,
}

#[derive(Debug, Deserialize)]
pub struct PostgresPayload {
    pub host: String,
    #[serde(default)]
    pub port: u16,
    pub database: String,
    pub user: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub sslmode: String,
}

#[derive(Debug, Serialize)]
pub struct SaveOutcome {
    /// `keyring`, `file` or `none` (SQLite).
    pub password_store: String,
    pub warning: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SetupState {
    pub config_file: String,
    pub data_dir: String,
    pub sqlite_path: String,
    pub config_exists: bool,
    pub last_error: Option<String>,
    pub current: Option<SetupSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct SetupSnapshot {
    pub mode: String,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub user: Option<String>,
    pub sslmode: Option<String>,
    pub password_saved: bool,
}

/// Shape of `lkb.toml` for the fields the shell owns. Unknown keys written by
/// the kernel are not preserved on rewrite; this file is the shell's own.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct ShellConfig {
    profile: String,
    server: ServerSection,
    auth: AuthSection,
    storage: StorageSection,
    desktop: DesktopSection,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            profile: "solo".to_string(),
            server: ServerSection::default(),
            auth: AuthSection::default(),
            storage: StorageSection::default(),
            desktop: DesktopSection::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct ServerSection {
    bind: String,
}

impl Default for ServerSection {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:7331".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct AuthSection {
    mode: String,
}

impl Default for AuthSection {
    fn default() -> Self {
        Self {
            mode: "none".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct StorageSection {
    backend: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    sqlite_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url_env: Option<String>,
}

impl Default for StorageSection {
    fn default() -> Self {
        Self {
            backend: "sqlite".to_string(),
            sqlite_path: None,
            url_env: None,
        }
    }
}

/// Shell-owned table; `lkb serve` ignores unknown top-level tables.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct DesktopSection {
    mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    database: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sslmode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    secret: Option<String>,
}

/// What the setup window should pre-fill and show.
pub fn state(app: &AppHandle, paths: &Paths) -> SetupState {
    let config_exists = paths.config_file.is_file();
    let current = if config_exists {
        fs::read_to_string(&paths.config_file)
            .ok()
            .and_then(|text| toml::from_str::<ShellConfig>(&text).ok())
            .map(|config| SetupSnapshot {
                mode: if config.desktop.mode.is_empty() {
                    config.storage.backend.clone()
                } else {
                    config.desktop.mode.clone()
                },
                host: config.desktop.host.clone(),
                port: config.desktop.port,
                database: config.desktop.database.clone(),
                user: config.desktop.user.clone(),
                sslmode: config.desktop.sslmode.clone(),
                password_saved: match config.desktop.secret.as_deref() {
                    Some("keyring") => true,
                    Some("file") => paths.pg_env.is_file(),
                    _ => false,
                },
            })
    } else {
        None
    };
    let last_error = crate::engine::last_error(app);
    SetupState {
        config_file: paths.config_file.display().to_string(),
        data_dir: paths.data_dir.display().to_string(),
        sqlite_path: paths.sqlite_path().display().to_string(),
        config_exists,
        last_error,
        current,
    }
}

pub fn save(paths: &Paths, payload: &SetupPayload) -> Result<SaveOutcome, String> {
    match payload.mode.as_str() {
        "sqlite" => save_sqlite(paths),
        "postgres" => {
            let postgres = payload
                .postgres
                .as_ref()
                .ok_or_else(|| "缺少 PostgreSQL 连接信息".to_string())?;
            save_postgres(paths, postgres)
        }
        other => Err(format!("未知存储模式 {other:?}；可选 sqlite 或 postgres")),
    }
}

fn save_sqlite(paths: &Paths) -> Result<SaveOutcome, String> {
    fs::create_dir_all(&paths.data_dir)
        .map_err(|err| format!("无法创建数据目录 {}：{err}", paths.data_dir.display()))?;
    let mut config = ShellConfig::default();
    config.desktop.mode = "sqlite".to_string();
    config.storage.sqlite_path = Some(paths.sqlite_path().display().to_string());
    write_config(&paths.config_file, &config)?;
    remove_secret(paths);
    Ok(SaveOutcome {
        password_store: "none".to_string(),
        warning: None,
    })
}

fn save_postgres(paths: &Paths, payload: &PostgresPayload) -> Result<SaveOutcome, String> {
    let host = payload.host.trim().to_string();
    validate_host(&host)?;
    let database = require(payload.database.trim(), "数据库名")?.to_string();
    let user = require(payload.user.trim(), "用户名")?.to_string();
    let sslmode = if payload.sslmode.trim().is_empty() {
        "prefer"
    } else {
        require_sslmode(&payload.sslmode)?
    };
    let port = if payload.port == 0 { 5432 } else { payload.port };

    let password = if payload.password.is_empty() {
        load_password(paths)
            .map_err(|_| "请输入 PostgreSQL 密码（没有可沿用的已保存凭据）".to_string())?
    } else {
        payload.password.clone()
    };
    let url = database_url(&host, port, &database, &user, &password, sslmode);
    let (password_store, warning) = store_password(paths, &url, &password);

    let mut config = ShellConfig::default();
    config.profile = "team".to_string();
    config.auth.mode = "session".to_string();
    config.storage.backend = "postgres".to_string();
    config.storage.url_env = Some(DATABASE_URL_ENV.to_string());
    config.desktop = DesktopSection {
        mode: "postgres".to_string(),
        host: Some(host),
        port: Some(port),
        database: Some(database),
        user: Some(user),
        sslmode: Some(sslmode.to_string()),
        secret: Some(password_store.to_string()),
    };
    write_config(&paths.config_file, &config)?;
    Ok(SaveOutcome {
        password_store: password_store.to_string(),
        warning,
    })
}

fn write_config(path: &Path, config: &ShellConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("无法创建配置目录 {}：{err}", parent.display()))?;
    }
    let body = toml::to_string_pretty(config)
        .map_err(|err| format!("无法序列化配置：{err}"))?;
    let text = format!("# Linxira 桌面版配置：由首启向导生成，可手工编辑（重启应用生效）。\n{body}");
    fs::write(path, text).map_err(|err| format!("无法写入配置 {}：{err}", path.display()))
}

/// Load the sidecar invocation, including the PostgreSQL URL.
pub fn load_plan(app: &AppHandle, paths: &Paths) -> Result<LaunchPlan, String> {
    let text = fs::read_to_string(&paths.config_file)
        .map_err(|err| format!("无法读取配置 {}：{err}", paths.config_file.display()))?;
    let config: ShellConfig = toml::from_str(&text)
        .map_err(|err| format!("无法解析配置 {}：{err}", paths.config_file.display()))?;
    let backend = {
        let backend = config.storage.backend.trim().to_ascii_lowercase();
        if backend.is_empty() {
            config.desktop.mode.trim().to_ascii_lowercase()
        } else {
            backend
        }
    };
    let (profile, database_url) = match backend.as_str() {
        "postgres" | "postgresql" | "pg" => (
            profile_or(&config, "team"),
            Some(resolve_database_url(paths, &config)?),
        ),
        _ => (profile_or(&config, "solo"), None),
    };
    Ok(LaunchPlan {
        config_file: paths.config_file.clone(),
        log_file: paths.sidecar_log.clone(),
        web_dir: resolve_web_dir(app),
        profile,
        database_url,
    })
}

fn profile_or(config: &ShellConfig, fallback: &str) -> String {
    let profile = config.profile.trim();
    if profile.is_empty() {
        fallback.to_string()
    } else {
        profile.to_string()
    }
}

fn resolve_database_url(paths: &Paths, config: &ShellConfig) -> Result<String, String> {
    if let Ok(password) = load_password(paths) {
        if let Some(url) = build_url_from_desktop(&config.desktop, &password) {
            return Ok(url);
        }
    }
    if let Some(url) = read_pg_env(paths) {
        return Ok(url);
    }
    Err(format!(
        "找不到可用的 PostgreSQL 密码：系统凭据库不可用，且 {} 不存在或无效；请重新运行设置。",
        paths.pg_env.display()
    ))
}

fn build_url_from_desktop(desktop: &DesktopSection, password: &str) -> Option<String> {
    let host = desktop.host.as_deref()?;
    let port = desktop.port?;
    let database = desktop.database.as_deref()?;
    let user = desktop.user.as_deref()?;
    let sslmode = require_sslmode(desktop.sslmode.as_deref().unwrap_or("prefer")).ok()?;
    Some(database_url(host, port, database, user, password, sslmode))
}

/// The bundled `web/dist`, copied next to the executable by `tauri build`.
fn resolve_web_dir(app: &AppHandle) -> Option<PathBuf> {
    let web = app
        .path()
        .resource_dir()
        .ok()?
        .join("resources")
        .join("web");
    if web.join("index.html").is_file() {
        Some(web)
    } else {
        None
    }
}

fn validate_host(host: &str) -> Result<(), String> {
    if host.is_empty() {
        return Err("主机不能为空".to_string());
    }
    if host.chars().any(char::is_whitespace) {
        return Err("主机不能包含空白字符".to_string());
    }
    if host.contains(['/', '?', '#', '@']) {
        return Err("主机不能包含 / ? # @".to_string());
    }
    Ok(())
}

fn require<'a>(value: &'a str, label: &str) -> Result<&'a str, String> {
    if value.is_empty() {
        Err(format!("{label}不能为空"))
    } else {
        Ok(value)
    }
}

fn require_sslmode(value: &str) -> Result<&'static str, String> {
    SSL_MODES
        .iter()
        .find(|mode| mode.eq_ignore_ascii_case(value.trim()))
        .copied()
        .ok_or_else(|| format!("不支持的 SSL 模式 {value:?}；可选 {}", SSL_MODES.join(", ")))
}

fn database_url(host: &str, port: u16, database: &str, user: &str, password: &str, sslmode: &str) -> String {
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    format!(
        "postgres://{}:{}@{}:{}/{}?sslmode={}",
        encode_component(user),
        encode_component(password),
        host,
        port,
        encode_component(database),
        sslmode
    )
}

/// Percent-encode everything outside the RFC 3986 unreserved set, so user,
/// password and database name cannot break out of their URL component.
fn encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for &byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 3 <= bytes.len() {
            let high = (bytes[index + 1] as char).to_digit(16);
            let low = (bytes[index + 2] as char).to_digit(16);
            if let (Some(high), Some(low)) = (high, low) {
                out.push((high * 16 + low) as u8);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(out).ok()
}

/// Store the password in the OS credential store; fall back to `pg.env`.
fn store_password(paths: &Paths, url: &str, password: &str) -> (&'static str, Option<String>) {
    let keyring_result = keyring_entry().and_then(|entry| {
        entry
            .set_password(password)
            .map_err(|err| err.to_string())
            .map(|_| ())
    });
    match keyring_result {
        Ok(()) => {
            let _ = fs::remove_file(&paths.pg_env);
            ("keyring", None)
        }
        Err(keyring_error) => match write_pg_env(paths, url) {
            Ok(()) => (
                "file",
                Some(format!(
                    "系统凭据库不可用（{keyring_error}）；数据库密码已写入 {}（Unix 权限 0600，Windows 已尽力收紧），请勿分享该文件。",
                    paths.pg_env.display()
                )),
            ),
            Err(write_error) => (
                "file",
                Some(format!(
                    "系统凭据库不可用（{keyring_error}），且写入 {} 失败（{write_error}）；配置已保存，但下次启动可能找不到密码。",
                    paths.pg_env.display()
                )),
            ),
        },
    }
}

fn load_password(paths: &Paths) -> Result<String, String> {
    // Keyring first: pg.env only exists when the keyring was unavailable at
    // save time.
    if let Ok(password) = keyring_password() {
        return Ok(password);
    }
    read_pg_env(paths)
        .and_then(|url| password_from_url(&url))
        .ok_or_else(|| "没有已保存的密码".to_string())
}

fn read_pg_env(paths: &Paths) -> Option<String> {
    let text = fs::read_to_string(&paths.pg_env).ok()?;
    let prefix = format!("{DATABASE_URL_ENV}=");
    text.lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix(&prefix))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn password_from_url(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("postgres://")
        .or_else(|| url.strip_prefix("postgresql://"))?;
    let (userinfo, _) = rest.split_once('@')?;
    let (_, password) = userinfo.split_once(':')?;
    percent_decode(password)
}

fn write_pg_env(paths: &Paths, url: &str) -> std::io::Result<()> {
    let contents = format!(
        "# Linxira 桌面版备用凭据（系统凭据库不可用时由首启向导写入）。\n\
         # 内含 PostgreSQL 密码，请保持文件私密。\n\
         {DATABASE_URL_ENV}={url}\n"
    );
    if let Some(parent) = paths.pg_env.parent() {
        fs::create_dir_all(parent)?;
    }
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&paths.pg_env)?;
        file.write_all(contents.as_bytes())?;
    }
    #[cfg(windows)]
    {
        fs::write(&paths.pg_env, contents)?;
        restrict_windows_acl(&paths.pg_env);
    }
    Ok(())
}

#[cfg(windows)]
fn restrict_windows_acl(path: &Path) {
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "%USERNAME%".to_string());
    let _ = std::process::Command::new("icacls")
        .arg(path)
        .arg("/inheritance:r")
        .arg("/grant:r")
        .arg(format!("{user}:F"))
        .output();
}

fn remove_secret(paths: &Paths) {
    if let Ok(entry) = keyring_entry() {
        let _ = entry.delete_credential();
    }
    let _ = fs::remove_file(&paths.pg_env);
}

fn keyring_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|err| err.to_string())
}

fn keyring_password() -> Result<String, String> {
    keyring_entry()?
        .get_password()
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_never_use_branded_directories() {
        let paths = paths().unwrap();
        for path in [
            &paths.config_file,
            &paths.pg_env,
            &paths.data_dir,
            &paths.sidecar_log,
        ] {
            let text = path.to_string_lossy().to_ascii_lowercase();
            assert!(
                !text.contains("linxira"),
                "brand leaked into a path: {}",
                path.display()
            );
            assert!(
                text.contains("lkb"),
                "path is not under lkb: {}",
                path.display()
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_paths_match_the_lkb_layout() {
        let paths = paths().unwrap();
        assert!(paths.config_file.ends_with(r"lkb\lkb.toml"));
        assert!(paths.pg_env.ends_with(r"lkb\pg.env"));
        assert!(paths.data_dir.ends_with(r"lkb\data"));
        assert!(paths.sidecar_log.ends_with(r"lkb\logs\sidecar.log"));
    }

    #[test]
    fn encode_component_escapes_url_metacharacters() {
        assert_eq!(encode_component("p@ss:w/rd"), "p%40ss%3Aw%2Frd");
        assert_eq!(encode_component("simple-Pass_1.~"), "simple-Pass_1.~");
    }

    #[test]
    fn password_round_trips_through_the_url() {
        let url = database_url("db.example.com", 5432, "lkb", "lkb", "p@ss:w/rd", "require");
        assert_eq!(
            url,
            "postgres://lkb:p%40ss%3Aw%2Frd@db.example.com:5432/lkb?sslmode=require"
        );
        assert_eq!(password_from_url(&url).as_deref(), Some("p@ss:w/rd"));
    }

    #[test]
    fn ipv6_hosts_are_bracketed() {
        let url = database_url("::1", 5432, "lkb", "lkb", "x", "prefer");
        assert!(url.contains("@[::1]:5432/lkb"));
    }

    #[test]
    fn sslmode_allowlist_rejects_garbage() {
        assert_eq!(require_sslmode("require").unwrap(), "require");
        assert_eq!(require_sslmode("VERIFY-FULL").unwrap(), "verify-full");
        assert!(require_sslmode("require&statement-cache-capacity=1").is_err());
    }

    #[test]
    fn shell_config_round_trips_through_toml() {
        let config = ShellConfig {
            profile: "team".to_string(),
            auth: AuthSection {
                mode: "session".to_string(),
            },
            storage: StorageSection {
                backend: "postgres".to_string(),
                url_env: Some(DATABASE_URL_ENV.to_string()),
                ..StorageSection::default()
            },
            desktop: DesktopSection {
                mode: "postgres".to_string(),
                host: Some("127.0.0.1".to_string()),
                port: Some(5432),
                database: Some("lkb".to_string()),
                user: Some("postgres".to_string()),
                sslmode: Some("prefer".to_string()),
                secret: Some("keyring".to_string()),
            },
            ..ShellConfig::default()
        };
        let text = toml::to_string_pretty(&config).unwrap();
        assert!(text.contains("backend = \"postgres\""));
        assert!(!text.contains("sqlite_path"));
        let parsed: ShellConfig = toml::from_str(&text).unwrap();
        assert_eq!(parsed.desktop.host.as_deref(), Some("127.0.0.1"));
        assert_eq!(parsed.desktop.port, Some(5432));
        assert_eq!(parsed.storage.backend, "postgres");
    }
}
