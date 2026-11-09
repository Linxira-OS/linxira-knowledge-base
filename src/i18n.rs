//! Locale resolution, stable message keys and small ICU-style formatters.
//!
//! Message catalogs are intentionally code-resident for the initial zh-CN/en
//! release but keyed so they can move to external files later. The lookup
//! ceiling is 28 locales, matching the design reservation.

use chrono::{DateTime, Utc};
use std::future::Future;
use std::sync::OnceLock;
use tokio::task_local;

pub const MAX_SUPPORTED_LOCALES: usize = 28;
pub const DEFAULT_LOCALE: &str = "en";
static CLI_LOCALE: OnceLock<String> = OnceLock::new();

task_local! {
    static CURRENT_LOCALE: String;
}

pub fn set_cli_locale(locale: &str) {
    let _ = CLI_LOCALE.set(normalize_tag(locale).unwrap_or(DEFAULT_LOCALE).to_string());
}

pub fn current_locale() -> String {
    CURRENT_LOCALE
        .try_with(|value| value.clone())
        .ok()
        .or_else(|| CLI_LOCALE.get().cloned())
        .unwrap_or_else(|| DEFAULT_LOCALE.to_string())
}

pub async fn with_locale<F>(locale: String, future: F) -> F::Output
where
    F: Future,
{
    CURRENT_LOCALE.scope(locale, future).await
}

pub fn normalize_tag(tag: &str) -> Option<&'static str> {
    let tag = tag.trim();
    if tag.eq_ignore_ascii_case("zh")
        || tag.to_ascii_lowercase().starts_with("zh-")
        || tag.eq_ignore_ascii_case("zh_cn")
    {
        Some("zh-CN")
    } else if tag.eq_ignore_ascii_case("en") || tag.to_ascii_lowercase().starts_with("en-") {
        Some("en")
    } else {
        None
    }
}

fn locale_is_supported(locale: &str, supported: &[String]) -> bool {
    supported
        .iter()
        .any(|value| normalize_tag(value) == normalize_tag(locale))
}

/// Parse `Accept-Language` in q-order, returning recognized locales.
pub fn parse_accept_language(value: &str) -> Vec<String> {
    let mut entries: Vec<(String, f32)> = Vec::new();
    for part in value.split(',') {
        let mut pieces = part.trim().split(';');
        let tag = pieces.next().unwrap_or("").trim();
        if tag.is_empty() || tag == "*" {
            continue;
        }
        let q = pieces
            .find_map(|piece| piece.trim().strip_prefix("q="))
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(1.0);
        if let Some(locale) = normalize_tag(tag) {
            entries.push((locale.to_string(), q));
        }
    }
    entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    entries.into_iter().map(|(locale, _)| locale).collect()
}

/// Resolution order: request header → user preference → tenant default →
/// configured default → en. Only locales in `supported` are eligible.
pub fn resolve(
    accept_language: Option<&str>,
    user_preference: Option<&str>,
    tenant_default: Option<&str>,
    config_default: &str,
    supported: &[String],
) -> String {
    if let Some(header) = accept_language {
        for candidate in parse_accept_language(header) {
            if locale_is_supported(&candidate, supported) {
                return candidate;
            }
        }
    }
    for candidate in [user_preference, tenant_default, Some(config_default)]
        .into_iter()
        .flatten()
    {
        if let Some(normalized) = normalize_tag(candidate) {
            if locale_is_supported(normalized, supported) {
                return normalized.to_string();
            }
        }
    }
    if locale_is_supported(DEFAULT_LOCALE, supported) {
        DEFAULT_LOCALE.to_string()
    } else {
        config_default.to_string()
    }
}

