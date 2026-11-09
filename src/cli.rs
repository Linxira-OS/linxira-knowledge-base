// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::config::Config;
use crate::error::StoreError;
use crate::governance::{
    Acl, GroupRole, Policy, Principal, PrincipalKind, PrincipalType, SpaceRole, TokenScope,
};
use crate::paths;
use crate::providers::ProviderSet;
use crate::search::{SearchMode, SearchTuning};
use crate::store::{
    self, GroupMemberRecord, GroupRecord, QuotaScope, SpaceMemberRecord, SpaceRecord, Store,
    TokenRecord, UserRecord,
};
use crate::util::{
    now_rfc3339, random_hex, sha256_hex_str, ISSUES_URL, REPOSITORY_URL, SECURITY_URL,
};
use anyhow::{bail, Context};
use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
use argon2::Argon2;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Long form of `lkb --version`: the bare `lkb -V` stays terse. clap takes a
/// `&'static str` here, so the URL is concatenated at compile time; the util
/// test pins `CARGO_PKG_REPOSITORY` to `REPOSITORY_URL`.
const LONG_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("CARGO_PKG_REPOSITORY"),
    ")"
);

/// `lkb --help` footer: project links plus the filesystem locations the CLI
/// resolves by default (the same values `lkb config check` prints).
fn after_help() -> String {
    format!(
        "Repository:  {REPOSITORY_URL}\nReport bugs: {ISSUES_URL}\nSecurity:    {SECURITY_URL}\n\n\
         Default paths (override with LKB_CONFIG_DIR / LKB_DATA_DIR / LKB_LOG_DIR):\n\
         config  {config}\n\
         data    {data}\n\
         logs    {logs}",
        config = paths::resolve_config_file(None).display(),
        data = paths::data_dir().display(),
        logs = paths::log_dir().display(),
    )
}

