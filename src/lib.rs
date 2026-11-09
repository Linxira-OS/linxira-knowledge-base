//! Layered Knowledge Base (LKB).
//!
//! One kernel, three profiles. Profile selection lives in configuration and
//! adapter wiring only; domain code paths do not branch on the profile name.

pub mod access;
pub mod answer;
pub mod audit;
pub mod billing;
pub mod cli;
pub mod config;
pub mod connectors;
pub mod contract;
pub mod conversation;
pub mod error;
pub mod eval;
pub mod event;
pub mod frontmatter;
pub mod gateway;
pub mod governance;
pub mod i18n;
pub mod ingest;
pub mod limits;
pub mod logging;
pub mod mcp;
pub mod metrics;
pub mod openapi;
pub mod payment;
pub mod providers;
pub mod quota;
pub mod search;
pub mod seed;
pub mod store;
pub mod summary;
pub mod token_quota;
pub mod util;
pub mod web;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
