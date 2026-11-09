// SPDX-License-Identifier: MIT OR Apache-2.0
//! OS-conventional directories for CLI/server state.
//!
//! One unified `lkb` layout (see `document/product-spec.md` §2 and ADR 0014);
//! no resolved path ever contains the brand name.
//!
//! | | config | data | logs |
//! | --- | --- | --- | --- |
//! | Windows | `%APPDATA%\lkb` | `%LOCALAPPDATA%\lkb\data` | `%LOCALAPPDATA%\lkb\logs` |
//! | Linux | `$XDG_CONFIG_HOME/lkb` (fallback `~/.config/lkb`) | `$XDG_DATA_HOME/lkb` (fallback `~/.local/share/lkb`) | `$XDG_STATE_HOME/lkb/logs` (fallback `~/.local/state/lkb/logs`) |
//! | macOS | `~/Library/Application Support/lkb` | `~/Library/Application Support/lkb` | `~/Library/Logs/lkb` |
//!
//! A portable install (an exe-adjacent `portable.flag`) keeps config, data and
//! logs under `<exe_dir>/lkb-data`. Each directory resolves as
//! `LKB_CONFIG_DIR` / `LKB_DATA_DIR` / `LKB_LOG_DIR` > portable > OS default.
//!
//! The config *file* resolves as explicit `--config` > `LKB_CONFIG_FILE` >
//! `./lkb.toml` (when present, keeping the repo-local development workflow) >
//! the OS default `<config_dir>/lkb.toml`. CLI auth credentials live in a
//! separate file, `<config_dir>/config.toml`.

use std::env;
use std::path::{Path, PathBuf};

/// Directory name under every OS base (config, data and log roots).
pub const APP_DIR: &str = "lkb";
/// Main configuration file name.
pub const CONFIG_FILE_NAME: &str = "lkb.toml";
/// CLI auth credentials file (`lkb auth login`); never the main config.
pub const AUTH_FILE_NAME: &str = "config.toml";
/// SQLite file name inside the data directory.
pub const SQLITE_FILE_NAME: &str = "lkb.sqlite";
/// Marker next to the executable that switches to the portable layout.
pub const PORTABLE_FLAG: &str = "portable.flag";
/// Root of a portable install, next to the executable.
pub const PORTABLE_DIR: &str = "lkb-data";

/// Overrides the config directory (`<dir>/lkb.toml`, `<dir>/config.toml`).
pub const CONFIG_DIR_ENV: &str = "LKB_CONFIG_DIR";
/// Overrides the data directory (`<dir>/lkb.sqlite`).
pub const DATA_DIR_ENV: &str = "LKB_DATA_DIR";
/// Overrides the log directory.
pub const LOG_DIR_ENV: &str = "LKB_LOG_DIR";
/// Overrides the main configuration file path.
pub const CONFIG_FILE_ENV: &str = "LKB_CONFIG_FILE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Platform {
    Windows,
    Macos,
    Linux,
}

const fn current_platform() -> Platform {
    if cfg!(target_os = "windows") {
        Platform::Windows
    } else if cfg!(target_os = "macos") {
        Platform::Macos
    } else {
        Platform::Linux
    }
}

/// Environment lookup, injected so the layout rules are testable without
/// mutating process-global environment variables.
type EnvGet<'a> = &'a dyn Fn(&str) -> Option<String>;

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

fn path_var(get: EnvGet, key: &str) -> Option<PathBuf> {
    non_empty(get(key)).map(PathBuf::from)
}

fn home_dir(get: EnvGet) -> Option<PathBuf> {
    path_var(get, "HOME").or_else(|| path_var(get, "USERPROFILE"))
}

fn env_get(key: &str) -> Option<String> {
    env::var(key).ok()
}

fn exe_dir() -> Option<PathBuf> {
    env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
}