#[derive(Debug, Parser)]
#[command(
    name = "lkb",
    version,
    long_version = LONG_VERSION,
    about = "Layered knowledge base",
    after_help = after_help()
)]
struct Cli {
    /// Configuration file: this flag when given, else `LKB_CONFIG_FILE`, else
    /// `./lkb.toml` when it exists, else `<config_dir>/lkb.toml`.
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[arg(long, global = true)]
    token: Option<String>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Serve,
    Seed {
        #[arg(long)]
        space: Option<String>,
        #[arg(long)]
        lang: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Import {
        path: PathBuf,
        #[arg(long)]
        space: Option<String>,
        #[arg(long)]
        acl_json: Option<String>,
        #[arg(long)]
        lang: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Export {
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long)]
        space: Option<String>,
    },
    Search {
        query: String,
        #[arg(long)]
        mode: Option<String>,
        #[arg(long, default_value_t = 10)]
        k: usize,
        #[arg(long)]
        space: Option<String>,
        #[arg(long)]
        lang: Option<String>,
        #[arg(long)]
        as_of: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Ask {
        query: String,
        #[arg(long)]
        mode: Option<String>,
        #[arg(long, default_value_t = 8)]
        limit: usize,
        #[arg(long)]
        space: Option<String>,
        #[arg(long)]
        lang: Option<String>,
        #[arg(long)]
        as_of: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Eval {
        dataset: PathBuf,
        #[arg(long, default_value_t = 5)]
        k: usize,
        #[arg(long)]
        mode: Option<String>,
        #[arg(long)]
        space: Option<String>,
        #[arg(long)]
        lang: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    User {
        #[command(subcommand)]
        command: UserCommand,
    },
    Token {
        #[command(subcommand)]
        command: TokenCommand,
    },
    Page {
        #[command(subcommand)]
        command: PageCommand,
    },
    Group {
        #[command(subcommand)]
        command: GroupCommand,
    },
    Space {
        #[command(subcommand)]
        command: SpaceCommand,
    },
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
    Features {
        #[arg(long)]
        json: bool,
    },
    Billing {
        #[command(subcommand)]
        command: BillingCommand,
    },
    Models {
        #[command(subcommand)]
        command: ModelsCommand,
    },
    Usage {
        #[arg(long)]
        user: Option<String>,
        #[arg(long)]
        space: Option<String>,
        #[arg(long)]
        alias: Option<String>,
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        to: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Quota {
        #[arg(long)]
        json: bool,
    },
    Summary {
        #[command(subcommand)]
        command: SummaryCommand,
    },
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
}

#[derive(Debug, Subcommand)]
enum McpCommand {
    Serve {
        #[arg(long)]
        token: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum SummaryCommand {
    Run {
        #[arg(long, default_value_t = 16)]
        limit: i64,
        #[arg(long)]
        json: bool,
    },
    List {
        #[arg(long, default_value = "queued")]
        status: String,
        #[arg(long, default_value_t = 50)]
        limit: i64,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ModelsCommand {
    List,
    Show { alias: String },
    Routes { alias: String },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Print the effective configuration and the resolved default paths.
    Check,
    /// Write a minimal solo `lkb.toml` to the resolved config path.
    Init,
}

#[derive(Debug, Subcommand)]
enum UserCommand {
    Add {
        username: String,
        #[arg(long)]
        role: Option<String>,
    },
    Role {
        #[command(subcommand)]
        command: UserRoleCommand,
    },
}

#[derive(Debug, Subcommand)]
enum UserRoleCommand {
    Set { username: String, role: String },
}

#[derive(Debug, Subcommand)]
enum BillingCommand {
    Checkout {
        #[arg(long)]
        plan: String,
        #[arg(long)]
        amount_minor: i64,
        #[arg(long)]
        currency: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum AuthCommand {
    Login {
        #[arg(long)]
        token: Option<String>,
        #[arg(long, default_value_t = false)]
        device: bool,
        #[arg(long)]
        json: bool,
    },
    Status {
        #[arg(long)]
        json: bool,
    },
    Logout {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ApiConfigFile {
    base_url: Option<String>,
    token: Option<String>,
    tenant: Option<String>,
    space: Option<String>,
}

#[derive(Debug, Subcommand)]
enum GroupCommand {
    Create {
        name: String,
        #[arg(long)]
        description: Option<String>,
    },
    List,
    AddMember {
        group_id: String,
        principal_id: String,
        #[arg(long, default_value = "group_member")]
        role: String,
    },
    RemoveMember {
        group_id: String,
        principal_id: String,
    },
}

#[derive(Debug, Subcommand)]
enum SpaceCommand {
    Create {
        #[arg(long)]
        space_id: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, default_value = "private")]
        visibility: String,
    },
    List,
    Members {
        space_id: String,
    },
    Usage {
        space_id: String,
    },
}

#[derive(Debug, Subcommand)]
enum TokenCommand {
    Create {
        username: String,
        #[arg(long, default_value = "read")]
        scope: String,
        #[arg(long, default_value = "cli")]
        label: String,
        #[arg(long, default_value = "user")]
        kind: String,
        #[arg(long)]
        expires_days: Option<i64>,
    },
    Revoke {
        token_id: String,
    },
    List {
        username: String,
    },
}

#[derive(Debug, Subcommand)]
enum PageCommand {
    List {
        #[arg(long)]
        space: Option<String>,
        #[arg(long)]
        lang: Option<String>,
    },
    Show {
        space: String,
        slug: String,
        #[arg(long)]
        lang: Option<String>,
    },
}

/// Marker for problems in configuration the user supplies. `Display`, `Debug`
/// and the source chain pass straight through, so CLI output is unchanged.
struct UserInput(anyhow::Error);

impl std::fmt::Display for UserInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl std::fmt::Debug for UserInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.0, f)
    }
}

impl std::error::Error for UserInput {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // Skip the wrapped top message, which `Display` already shows.
        self.0.chain().nth(1)
    }
}

/// Errors the operator can fix (flags, environment, config files) never get
/// the bug-report tail; anything else may deserve a GitHub issue.
fn is_user_input_error(err: &anyhow::Error) -> bool {
    if err.downcast_ref::<UserInput>().is_some() {
        return true;
    }
    // `bail!`/`anyhow!` build a message-only error with no typed cause: that
    // is CLI guidance ("username must be ..."), not a crash report.
    err.chain().count() == 1
        && (err.downcast_ref::<&str>().is_some() || err.downcast_ref::<String>().is_some())
}

pub fn run_blocking() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("cannot create tokio runtime")?;
    let result = runtime.block_on(run());
    // Drop the runtime before the exit path below so pools and blocking
    // threads still shut down exactly as they do on the success path.
    drop(runtime);
    match result {
        Ok(()) => Ok(()),
        Err(err) => {
            eprintln!("Error: {err:?}");
            if !is_user_input_error(&err) {
                eprintln!("If this is a bug, report: {ISSUES_URL}");
            }
            std::process::exit(1);
        }
    }
}

pub async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let command_name = cli_command_name(&cli.command);
    let service = if matches!(cli.command, Commands::Serve) {
        "lkb-api"
    } else {
        "lkb-cli"
    };
    crate::logging::init(service);

    let config_path = paths::resolve_config_file(cli.config.as_deref());
    // Configuration problems are the operator's to fix, not bugs; mark them
    // so the top-level exit skips the bug-report tail.
    let config = Config::load(&config_path).map_err(UserInput)?;
    crate::i18n::set_cli_locale(&config.i18n.default_locale);
    crate::logging::cli_event(command_name, 0, None, &[]);
    match cli.command {
        Commands::Serve => {
            tracing::info!(
                config_file = %config_path.display(),
                config_dir = %paths::config_dir().display(),
                data_dir = %paths::data_dir().display(),
                log_dir = %paths::log_dir().display(),
                storage_backend = config.storage.backend.as_str(),
                "resolved filesystem paths"
            );
            let store = open_store(&config).await?;
            let _ = crate::seed::auto_seed_if_empty(&store, &config).await;
            let providers = Arc::new(ProviderSet::from_config(&config.models));
            let state = crate::web::AppState::new(store, Arc::new(config.clone()), providers);
            crate::summary::spawn_worker(state.store.clone(), state.config.clone());
            crate::web::serve(state, &config.server.bind).await
        }
        Commands::Seed { space, lang, json } => {
            let store = open_store(&config).await?;
            let options = crate::seed::SeedOptions {
                space_id: space.unwrap_or_else(|| config.seed.space.clone()),
                lang: lang_arg(lang),
            };
            let report = crate::seed::run_seed(&store, &config, &options).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", report.human());
            }
            if report.failed > 0 {
                std::process::exit(2);
            }
            Ok(())
        }
        Commands::Import {
            path,
            space,
            acl_json,
            lang,
            json,
        } => {
            let store = open_store(&config).await?;
            let providers = ProviderSet::from_config(&config.models);
            let acl = match acl_json {
                Some(value) => Some(
                    crate::governance::Acl::from_user_input(&value).map_err(anyhow::Error::msg)?,
                ),
                None => None,
            };
            let options = crate::ingest::ImportOptions {
                tenant_id: config.storage.tenant_id.clone(),
                space_id: space.unwrap_or_else(|| config.storage.default_space.clone()),
                acl,
                allow_implicit_allow_all: config.profile_name() == crate::config::ProfileName::Solo,
                owner_principal_id: "cli".to_string(),
                lang: lang_arg(lang),
                ..Default::default()
            };
            let report = crate::ingest::import_path(
                &store,
                &path,
                &options,
                providers.embedding.as_deref(),
                providers.extractor.as_deref(),
            )
            .await?;
            if json {
                println!("{}", crate::ingest::report_json(&report));
            } else {
                print!("{}", report.human());
            }
            if !report.failed.is_empty() {
                std::process::exit(2);
            }
            Ok(())
        }
        Commands::Export { out, space } => {
            let store = open_store(&config).await?;
            match out {
                Some(path) => {
                    let file = std::fs::File::create(&path)
                        .with_context(|| format!("cannot create {}", path.display()))?;
                    let mut writer = std::io::BufWriter::new(file);
                    let count = crate::ingest::export_jsonl(
                        &store,
                        &config.storage.tenant_id,
                        space.as_deref(),
                        &mut writer,
                    )
                    .await?;
                    eprintln!("exported {count} document(s) to {}", path.display());
                }
                None => {
                    let stdout = std::io::stdout();
                    let mut writer = std::io::BufWriter::new(stdout.lock());
                    let count = crate::ingest::export_jsonl(
                        &store,
                        &config.storage.tenant_id,
                        space.as_deref(),
                        &mut writer,
                    )
                    .await?;
                    eprintln!("exported {count} document(s)");
                }
            }
            Ok(())
        }
        Commands::Search {
            query,
            mode,
            k,
            space,
            lang,
            as_of,
            json,
        } => {
            if query.chars().count() > config.limits.max_query_chars {
                bail!("invalid_query: query too long");
            }
            if k > config.limits.max_top_k {
                bail!("invalid_top_k: top_k too large");
            }
            let store = open_store(&config).await?;
            let providers = ProviderSet::from_config(&config.models);
            let principal = resolve_principal(&config, &store, cli.token.as_deref()).await?;
            let principal = with_space(principal, &config, space);
            let policy = Policy::new(principal, as_of.as_deref()).map_err(anyhow::Error::msg)?;
            let mode_str = mode.unwrap_or_else(|| config.retrieval.mode.clone());
            let mode = SearchMode::parse(&mode_str).map_err(anyhow::Error::msg)?;
            let mut tuning = SearchTuning::from_retrieval(&config.retrieval);
            tuning.lang = lang_arg(lang);
            tuning.billing_local_ledger = config.billing.engine.eq_ignore_ascii_case("local");
            let response = crate::search::search_with_tuning(
                &store, &providers, &policy, &query, mode, k, &tuning,
            )
            .await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!(
                    "mode: {} (used {})",
                    response.mode_requested.as_str(),
                    response.mode_used.as_str()
                );
                println!("engine: {}", response.keyword_engine);
                if !response.degraded.is_empty() {
                    println!("degraded:");
                    for item in &response.degraded {
                        println!("  - {} ({:?}): {}", item.code, item.category, item.impact);
                    }
                }
                for (index, hit) in response.hits.iter().enumerate() {
                    println!(
                        "[{}] {}:{}-{} score={:.4}\n    {}",
                        index + 1,
                        hit.path,
                        hit.line_start,
                        hit.line_end,
                        hit.score,
                        crate::util::truncate_chars(hit.quote.trim(), 240).replace('\n', " ")
                    );
                }
            }
            Ok(())
        }
        Commands::Ask {
            query,
            mode,
            limit,
            space,
            lang,
            as_of,
            json,
        } => {
            if query.chars().count() > config.limits.max_query_chars {
                bail!("invalid_query: query too long");
            }
            if limit > config.limits.max_top_k {
                bail!("invalid_top_k: top_k too large");
            }
            let store = open_store(&config).await?;
            let providers = ProviderSet::from_config(&config.models);
            let principal = resolve_principal(&config, &store, cli.token.as_deref()).await?;
            let principal = with_space(principal, &config, space);
            let policy = Policy::new(principal, as_of.as_deref()).map_err(anyhow::Error::msg)?;
            let mode_str = mode.unwrap_or_else(|| config.retrieval.mode.clone());
            let mode = SearchMode::parse(&mode_str).map_err(anyhow::Error::msg)?;
            let mut tuning = SearchTuning::from_retrieval(&config.retrieval);
            tuning.lang = lang_arg(lang);
            tuning.billing_local_ledger = config.billing.engine.eq_ignore_ascii_case("local");
            let answer = crate::answer::build_answer_with_ledger(
                &store,
                &providers,
                &policy,
                &query,
                &crate::answer::AnswerOptions {
                    mode,
                    limit,
                    tuning,
                },
                config.billing.engine.eq_ignore_ascii_case("local"),
            )
            .await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&answer)?);
            } else {
                println!("mode: {:?}", answer.mode);
                println!(
                    "confidence: {:.3} ({:?})",
                    answer.confidence.score, answer.confidence.band
                );
                match &answer.answer {
                    Some(text) => println!("\n{text}"),
                    None => println!("\n未找到可用证据。"),
                }
                if !answer.citations.is_empty() {
                    println!("\ncitations:");
                    for (index, citation) in answer.citations.iter().enumerate() {
                        println!(
                            "  [{}] {}:{} ({})",
                            index + 1,
                            citation.path,
                            citation.line_start,
                            citation.doc_id
                        );
                    }
                }
                if !answer.conflicts.is_empty() {
                    println!("\nconflicts: {}", answer.conflicts.len());
                }
                if !answer.degraded.is_empty() {
                    println!("\ndegraded:");
                    for item in &answer.degraded {
                        println!("  - {} ({:?}): {}", item.code, item.category, item.impact);
                    }
                }
            }
            Ok(())
        }
        Commands::Eval {
            dataset,
            k,
            mode,
            space,
            lang,
            json,
        } => {
            let store = open_store(&config).await?;
            let providers = ProviderSet::from_config(&config.models);
            let principal = resolve_principal(&config, &store, cli.token.as_deref()).await?;
            let principal = with_space(principal, &config, space);
            let policy = Policy::new(principal, None).map_err(anyhow::Error::msg)?;
            let mode_str = mode.unwrap_or_else(|| config.retrieval.mode.clone());
            let mode = SearchMode::parse(&mode_str).map_err(anyhow::Error::msg)?;
            let mut tuning = SearchTuning::from_retrieval(&config.retrieval);
            tuning.lang = lang_arg(lang);
            tuning.billing_local_ledger = config.billing.engine.eq_ignore_ascii_case("local");
            let report =
                crate::eval::evaluate(&store, &providers, &policy, &dataset, k, mode, &tuning)
                    .await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("dataset: {}", report.dataset);
                println!("queries: {}", report.queries);
                println!("Recall@{}: {:.4}", report.k, report.recall_at_k);
                println!("MRR: {:.4}", report.mrr);
                for item in &report.per_query {
                    println!(
                        "  - {:?} recall={} rr={:.4}",
                        item.query, item.recall, item.reciprocal_rank
                    );
                }
            }
            Ok(())
        }
        Commands::Config { command } => match command {
            ConfigCommand::Check => {
                print!("{}", paths::describe(&config_path));
                print!("{}", config.describe());
                Ok(())
            }
            ConfigCommand::Init => init_config(&config_path),
        },
        Commands::User {
            command: UserCommand::Add { username, role },
        } => {
            if username.trim().is_empty() || username.contains('/') || username.contains('\\') {
                bail!("username must be a non-empty single token");
            }
            let role = role
                .as_deref()
                .map(crate::governance::UserRole::parse)
                .transpose()
                .map_err(anyhow::Error::msg)?
                .unwrap_or(crate::governance::UserRole::User);
            let store = open_store(&config).await?;
            let password = read_password()?;
            if password.trim().is_empty() {
                bail!("password must not be empty");
            }
            let salt = SaltString::generate(&mut OsRng);
            let hash = Argon2::default()
                .hash_password(password.as_bytes(), &salt)
                .map_err(|err| anyhow::anyhow!("cannot hash password: {err}"))?
                .to_string();
            let user = store
                .add_user_with_role(&config.storage.tenant_id, &username, &hash, role)
                .await?;
            println!(
                "user added: {} ({}) role={}",
                user.username,
                user.user_id,
                user.role.as_str()
            );
            Ok(())
        }
        Commands::User {
            command:
                UserCommand::Role {
                    command: UserRoleCommand::Set { username, role },
                },
        } => {
            let role = crate::governance::UserRole::parse(&role).map_err(anyhow::Error::msg)?;
            let store = open_store(&config).await?;
            if role != crate::governance::UserRole::Owner {
                let users = store.list_users(&config.storage.tenant_id).await?;
                let owners = users
                    .iter()
                    .filter(|user| user.role == crate::governance::UserRole::Owner)
                    .count();
                let target = users.iter().find(|user| user.username == username);
                if owners <= 1
                    && target
                        .map(|user| user.role == crate::governance::UserRole::Owner)
                        .unwrap_or(false)
                {
                    bail!("cannot demote the last tenant owner");
                }
            }
            if !store
                .set_user_role(&config.storage.tenant_id, &username, role)
                .await?
            {
                bail!("user not found: {username}");
            }
            println!("user role updated: {username} role={}", role.as_str());
            Ok(())
        }
        Commands::Token {
            command:
                TokenCommand::Create {
                    username,
                    scope,
                    label,
                    kind,
                    expires_days,
                },
        } => {
            let store = open_store(&config).await?;
            let user = store
                .get_user(&config.storage.tenant_id, &username)
                .await?
                .ok_or_else(|| anyhow::anyhow!("user not found: {username}"))?;
            let scope = TokenScope::parse(&scope).map_err(anyhow::Error::msg)?;
            let kind = match kind.trim().to_ascii_lowercase().as_str() {
                "user" | "human" => PrincipalKind::Human,
                "agent" => PrincipalKind::Agent,
                "service" => PrincipalKind::Service,
                other => bail!("unknown token kind {other:?}; expected user or agent"),
            };
            let raw = format!("lkb_{}", random_hex(32));
            let token_id = format!("tok_{}", random_hex(8));
            let now = chrono::Utc::now();
            let expires_at = expires_days.map(|days| {
                (now + chrono::Duration::days(days))
                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
            });
            let record = TokenRecord {
                token_hash: sha256_hex_str(&raw),
                token_id: token_id.clone(),
                tenant_id: config.storage.tenant_id.clone(),
                user_id: user.user_id.clone(),
                scope,
                kind,
                label,
                created_at: now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                expires_at,
                revoked_at: None,
            };
            store.create_token(&record).await?;
            println!("token_id: {token_id}");
            println!("scope: {}", scope.as_str());
            println!("token (shown once): {raw}");
            Ok(())
        }
        Commands::Token {
            command: TokenCommand::Revoke { token_id },
        } => {
            let store = open_store(&config).await?;
            let revoked = store.revoke_token(&token_id).await?;
            if revoked {
                println!("token revoked: {token_id}");
            } else {
                println!("token not found or already revoked: {token_id}");
            }
            Ok(())
        }
        Commands::Token {
            command: TokenCommand::List { username },
        } => {
            let store = open_store(&config).await?;
            let user = store
                .get_user(&config.storage.tenant_id, &username)
                .await?
                .ok_or_else(|| anyhow::anyhow!("user not found: {username}"))?;
            let tokens = store
                .list_tokens(&config.storage.tenant_id, &user.user_id)
                .await?;
            for token in tokens {
                println!(
                    "{} scope={} label={} created={} expires={:?} revoked={:?}",
                    token.token_id,
                    token.scope.as_str(),
                    token.label,
                    token.created_at,
                    token.expires_at,
                    token.revoked_at
                );
            }
            Ok(())
        }
        Commands::Page {
            command: PageCommand::List { space, lang },
        } => {
            let store = open_store(&config).await?;
            let principal = resolve_principal(&config, &store, cli.token.as_deref()).await?;
            let principal = with_space(principal, &config, space);
            let policy = Policy::new(principal, None)
                .map_err(anyhow::Error::msg)?
                .with_lang(lang_arg(lang));
            let pages = store.list_pages(&policy, 5000).await?;
            for page in pages {
                println!(
                    "{}/{}/{}  v{}  {}  {}",
                    page.space_id, page.lang, page.slug, page.version, page.title, page.updated_at
                );
            }
            Ok(())
        }
        Commands::Page {
            command: PageCommand::Show { space, slug, lang },
        } => {
            let store = open_store(&config).await?;
            let principal = resolve_principal(&config, &store, cli.token.as_deref()).await?;
            let lang = lang_arg(lang);
            let policy = Policy::new(principal, None)
                .map_err(anyhow::Error::msg)?
                .with_lang(lang.clone());
            let view = store
                .get_page(&policy, &space, &slug, lang.as_deref())
                .await?
                .ok_or_else(|| anyhow::anyhow!("page not found: {space}/{slug}"))?;
            println!("title: {}", view.page.title);
            println!("space: {}", view.page.space_id);
            println!("lang: {}", view.page.lang);
            println!("slug: {}", view.page.slug);
            println!("version: {}", view.page.version);
            println!("author: {}", view.revision.author);
            println!("created_at: {}", view.revision.created_at);
            println!("content_hash: {}", view.page.content_hash);
            if let Some(source_url) = &view.revision.source_url {
                println!("source_url: {source_url}");
            }
            if let Some(source_revision) = &view.revision.source_revision {
                println!("source_revision: {source_revision}");
            }
            println!("\n{}", view.revision.markdown);
            Ok(())
        }

        Commands::Auth {
            command:
                AuthCommand::Login {
                    token,
                    device,
                    json,
                },
        } => {
            if device && token.is_none() {
                bail!(
                    "not_implemented: {}",
                    crate::i18n::t(
                        &crate::i18n::current_locale(),
                        "cli.auth.device_not_implemented"
                    )
                );
            }
            let token = token
                .or_else(|| std::env::var("LKB_TOKEN").ok())
                .or_else(|| std::env::var("LKB_API_TOKEN").ok())
                .filter(|value| !value.trim().is_empty());
            if let Some(token) = token {
                if !token.starts_with("lkb_") {
                    bail!("invalid token: expected lkb_ prefix");
                }
                let mut api = load_api_config()?;
                api.token = Some(token.clone());
                save_api_config(&api)?;
                let prefix = &crate::util::sha256_hex(token.as_bytes())[..16];
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "status": "logged_in",
                            "token_prefix": prefix,
                        }))?
                    );
                } else {
                    println!(
                        "{}; token prefix {prefix}",
                        crate::i18n::t(&crate::i18n::current_locale(), "cli.auth.logged_in")
                    );
                }
            } else {
                bail!("usage: lkb auth login --token <token>");
            }
            Ok(())
        }
        Commands::Auth {
            command: AuthCommand::Status { json },
        } => {
            let api = load_api_config()?;
            let env_token = std::env::var("LKB_TOKEN")
                .ok()
                .or_else(|| std::env::var("LKB_API_TOKEN").ok())
                .filter(|value| !value.is_empty());
            let effective = env_token.or_else(|| api.token.clone());
            let prefix = effective
                .as_deref()
                .map(|token| crate::util::sha256_hex(token.as_bytes())[..16].to_string());
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "base_url": api.base_url,
                        "tenant": api.tenant,
                        "space": api.space,
                        "token_configured": effective.is_some(),
                        "token_prefix": prefix,
                        "source": if std::env::var("LKB_TOKEN").is_ok() || std::env::var("LKB_API_TOKEN").is_ok() {"env"} else if api.token.is_some() {"config"} else {"none"},
                    }))?
                );
            } else {
                println!("base_url: {}", api.base_url.unwrap_or_default());
                println!("tenant: {}", api.tenant.unwrap_or_default());
                println!("space: {}", api.space.unwrap_or_default());
                println!("token_configured: {}", effective.is_some());
                println!("token_prefix: {}", prefix.unwrap_or_default());
            }
            Ok(())
        }
        Commands::Auth {
            command: AuthCommand::Logout { json },
        } => {
            let mut api = load_api_config()?;
            api.token = None;
            save_api_config(&api)?;
            if json {
                println!("{}", serde_json::json!({"status": "logged_out"}));
            } else {
                println!(
                    "{}",
                    crate::i18n::t(&crate::i18n::current_locale(), "cli.auth.logged_out")
                );
            }
            Ok(())
        }
        Commands::Mcp { command } => {
            let store = open_store(&config).await?;
            let config = Arc::new(config);
            match command {
                McpCommand::Serve { token } => {
                    let token = token.or_else(|| cli.token.clone());
                    crate::mcp::serve(store, config, token).await?;
                }
            }
            Ok(())
        }
        Commands::Summary { command } => {
            let store = open_store(&config).await?;
            let principal = resolve_principal(&config, &store, cli.token.as_deref()).await?;
            if !principal.tenant_role.is_admin() {
                bail!("tenant admin required");
            }
            match command {
                SummaryCommand::Run { limit, json } => {
                    let before = store.list_summary_jobs("queued", limit).await?;
                    let processed = crate::summary::process_pending(&store, &config, limit).await;
                    let after = store.list_summary_jobs("queued", limit).await?;
                    if json {
                        println!(
                            "{}",
                            serde_json::json!({"queued_before": before.len(), "processed": processed, "queued_after": after.len()})
                        );
                    } else {
                        println!(
                            "queued_before={} processed={} queued_after={}",
                            before.len(),
                            processed,
                            after.len()
                        );
                    }
                }
                SummaryCommand::List {
                    status,
                    limit,
                    json,
                } => {
                    let jobs = store.list_summary_jobs(&status, limit).await?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&jobs)?);
                    } else {
                        for job in jobs {
                            println!(
                                "{} doc={} revision={} status={} attempts={} error={:?}",
                                job.job_id,
                                job.doc_id,
                                job.revision_id,
                                job.status,
                                job.attempts,
                                job.error
                            );
                        }
                    }
                }
            }
            Ok(())
        }
        Commands::Models { command } => {
            let store = open_store(&config).await?;
            let principal = resolve_principal(&config, &store, cli.token.as_deref()).await?;
            if !principal.tenant_role.is_admin() {
                bail!("tenant admin required");
            }
            match command {
                ModelsCommand::List => {
                    for alias in store.list_model_aliases(&config.storage.tenant_id).await? {
                        println!(
                            "{}  {}  status={}  version={}",
                            alias.public_id, alias.display_name, alias.status, alias.version
                        );
                    }
                }
                ModelsCommand::Show { alias } => {
                    let alias = store
                        .get_model_alias(&config.storage.tenant_id, &alias)
                        .await?
                        .ok_or_else(|| anyhow::anyhow!("model alias not found"))?;
                    println!("{}", serde_json::to_string_pretty(&alias)?);
                }
                ModelsCommand::Routes { alias } => {
                    for route in store
                        .list_routes(&config.storage.tenant_id, Some(&alias))
                        .await?
                    {
                        println!(
                            "{}  model={}  priority={} weight={} health={} circuit={}",
                            route.route_id,
                            route.provider_model_id,
                            route.priority,
                            route.weight,
                            route.health,
                            route.circuit_state
                        );
                    }
                }
            }
            Ok(())
        }
        Commands::Usage {
            user,
            space,
            alias,
            from,
            to,
            json,
        } => {
            let store = open_store(&config).await?;
            let principal = resolve_principal(&config, &store, cli.token.as_deref()).await?;
            if !principal.tenant_role.is_admin() {
                bail!("tenant admin required");
            }
            let events = store
                .list_usage_events(&crate::store::UsageFilter {
                    tenant_id: config.storage.tenant_id.clone(),
                    space_id: space,
                    user_id: user,
                    alias,
                    from,
                    to,
                    limit: 1000,
                })
                .await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&events)?);
            } else {
                let mut input = 0i64;
                let mut output = 0i64;
                let mut cost = 0i64;
                for event in &events {
                    input += event.input_tokens;
                    output += event.output_tokens;
                    cost += event.cost_micros;
                }
                println!(
                    "requests={} input_tokens={} output_tokens={} cost_micros={}",
                    events.len(),
                    input,
                    output,
                    cost
                );
                for event in events {
                    println!(
                        "{} alias={} input={} output={} cost={} status={}",
                        event.request_id,
                        event.alias,
                        event.input_tokens,
                        event.output_tokens,
                        event.cost_micros,
                        event.status
                    );
                }
            }
            Ok(())
        }
        Commands::Quota { json } => {
            let store = open_store(&config).await?;
            let principal = resolve_principal(&config, &store, cli.token.as_deref()).await?;
            if !principal.tenant_role.is_admin() {
                bail!("tenant admin required");
            }
            let quotas = store.list_token_quotas(&config.storage.tenant_id).await?;
            let counters = store.list_quota_counters(&config.storage.tenant_id).await?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &serde_json::json!({"quotas": quotas, "counters": counters})
                    )?
                );
            } else {
                for quota in quotas {
                    println!("{} {}:{} period={} tokens={:?} requests={:?} cost={:?} enforcement={} enabled={}", quota.quota_id, quota.scope_type, quota.scope_id, quota.period, quota.limit_tokens, quota.limit_requests, quota.limit_cost_micros, quota.enforcement, quota.enabled);
                }
                for counter in counters {
                    println!(
                        "counter {}:{} period={} used_tokens={} reserved_tokens={}",
                        counter.scope_type,
                        counter.scope_id,
                        counter.period_start,
                        counter.used_tokens,
                        counter.reserved_tokens
                    );
                }
            }
            Ok(())
        }
        Commands::Features { json } => {
            let features = config.features();
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "mode": config.deployment_mode().as_str(),
                        "features": features.to_json(),
                    }))?
                );
            } else {
                println!("mode: {}", config.deployment_mode().as_str());
                println!("{}", serde_json::to_string_pretty(&features.to_json())?);
            }
            Ok(())
        }
        Commands::Billing {
            command:
                BillingCommand::Checkout {
                    plan,
                    amount_minor,
                    currency,
                },
        } => {
            if !config.feature_enabled(crate::config::Feature::Billing) {
                bail!(
                    "feature_disabled: {}",
                    crate::i18n::t(&crate::i18n::current_locale(), "cli.billing.disabled")
                );
            }
            if plan.trim().is_empty() || amount_minor <= 0 {
                bail!("invalid_checkout: plan and amount_minor are required");
            }
            let _currency = currency.unwrap_or_else(|| config.payment.currency.clone());
            if !crate::payment::is_configured(&config.payment) {
                bail!(
                    "{}",
                    crate::i18n::t(&crate::i18n::current_locale(), "payment.not_configured")
                );
            }
            bail!(
                "payment_provider_unavailable: payment adapter is not implemented in this release"
            );
        }
        Commands::Group {
            command: GroupCommand::Create { name, description },
        } => {
            let store = open_store(&config).await?;
            let principal = resolve_principal(&config, &store, cli.token.as_deref()).await?;
            if !principal.tenant_role.is_admin() && !config.groups.allow_user_creation {
                bail!("self-service group creation is disabled");
            }
            if name.trim().is_empty() {
                bail!("group name must not be empty");
            }
            let now = now_rfc3339();
            let group = GroupRecord {
                group_id: format!("grp_{}", random_hex(8)),
                tenant_id: config.storage.tenant_id.clone(),
                name,
                description: description.unwrap_or_default(),
                created_by: principal.principal_id.clone(),
                status: "active".to_string(),
                created_at: now.clone(),
                updated_at: now.clone(),
            };
            store.create_group(&group).await?;
            store
                .add_group_member(&GroupMemberRecord {
                    group_id: group.group_id.clone(),
                    principal_id: principal.principal_id.clone(),
                    role: GroupRole::GroupAdmin,
                    added_by: principal.principal_id.clone(),
                    created_at: now,
                })
                .await?;
            println!("group created: {} ({})", group.name, group.group_id);
            Ok(())
        }
        Commands::Group {
            command: GroupCommand::List,
        } => {
            let store = open_store(&config).await?;
            let principal = resolve_principal(&config, &store, cli.token.as_deref()).await?;
            let groups = if principal.tenant_role.is_admin() {
                store.list_groups(&config.storage.tenant_id).await?
            } else {
                store
                    .list_group_memberships(&config.storage.tenant_id, &principal.principal_id)
                    .await?
                    .into_iter()
                    .map(|membership| membership.group)
                    .collect()
            };
            for group in groups {
                println!(
                    "{}  {}  status={}  desc={}",
                    group.group_id, group.name, group.status, group.description
                );
            }
            Ok(())
        }
        Commands::Group {
            command:
                GroupCommand::AddMember {
                    group_id,
                    principal_id,
                    role,
                },
        } => {
            let store = open_store(&config).await?;
            let principal = resolve_principal(&config, &store, cli.token.as_deref()).await?;
            let group = store
                .get_group(&config.storage.tenant_id, &group_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("group not found: {group_id}"))?;
            if !principal.tenant_role.is_admin() {
                let member = store
                    .get_group_member(&group.group_id, &principal.principal_id)
                    .await?;
                if !member
                    .map(|member| member.role == GroupRole::GroupAdmin)
                    .unwrap_or(false)
                {
                    bail!("group_admin required");
                }
            }
            if store
                .get_user_by_id(&config.storage.tenant_id, &principal_id)
                .await?
                .is_none()
            {
                bail!("user not found: {principal_id}");
            }
            let role = GroupRole::parse(&role).map_err(anyhow::Error::msg)?;
            store
                .add_group_member(&GroupMemberRecord {
                    group_id: group_id.clone(),
                    principal_id: principal_id.clone(),
                    role,
                    added_by: principal.principal_id.clone(),
                    created_at: now_rfc3339(),
                })
                .await?;
            println!(
                "group member added: {group_id} -> {principal_id} role={}",
                role.as_str()
            );
            Ok(())
        }
        Commands::Group {
            command:
                GroupCommand::RemoveMember {
                    group_id,
                    principal_id,
                },
        } => {
            let store = open_store(&config).await?;
            let principal = resolve_principal(&config, &store, cli.token.as_deref()).await?;
            if !principal.tenant_role.is_admin() {
                let member = store
                    .get_group_member(&group_id, &principal.principal_id)
                    .await?;
                if !member
                    .map(|member| member.role == GroupRole::GroupAdmin)
                    .unwrap_or(false)
                {
                    bail!("group_admin required");
                }
            }
            if let Some(member) = store.get_group_member(&group_id, &principal_id).await? {
                if member.role == GroupRole::GroupAdmin
                    && store.count_group_admins(&group_id).await? <= 1
                {
                    bail!("cannot remove the last group_admin");
                }
            }
            store.remove_group_member(&group_id, &principal_id).await?;
            println!("group member removed: {group_id} -> {principal_id}");
            Ok(())
        }
        Commands::Space {
            command:
                SpaceCommand::Create {
                    space_id,
                    name,
                    visibility,
                },
        } => {
            let store = open_store(&config).await?;
            let principal = resolve_principal(&config, &store, cli.token.as_deref()).await?;
            if principal.kind == PrincipalKind::Agent {
                bail!("agents cannot create spaces");
            }
            if !principal.tenant_role.is_admin() && !config.spaces.allow_user_creation {
                bail!("self-service space creation is disabled");
            }
            if visibility != "private" && visibility != "internal" {
                bail!("visibility must be private or internal");
            }
            let space_id = space_id.unwrap_or_else(|| format!("sp_{}", random_hex(8)));
            crate::quota::check_space_creation(
                &store,
                &config.storage.tenant_id,
                "user",
                &principal.principal_id,
                config.spaces.max_per_user,
            )
            .await
            .map_err(|err| anyhow::anyhow!("{}: {}", err.code, err.message))?;
            let now = now_rfc3339();
            let mut space = SpaceRecord::new(
                &config.storage.tenant_id,
                &space_id,
                name.as_deref().unwrap_or(&space_id),
                &Acl::allow_all(),
                &principal.principal_id,
                &now,
            );
            space.visibility = visibility;
            store.ensure_space(&space).await?;
            store
                .upsert_space_member(&SpaceMemberRecord {
                    space_id: space_id.clone(),
                    principal_type: PrincipalType::User,
                    principal_id: principal.principal_id.clone(),
                    role: SpaceRole::SpaceOwner,
                    added_by: principal.principal_id.clone(),
                    created_at: now.clone(),
                })
                .await?;
            crate::quota::apply_default_quota(
                &store,
                &config.storage.tenant_id,
                &space_id,
                &principal.principal_id,
                &config.spaces,
            )
            .await?;
            println!(
                "space created: {} ({}) owner={}",
                space.name, space_id, principal.principal_id
            );
            Ok(())
        }
        Commands::Space {
            command: SpaceCommand::List,
        } => {
            let store = open_store(&config).await?;
            let principal = resolve_principal(&config, &store, cli.token.as_deref()).await?;
            for space in store.list_spaces(&config.storage.tenant_id).await? {
                if space.status == "deleted" {
                    continue;
                }
                let access =
                    crate::access::effective_space_role(&store, &principal, &space).await?;
                if access.is_some() || space.visibility == "internal" {
                    println!(
                        "{}  {}  status={}  role={}",
                        space.space_id,
                        space.name,
                        space.status,
                        access.map(|value| value.role.as_str()).unwrap_or("-")
                    );
                }
            }
            Ok(())
        }
        Commands::Space {
            command: SpaceCommand::Members { space_id },
        } => {
            let store = open_store(&config).await?;
            let principal = resolve_principal(&config, &store, cli.token.as_deref()).await?;
            let space = store
                .get_space(&config.storage.tenant_id, &space_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("space not found: {space_id}"))?;
            let access = crate::access::effective_space_role(&store, &principal, &space).await?;
            if !access
                .map(|value| value.role >= SpaceRole::SpaceAdmin)
                .unwrap_or(false)
            {
                bail!("space_admin required");
            }
            for member in store.list_space_members(&space_id).await? {
                println!(
                    "{}:{}  role={}  added_by={}",
                    member.principal_type.as_str(),
                    member.principal_id,
                    member.role.as_str(),
                    member.added_by
                );
            }
            Ok(())
        }
        Commands::Space {
            command: SpaceCommand::Usage { space_id },
        } => {
            let store = open_store(&config).await?;
            let principal = resolve_principal(&config, &store, cli.token.as_deref()).await?;
            let space = store
                .get_space(&config.storage.tenant_id, &space_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("space not found: {space_id}"))?;
            let access = crate::access::effective_space_role(&store, &principal, &space).await?;
            if !access
                .map(|value| value.role >= SpaceRole::Viewer)
                .unwrap_or(false)
            {
                bail!("space membership required");
            }
            let usage = store
                .get_storage_usage(&config.storage.tenant_id, &space_id)
                .await?
                .unwrap_or_else(|| {
                    crate::store::StorageUsage::zero(
                        &config.storage.tenant_id,
                        &space_id,
                        &now_rfc3339(),
                    )
                });
            println!(
                "space={} source={} index={} history={} total={} reserved={} documents={} chunks={} members={}",
                usage.space_id,
                usage.source_bytes,
                usage.index_bytes,
                usage.history_bytes,
                usage.total_bytes,
                usage.reserved_bytes,
                usage.documents,
                usage.chunks,
                usage.members
            );
            for quota in store.list_quotas(&config.storage.tenant_id).await? {
                if quota.scope_type == QuotaScope::Space && quota.scope_id == space_id {
                    println!(
                        "quota storage={:?} documents={:?} chunks={:?} members={:?} enforcement={}",
                        quota.storage_bytes,
                        quota.documents,
                        quota.chunks,
                        quota.members,
                        quota.enforcement
                    );
                }
            }
            Ok(())
        }
    }
}