fn lookup(locale: &str, key: &str) -> Option<&'static str> {
    let zh = normalize_tag(locale) == Some("zh-CN");
    Some(match key {
        "error.unauthorized" => {
            if zh {
                "未认证或凭据无效"
            } else {
                "Unauthorized or invalid credentials"
            }
        }
        "error.forbidden" => {
            if zh {
                "无权执行该操作"
            } else {
                "Forbidden"
            }
        }
        "error.bad_request" => {
            if zh {
                "请求参数无效"
            } else {
                "Invalid request"
            }
        }
        "error.not_found" => {
            if zh {
                "资源不存在"
            } else {
                "Not found"
            }
        }
        "error.conflict" => {
            if zh {
                "资源状态冲突"
            } else {
                "Conflict"
            }
        }
        "error.internal" => {
            if zh {
                "内部错误"
            } else {
                "Internal error"
            }
        }
        "error.rate_limited" => {
            if zh {
                "请求过于频繁，请稍后重试"
            } else {
                "Rate limit exceeded; retry later"
            }
        }
        "error.queue_full" => {
            if zh {
                "服务繁忙，队列已满"
            } else {
                "Queue is full"
            }
        }
        "error.feature_disabled" => {
            if zh {
                "该功能未启用"
            } else {
                "Feature is disabled"
            }
        }
        "error.payment_not_configured" => {
            if zh {
                "支付渠道未配置"
            } else {
                "Payment provider is not configured"
            }
        }
        "error.payment_provider_unavailable" => {
            if zh {
                "支付渠道不可用"
            } else {
                "Payment provider is unavailable"
            }
        }
        "error.invalid_query" => {
            if zh {
                "查询过长或格式无效"
            } else {
                "Invalid query"
            }
        }
        "error.invalid_top_k" => {
            if zh {
                "top_k 超出上限"
            } else {
                "top_k exceeds the limit"
            }
        }
        "error.space_quota_exceeded" => {
            if zh {
                "空间配额已超限"
            } else {
                "Space quota exceeded"
            }
        }
        "error.tenant_quota_exceeded" => {
            if zh {
                "租户配额已超限"
            } else {
                "Tenant quota exceeded"
            }
        }
        "error.last_space_owner_protected" => {
            if zh {
                "不能移除或降级最后一个空间所有者"
            } else {
                "Cannot remove or demote the last space owner"
            }
        }
        "error.last_group_admin_protected" => {
            if zh {
                "不能移除或降级最后一个组管理员"
            } else {
                "Cannot remove or demote the last group admin"
            }
        }
        "error.space_not_active" => {
            if zh {
                "空间已归档或删除，只读"
            } else {
                "Space is archived or deleted"
            }
        }
        "error.acl_denied" => {
            if zh {
                "文档权限不允许访问"
            } else {
                "Document ACL denied access"
            }
        }
        "error.timeout" => {
            if zh {
                "请求超时"
            } else {
                "Request timed out"
            }
        }
        "error.storage_unavailable" => {
            if zh {
                "存储不可用"
            } else {
                "Storage unavailable"
            }
        }
        "cli.auth.logged_in" => {
            if zh {
                "已登录"
            } else {
                "Logged in"
            }
        }
        "cli.auth.logged_out" => {
            if zh {
                "已退出登录（本地凭据已删除）"
            } else {
                "Logged out (local credentials removed)"
            }
        }
        "cli.auth.device_not_implemented" => {
            if zh {
                "设备码登录将在 P1 提供；请使用 --token"
            } else {
                "Device-code login is a P1 item; use --token"
            }
        }
        "cli.billing.disabled" => {
            if zh {
                "计费功能未启用"
            } else {
                "Billing is disabled"
            }
        }
        "payment.not_configured" => {
            if zh {
                "支付渠道未配置"
            } else {
                "Payment provider is not configured"
            }
        }
        _ => return None,
    })
}

pub fn t(locale: &str, key: &str) -> String {
    lookup(locale, key).unwrap_or(key).to_string()
}

pub fn error_message(locale: &str, code: &str) -> Option<String> {
    lookup(locale, &format!("error.{code}")).map(|value| value.to_string())
}

pub fn format_number(locale: &str, value: f64, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    if normalize_tag(locale) == Some("en") {
        let (integer, fraction) = text.split_once('.').unwrap_or((text.as_str(), ""));
        let mut grouped = String::new();
        for (index, ch) in integer.chars().rev().enumerate() {
            if index > 0 && index % 3 == 0 {
                grouped.push(',');
            }
            grouped.push(ch);
        }
        let integer: String = grouped.chars().rev().collect();
        if fraction.is_empty() {
            integer
        } else {
            format!("{integer}.{fraction}")
        }
    } else {
        text
    }
}

pub fn format_date(locale: &str, value: &str) -> String {
    let parsed = DateTime::parse_from_rfc3339(value)
        .map(|date| date.with_timezone(&Utc))
        .ok();
    match parsed {
        Some(date) if normalize_tag(locale) == Some("en") => date.format("%b %d, %Y").to_string(),
        Some(date) => date.format("%Y-%m-%d").to_string(),
        None => value.to_string(),
    }
}

pub fn format_currency(locale: &str, minor_units: i64, currency: &str) -> String {
    let amount = minor_units as f64 / 100.0;
    let number = format_number(locale, amount, 2);
    let symbol = match currency.to_ascii_uppercase().as_str() {
        "USD" => "$",
        "CNY" => "¥",
        "EUR" => "€",
        "GBP" => "£",
        _ => "",
    };
    format!("{symbol}{number} {currency}")
}

pub fn plural(locale: &str, count: i64, one: &str, other: &str) -> String {
    if normalize_tag(locale) == Some("en") && count == 1 {
        one.to_string()
    } else {
        other.to_string()
    }
}