fn cwd() -> PathBuf {
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// `<exe_dir>/lkb-data` when the exe-adjacent portable flag exists.
fn portable_root(exe_dir: Option<&Path>) -> Option<PathBuf> {
    let dir = exe_dir?;
    if dir.join(PORTABLE_FLAG).is_file() {
        Some(dir.join(PORTABLE_DIR))
    } else {
        None
    }
}

fn base_or_dot(base: Option<PathBuf>) -> PathBuf {
    base.unwrap_or_else(|| PathBuf::from("."))
}

/// Config directory: `LKB_CONFIG_DIR` > portable > OS default.
fn config_root(platform: Platform, get: EnvGet, exe: Option<&Path>) -> PathBuf {
    if let Some(dir) = path_var(get, CONFIG_DIR_ENV) {
        return dir;
    }
    if let Some(root) = portable_root(exe) {
        return root;
    }
    let base = match platform {
        Platform::Windows => path_var(get, "APPDATA")
            .or_else(|| home_dir(get).map(|home| home.join("AppData").join("Roaming"))),
        Platform::Macos => {
            home_dir(get).map(|home| home.join("Library").join("Application Support"))
        }
        Platform::Linux => path_var(get, "XDG_CONFIG_HOME")
            .or_else(|| home_dir(get).map(|home| home.join(".config"))),
    };
    base_or_dot(base).join(APP_DIR)
}

/// Data directory: `LKB_DATA_DIR` > portable > OS default.
fn data_root(platform: Platform, get: EnvGet, exe: Option<&Path>) -> PathBuf {
    if let Some(dir) = path_var(get, DATA_DIR_ENV) {
        return dir;
    }
    if let Some(root) = portable_root(exe) {
        return root.join("data");
    }
    match platform {
        Platform::Windows => {
            let base = path_var(get, "LOCALAPPDATA")
                .or_else(|| home_dir(get).map(|home| home.join("AppData").join("Local")));
            base_or_dot(base).join(APP_DIR).join("data")
        }
        Platform::Macos => {
            base_or_dot(home_dir(get).map(|home| home.join("Library").join("Application Support")))
                .join(APP_DIR)
        }
        Platform::Linux => base_or_dot(
            path_var(get, "XDG_DATA_HOME")
                .or_else(|| home_dir(get).map(|home| home.join(".local").join("share"))),
        )
        .join(APP_DIR),
    }
}

/// Log directory: `LKB_LOG_DIR` > portable > OS default.
fn log_root(platform: Platform, get: EnvGet, exe: Option<&Path>) -> PathBuf {
    if let Some(dir) = path_var(get, LOG_DIR_ENV) {
        return dir;
    }
    if let Some(root) = portable_root(exe) {
        return root.join("logs");
    }
    match platform {
        Platform::Windows => {
            let base = path_var(get, "LOCALAPPDATA")
                .or_else(|| home_dir(get).map(|home| home.join("AppData").join("Local")));
            base_or_dot(base).join(APP_DIR).join("logs")
        }
        Platform::Macos => {
            base_or_dot(home_dir(get).map(|home| home.join("Library").join("Logs"))).join(APP_DIR)
        }
        Platform::Linux => base_or_dot(
            path_var(get, "XDG_STATE_HOME")
                .or_else(|| home_dir(get).map(|home| home.join(".local").join("state"))),
        )
        .join(APP_DIR)
        .join("logs"),
    }
}

/// Resolved config directory (`<dir>/lkb.toml`, `<dir>/config.toml`).
pub fn config_dir() -> PathBuf {
    config_root(current_platform(), &env_get, exe_dir().as_deref())
}

/// Resolved data directory (holds `lkb.sqlite` by default).
pub fn data_dir() -> PathBuf {
    data_root(current_platform(), &env_get, exe_dir().as_deref())
}

/// Resolved log directory.
pub fn log_dir() -> PathBuf {
    log_root(current_platform(), &env_get, exe_dir().as_deref())
}

/// Default SQLite file: `<data_dir>/lkb.sqlite`.
pub fn default_sqlite_path() -> PathBuf {
    data_dir().join(SQLITE_FILE_NAME)
}

/// CLI auth credentials file: `<config_dir>/config.toml`.
pub fn auth_config_file() -> PathBuf {
    config_dir().join(AUTH_FILE_NAME)
}

/// Resolve the main config file for the current process.
///
/// `explicit` is the value of `--config` when the user passed it.
pub fn resolve_config_file(explicit: Option<&Path>) -> PathBuf {
    resolve_config_file_in(
        current_platform(),
        explicit,
        &env_get,
        &cwd(),
        exe_dir().as_deref(),
    )
}

fn resolve_config_file_in(
    platform: Platform,
    explicit: Option<&Path>,
    get: EnvGet,
    cwd: &Path,
    exe: Option<&Path>,
) -> PathBuf {
    if let Some(path) = explicit {
        return path.to_path_buf();
    }
    if let Some(path) = path_var(get, CONFIG_FILE_ENV) {
        return path;
    }
    let local = cwd.join(CONFIG_FILE_NAME);
    if local.is_file() {
        return local;
    }
    config_root(platform, get, exe).join(CONFIG_FILE_NAME)
}

/// Multi-line `key = value` block of resolved paths for `lkb config check`
/// and `lkb config init`.
pub fn describe(config_file: &Path) -> String {
    format!(
        "config.file = {file}\n\
         config.file_exists = {exists}\n\
         paths.config_dir = {config}\n\
         paths.data_dir = {data}\n\
         paths.log_dir = {log}\n\
         paths.sqlite_path = {sqlite}\n",
        file = config_file.display(),
        exists = config_file.is_file(),
        config = config_dir().display(),
        data = data_dir().display(),
        log = log_dir().display(),
        sqlite = default_sqlite_path().display(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).to_string())
        }
    }

    #[test]
    fn windows_defaults_match_the_spec_layout() {
        let get = fake_env(&[
            ("APPDATA", r"C:\Users\ada\AppData\Roaming"),
            ("LOCALAPPDATA", r"C:\Users\ada\AppData\Local"),
        ]);
        let roaming = PathBuf::from(r"C:\Users\ada\AppData\Roaming");
        let local = PathBuf::from(r"C:\Users\ada\AppData\Local");
        assert_eq!(
            config_root(Platform::Windows, &get, None),
            roaming.join("lkb")
        );
        assert_eq!(
            data_root(Platform::Windows, &get, None),
            local.join("lkb").join("data")
        );
        assert_eq!(
            log_root(Platform::Windows, &get, None),
            local.join("lkb").join("logs")
        );
    }

    #[test]
    fn windows_falls_back_to_user_profile_without_appdata() {
        let get = fake_env(&[("USERPROFILE", r"C:\Users\ada")]);
        let home = PathBuf::from(r"C:\Users\ada");
        assert_eq!(
            config_root(Platform::Windows, &get, None),
            home.join("AppData").join("Roaming").join("lkb")
        );
        assert_eq!(
            data_root(Platform::Windows, &get, None),
            home.join("AppData").join("Local").join("lkb").join("data")
        );
    }

    #[test]
    fn linux_defaults_match_the_xdg_layout() {
        let get = fake_env(&[("HOME", "/home/ada")]);
        let home = PathBuf::from("/home/ada");
        assert_eq!(
            config_root(Platform::Linux, &get, None),
            home.join(".config").join("lkb")
        );
        assert_eq!(
            data_root(Platform::Linux, &get, None),
            home.join(".local").join("share").join("lkb")
        );
        assert_eq!(
            log_root(Platform::Linux, &get, None),
            home.join(".local").join("state").join("lkb").join("logs")
        );
    }

    #[test]
    fn linux_prefers_xdg_variables_over_home_fallbacks() {
        let get = fake_env(&[
            ("HOME", "/home/ada"),
            ("XDG_CONFIG_HOME", "/xdg/cfg"),
            ("XDG_DATA_HOME", "/xdg/data"),
            ("XDG_STATE_HOME", "/xdg/state"),
        ]);
        assert_eq!(
            config_root(Platform::Linux, &get, None),
            PathBuf::from("/xdg/cfg").join("lkb")
        );
        assert_eq!(
            data_root(Platform::Linux, &get, None),
            PathBuf::from("/xdg/data").join("lkb")
        );
        assert_eq!(
            log_root(Platform::Linux, &get, None),
            PathBuf::from("/xdg/state").join("lkb").join("logs")
        );
    }

    #[test]
    fn macos_defaults_match_the_application_support_layout() {
        let get = fake_env(&[("HOME", "/Users/ada")]);
        let home = PathBuf::from("/Users/ada");
        let support = home.join("Library").join("Application Support");
        assert_eq!(
            config_root(Platform::Macos, &get, None),
            support.join("lkb")
        );
        assert_eq!(data_root(Platform::Macos, &get, None), support.join("lkb"));
        assert_eq!(
            log_root(Platform::Macos, &get, None),
            home.join("Library").join("Logs").join("lkb")
        );
    }

    #[test]
    fn explicit_env_overrides_win_over_everything() {
        let get = fake_env(&[
            ("LKB_CONFIG_DIR", "/override/cfg"),
            ("LKB_DATA_DIR", "/override/data"),
            ("LKB_LOG_DIR", "/override/logs"),
        ]);
        assert_eq!(
            config_root(Platform::Windows, &get, None),
            PathBuf::from("/override/cfg")
        );
        assert_eq!(
            data_root(Platform::Windows, &get, None),
            PathBuf::from("/override/data")
        );
        assert_eq!(
            log_root(Platform::Windows, &get, None),
            PathBuf::from("/override/logs")
        );
    }

    #[test]
    fn blank_env_overrides_are_ignored() {
        let get = fake_env(&[("LKB_DATA_DIR", "   "), ("HOME", "/home/ada")]);
        assert_eq!(
            data_root(Platform::Linux, &get, None),
            PathBuf::from("/home/ada")
                .join(".local")
                .join("share")
                .join("lkb")
        );
    }

    #[test]
    fn portable_layout_applies_when_the_flag_is_present() {
        let temp = tempfile::tempdir().unwrap();
        let exe = temp.path();
        std::fs::write(exe.join(PORTABLE_FLAG), "").unwrap();
        let get = fake_env(&[
            ("APPDATA", r"C:\Users\ada\AppData\Roaming"),
            ("LOCALAPPDATA", r"C:\Users\ada\AppData\Local"),
        ]);
        let root = exe.join(PORTABLE_DIR);
        assert_eq!(config_root(Platform::Windows, &get, Some(exe)), root);
        assert_eq!(
            data_root(Platform::Windows, &get, Some(exe)),
            root.join("data")
        );
        assert_eq!(
            log_root(Platform::Windows, &get, Some(exe)),
            root.join("logs")
        );
    }

    #[test]
    fn env_overrides_win_over_the_portable_flag() {
        let temp = tempfile::tempdir().unwrap();
        let exe = temp.path();
        std::fs::write(exe.join(PORTABLE_FLAG), "").unwrap();
        let get = fake_env(&[("LKB_DATA_DIR", "/override/data")]);
        assert_eq!(
            data_root(Platform::Windows, &get, Some(exe)),
            PathBuf::from("/override/data")
        );
    }

    #[test]
    fn portable_flag_absence_keeps_os_defaults() {
        let temp = tempfile::tempdir().unwrap();
        let get = fake_env(&[("LOCALAPPDATA", r"C:\Users\ada\AppData\Local")]);
        assert_eq!(
            data_root(Platform::Windows, &get, Some(temp.path())),
            PathBuf::from(r"C:\Users\ada\AppData\Local")
                .join("lkb")
                .join("data")
        );
    }

    #[test]
    fn config_file_priority_explicit_env_cwd_default() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = temp.path().join("cwd");
        std::fs::create_dir_all(&cwd).unwrap();
        let local = cwd.join(CONFIG_FILE_NAME);
        std::fs::write(&local, "profile = \"solo\"\n").unwrap();
        let get = fake_env(&[("LKB_CONFIG_FILE", "/env/lkb.toml"), ("HOME", "/home/ada")]);
        let explicit = Path::new("/cli/explicit.toml");

        // Explicit `--config` beats the env override, the cwd file and the default.
        assert_eq!(
            resolve_config_file_in(Platform::Linux, Some(explicit), &get, &cwd, None),
            explicit
        );
        // `LKB_CONFIG_FILE` beats the cwd file and the default.
        assert_eq!(
            resolve_config_file_in(Platform::Linux, None, &get, &cwd, None),
            PathBuf::from("/env/lkb.toml")
        );
        // A cwd `lkb.toml` beats the OS default.
        let get = fake_env(&[("HOME", "/home/ada")]);
        assert_eq!(
            resolve_config_file_in(Platform::Linux, None, &get, &cwd, None),
            local
        );
        // With nothing else present, the OS default applies.
        let empty = temp.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert_eq!(
            resolve_config_file_in(Platform::Linux, None, &get, &empty, None),
            PathBuf::from("/home/ada")
                .join(".config")
                .join("lkb")
                .join(CONFIG_FILE_NAME)
        );
    }

    #[test]
    fn resolved_paths_never_contain_the_brand() {
        let get = fake_env(&[("HOME", "/home/ada")]);
        let mut paths = Vec::new();
        for platform in [Platform::Windows, Platform::Macos, Platform::Linux] {
            paths.push(config_root(platform, &get, None));
            paths.push(data_root(platform, &get, None));
            paths.push(log_root(platform, &get, None));
        }
        paths.push(PathBuf::from(PORTABLE_DIR));
        paths.push(PathBuf::from(PORTABLE_FLAG));
        for path in &paths {
            let text = path.to_string_lossy().to_ascii_lowercase();
            assert!(!text.contains("linxira"), "brand leaked into {text}");
        }
        for name in [APP_DIR, CONFIG_FILE_NAME, AUTH_FILE_NAME, SQLITE_FILE_NAME] {
            assert!(!name.to_ascii_lowercase().contains("linxira"));
        }
    }
}