async fn open_store(config: &Config) -> anyhow::Result<Arc<dyn Store>> {
    let store = store::open(config).await?;
    store::bootstrap_space(&store, config).await?;
    Ok(store)
}

fn with_space(mut principal: Principal, config: &Config, space: Option<String>) -> Principal {
    if let Some(space) = space {
        principal.allowed_space_ids = vec![space];
    } else if principal.allowed_space_ids.is_empty() {
        principal.allowed_space_ids = vec![config.storage.default_space.clone()];
    }
    principal
}

fn lang_arg(lang: Option<String>) -> Option<String> {
    lang.map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
}

pub fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .unwrap_or_default()
}

pub(crate) async fn resolve_principal(
    config: &Config,
    store: &Arc<dyn Store>,
    token: Option<&str>,
) -> anyhow::Result<Principal> {
    let token = token
        .map(|value| value.to_string())
        .or_else(|| std::env::var("LKB_TOKEN").ok())
        .or_else(|| std::env::var("LKB_API_TOKEN").ok())
        .or_else(|| load_api_config().ok().and_then(|config| config.token))
        .filter(|value| !value.trim().is_empty());

    if let Some(raw) = token {
        let record = store
            .get_token(&sha256_hex_str(&raw))
            .await?
            .ok_or_else(|| anyhow::anyhow!("unknown API token"))?;
        let now = now_rfc3339();
        if !record.is_active(&now) {
            bail!("API token is expired or revoked");
        }
        if record.tenant_id != config.storage.tenant_id {
            bail!("API token belongs to another tenant");
        }
        let user = store
            .get_user_by_id(&record.tenant_id, &record.user_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("token user no longer exists"))?;
        if user.disabled {
            bail!("user is disabled");
        }
        let mut principal = Principal {
            principal_id: user.user_id,
            display_name: user.display_name,
            tenant_id: record.tenant_id,
            allowed_space_ids: Vec::new(),
            groups: Vec::new(),
            allow_all: false,
            kind: record.kind,
            scope: record.scope,
            tenant_role: user.role,
            token_id: Some(record.token_id.clone()),
        };
        principal.allowed_space_ids = crate::access::allowed_space_ids(store, &principal).await?;
        if principal.allowed_space_ids.is_empty() {
            principal
                .allowed_space_ids
                .push(config.storage.default_space.clone());
        }
        return Ok(principal);
    }

    if config.profile_name() == crate::config::ProfileName::Solo
        || config.auth.mode == crate::config::AuthMode::None
    {
        let mut principal = Principal::solo();
        principal.tenant_id = config.storage.tenant_id.clone();
        principal.allowed_space_ids = crate::access::allowed_space_ids(store, &principal).await?;
        if principal.allowed_space_ids.is_empty() {
            principal
                .allowed_space_ids
                .push(config.storage.default_space.clone());
        }
        return Ok(principal);
    }

    bail!(
        "profile {:?} requires --token or LKB_API_TOKEN for CLI queries",
        config.profile_name()
    )
}

