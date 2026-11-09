//! Multi-turn query rewriting. Retrieval ACL remains governed by the caller's
//! Policy; conversation context only changes the query text.

use crate::config::Config;
use crate::gateway::{ChatMessage, Gateway, GatewayRequest};
use crate::governance::Principal;
use crate::store::Store;
use std::sync::Arc;

pub async fn rewrite_query(
    store: &Arc<dyn Store>,
    config: &Config,
    principal: &Principal,
    conversation_id: &str,
    query: &str,
) -> (String, String, Option<String>) {
    let messages = store
        .list_messages(conversation_id, config.query.context_messages as i64)
        .await
        .unwrap_or_default();
    let context = messages
        .iter()
        .map(|message| format!("{}: {}", message.role, message.content))
        .collect::<Vec<_>>()
        .join("\n");
    let context_hash = crate::util::sha256_hex(context.as_bytes());
    if !config.query.rewrite_enabled || messages.is_empty() {
        return (query.to_string(), context_hash, None);
    }
    let prompt = format!(
        "Rewrite the follow-up question into a standalone search query. Return only the query text, no explanation.\nConversation:\n{context}\nFollow-up: {query}"
    );
    let request = GatewayRequest {
        request_id: format!("rw_{}", crate::util::random_hex(10)),
        principal: principal.clone(),
        space_id: None,
        role: "user".to_string(),
        alias: config.query.rewrite_alias.clone(),
        messages: vec![ChatMessage {
            role: "user".into(),
            content: prompt,
        }],
        max_output_tokens: 128,
        temperature: 0.0,
        source: "kb".to_string(),
        rewritten_query: None,
    };
    match Gateway::new(store.clone())
        .with_local_ledger(config.billing.engine.eq_ignore_ascii_case("local"))
        .chat(&request)
        .await
    {
        Ok(response) => {
            let rewritten = response.content.trim().to_string();
            if rewritten.is_empty() {
                (
                    query.to_string(),
                    context_hash,
                    Some("conversation_rewrite_failed".to_string()),
                )
            } else {
                if let Some(usage_event_id) = response.usage_event_id.as_deref() {
                    let _ = store
                        .set_usage_rewritten_query(usage_event_id, &rewritten)
                        .await;
                }
                (rewritten, context_hash, None)
            }
        }
        Err(err) => (
            query.to_string(),
            context_hash,
            Some(format!("conversation_rewrite_failed: {}", err.code)),
        ),
    }
}
