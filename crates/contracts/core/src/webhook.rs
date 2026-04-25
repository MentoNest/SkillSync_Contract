//! Webhook / event relay configuration.
//!
//! The admin registers an off-chain relay URL. The contract emits structured
//! events that an indexer or relayer can pick up and forward as HTTP webhooks.

use soroban_sdk::{contracttype, Address, Env, String, Symbol, symbol_short};

const KEY_WEBHOOK_URL: Symbol = symbol_short!("wh_url");
const KEY_WEBHOOK_ENABLED: Symbol = symbol_short!("wh_on");

/// Webhook event payload stored in contract events.
#[contracttype]
#[derive(Clone, Debug)]
pub struct WebhookPayload {
    pub session_id: soroban_sdk::Bytes,
    pub event_type: String,
    pub timestamp: u64,
    pub data: String,
}

/// Admin-only: set the webhook relay URL.
pub fn set_webhook(env: &Env, admin: &Address, url: String) {
    admin.require_auth();
    env.storage().instance().set(&KEY_WEBHOOK_URL, &url);
    env.storage().instance().set(&KEY_WEBHOOK_ENABLED, &true);
}

/// Admin-only: disable webhook relay.
pub fn disable_webhook(env: &Env, admin: &Address) {
    admin.require_auth();
    env.storage().instance().set(&KEY_WEBHOOK_ENABLED, &false);
}

/// Returns the configured webhook URL, if any.
pub fn get_webhook_url(env: &Env) -> Option<String> {
    env.storage().instance().get(&KEY_WEBHOOK_URL)
}

/// Returns whether webhook relay is currently enabled.
pub fn is_webhook_enabled(env: &Env) -> bool {
    env.storage().instance().get(&KEY_WEBHOOK_ENABLED).unwrap_or(false)
}

/// Emit a structured webhook event for off-chain relay.
///
/// Call this from contract entry points after significant state changes.
/// An indexer reads these events and forwards them to the registered URL.
pub fn emit_webhook_event(
    env: &Env,
    session_id: soroban_sdk::Bytes,
    event_type: String,
    data: String,
) {
    if !is_webhook_enabled(env) {
        return;
    }
    let payload = WebhookPayload {
        session_id,
        event_type,
        timestamp: env.ledger().timestamp(),
        data,
    };
    env.events().publish(
        (symbol_short!("webhook"), symbol_short!("relay")),
        payload,
    );
}