fn read_password() -> anyhow::Result<String> {
    if let Ok(value) = std::env::var("LKB_USER_PASSWORD") {
        if !value.is_empty() {
            return Ok(value);
        }
    }
    eprint!("Password (input is echoed; prefer LKB_USER_PASSWORD): ");
    std::io::stderr().flush()?;
    let mut buffer = String::new();
    std::io::stdin().read_line(&mut buffer)?;
    Ok(buffer.trim_end_matches(['\r', '\n']).to_string())
}

#[allow(dead_code)]
fn print_user(user: &UserRecord) {
    println!("{} {} ({})", user.user_id, user.username, user.tenant_id);
}

#[allow(dead_code)]
fn store_error_message(err: StoreError) -> String {
    err.to_string()
}

fn cli_command_name(command: &Commands) -> &'static str {
    match command {
        Commands::Serve => "serve",
        Commands::Seed { .. } => "seed",
        Commands::Import { .. } => "import",
        Commands::Export { .. } => "export",
        Commands::Search { .. } => "search",
        Commands::Ask { .. } => "ask",
        Commands::Eval { .. } => "eval",
        Commands::Config { .. } => "config",
        Commands::User { .. } => "user",
        Commands::Token { .. } => "token",
        Commands::Page { .. } => "page",
        Commands::Group { .. } => "group",
        Commands::Space { .. } => "space",
        Commands::Auth { .. } => "auth",
        Commands::Features { .. } => "features",
        Commands::Billing { .. } => "billing",
        Commands::Models { .. } => "models",
        Commands::Usage { .. } => "usage",
        Commands::Quota { .. } => "quota",
        Commands::Summary { .. } => "summary",
        Commands::Mcp { .. } => "mcp",
    }
}

/// Minimal solo config written by `lkb config init`; serialized through
/// `toml` so Windows paths are escaped correctly.
#[derive(Serialize)]
struct InitConfig<'a> {
    profile: &'a str,
    storage: InitStorage<'a>,
}

#[derive(Serialize)]
struct InitStorage<'a> {
    backend: &'a str,
    sqlite_path: String,
    url_env: &'a str,
    tenant_id: &'a str,
    default_space: &'a str,
}

fn init_config_text() -> anyhow::Result<String> {
    let init = InitConfig {
        profile: "solo",
        storage: InitStorage {
            backend: "sqlite",
            sqlite_path: paths::default_sqlite_path().display().to_string(),
            url_env: "LKB_DATABASE_URL",
            tenant_id: crate::governance::SOLO_TENANT,
            default_space: crate::governance::SOLO_SPACE,
        },
    };
    let body = toml::to_string(&init)?;
    Ok(format!(
        "# lkb configuration - generated by `lkb config init`.\n\
         # Delete a line to fall back to the platform default.\n\
         \n{body}"
    ))
}

/// `lkb config init`: create the config at its resolved path unless it
/// already exists (never overwrites user edits), then print the resolved paths.
fn init_config(path: &Path) -> anyhow::Result<()> {
    if path.is_file() {
        println!("config already exists: {} (left unchanged)", path.display());
    } else {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!("cannot create config directory {}", parent.display())
                })?;
            }
        }
        std::fs::write(path, init_config_text()?)
            .with_context(|| format!("cannot write config {}", path.display()))?;
        println!("config initialized: {}", path.display());
    }
    print!("{}", paths::describe(path));
    Ok(())
}

/// CLI auth credentials (`lkb auth login`) live at `<config_dir>/config.toml`;
/// `LKB_CONFIG_FILE` selects the main config file, not this file.
fn api_config_path() -> PathBuf {
    paths::auth_config_file()
}

fn load_api_config() -> anyhow::Result<ApiConfigFile> {
    let path = api_config_path();
    if !path.exists() {
        return Ok(ApiConfigFile::default());
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(toml::from_str(&text).unwrap_or_default())
}

fn save_api_config(config: &ApiConfigFile) -> anyhow::Result<()> {
    let path = api_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = toml::to_string(config)?;
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&path)?;
        file.write_all(text.as_bytes())?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&path, text)?;
        if cfg!(windows) {
            let user = std::env::var("USERNAME").unwrap_or_else(|_| "%USERNAME%".to_string());
            let _ = std::process::Command::new("icacls")
                .arg(&path)
                .arg("/inheritance:r")
                .arg("/grant:r")
                .arg(format!("{user}:F"))
                .output();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_scope_round_trips() {
        assert_eq!(TokenScope::parse("admin").unwrap().as_str(), "admin");
        assert!(TokenScope::parse("root").is_err());
    }

    #[test]
    fn bug_report_tail_is_reserved_for_suspected_bugs() {
        // CLI guidance and config problems: no GitHub pointer.
        assert!(is_user_input_error(&anyhow::anyhow!(
            "username must be a non-empty single token"
        )));
        assert!(is_user_input_error(&anyhow::anyhow!(
            "not found: {}",
            "alice"
        )));
        assert!(is_user_input_error(&anyhow::Error::new(UserInput(
            anyhow::anyhow!("profile team/enterprise requires storage.backend = \"postgres\"")
        ))));
        // Typed runtime failures: pointer shown.
        assert!(!is_user_input_error(&anyhow::Error::new(
            std::io::Error::other("connection refused")
        )));
        assert!(!is_user_input_error(
            &anyhow::Error::new(std::io::Error::other("io")).context("cannot open store")
        ));
    }
}
