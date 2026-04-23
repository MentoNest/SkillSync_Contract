#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _, LedgerInfo},
    token, vec, Address, Bytes, Env, IntoVal, Symbol,
};

fn setup_env() -> (Env, SkillSyncContractClient, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SkillSyncContract);
    let client = SkillSyncContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);
    (env, client, admin, treasury)
}

#[test]
fn test_auto_refund_success() {
    let (env, client, _admin, _treasury) = setup_env();

    let payer = Address::generate(&env);
    let payee = Address::generate(&env);
    
    // Setup token
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract(token_admin.clone());
    let token_id = Address::from_contract_id(&env, &token_contract);
    let token_client = token::Client::new(&env, &token_id);

    let amount = 1000_i128;
    let fee_bps = 500u32; // 5%
    let fee = (amount * fee_bps as i128) / 10000;
    let total = amount + fee;

    token_client.mint(&payer, &total);

    let session_id = Bytes::from_slice(&env, b"session_123");

    // 1. Lock funds
    client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);
    assert_eq!(token_client.balance(&payer), 0);

    // 2. Complete session
    let nonce = 1u64;
    client.complete_session(&session_id, &payee, &nonce);

    let session = client.get_session(&session_id).unwrap();
    assert_eq!(session.status, SessionStatus::Completed);

    // 3. Try auto_refund before window (should fail)
    let result = client.try_auto_refund(&session_id);
    assert!(result.is_err());

    // 4. Advance ledger time beyond dispute window
    env.ledger().set(LedgerInfo {
        timestamp: env.ledger().timestamp() + DEFAULT_DISPUTE_WINDOW_SECONDS + 1,
        protocol_version: 20,
        sequence_number: 100,
        network_id: [0u8; 32],
        base_reserve: 100,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 100,
    });

    // 5. Run auto_refund
    client.auto_refund(&session_id);

    // 6. Verify refund
    assert_eq!(token_client.balance(&payer), total);
    let session = client.get_session(&session_id).unwrap();
    assert_eq!(session.status, SessionStatus::Refunded);
}

#[test]
fn test_auto_refund_fails_if_not_completed() {
    let (env, client, _admin, _treasury) = setup_env();

    let payer = Address::generate(&env);
    let payee = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract(token_admin.clone());
    let token_id = Address::from_contract_id(&env, &token_contract);
    let token_client = token::Client::new(&env, &token_id);

    let amount = 1000_i128;
    token_client.mint(&payer, &1100);

    let session_id = Bytes::from_slice(&env, b"session_locked");
    client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &0);

    // Advance time
    env.ledger().set(LedgerInfo {
        timestamp: env.ledger().timestamp() + DEFAULT_DISPUTE_WINDOW_SECONDS + 1,
        protocol_version: 20,
        sequence_number: 100,
        network_id: [0u8; 32],
        base_reserve: 100,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 100,
    });

    // Should fail because status is Locked, not Completed
    let result = client.try_auto_refund(&session_id);
    assert!(result.is_err());
}

#[test]
fn test_dispute_and_resolution() {
    let (env, client, admin, treasury) = setup_env();

    let payer = Address::generate(&env);
    let payee = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract(token_admin.clone());
    let token_id = Address::from_contract_id(&env, &token_contract);
    let token_client = token::Client::new(&env, &token_id);

    let amount = 1000_i128;
    let fee_bps = 250u32; // 2.5%
    let fee = (amount * fee_bps as i128) / 10000; // 25
    let total = amount + fee; // 1025

    token_client.mint(&payer, &total);

    let session_id = Bytes::from_slice(&env, b"dispute_session");

    // 1. Lock funds
    client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

    // 2. Open dispute as payer
    let reason = Bytes::from_slice(&env, b"Service not as described");
    client.open_dispute(&session_id, &payer, &reason);

    let session = client.get_session(&session_id).unwrap();
    assert_eq!(session.status, SessionStatus::Disputed);
    assert!(session.dispute_opened_at > 0);

    // 3. Resolve dispute (50/50 split)
    let to_payer = 500_i128;
    let to_payee = 500_i128;
    let note = Some(Bytes::from_slice(&env, b"Split agreement"));
    
    client.resolve_dispute(&session_id, &to_payer, &to_payee, &note);

    // 4. Verify balances
    assert_eq!(token_client.balance(&payer), to_payer);
    assert_eq!(token_client.balance(&payee), to_payee);
    assert_eq!(token_client.balance(&treasury), fee);

    let session = client.get_session(&session_id).unwrap();
    assert_eq!(session.status, SessionStatus::Resolved);
    assert_eq!(session.resolved_at, env.ledger().timestamp());
    assert_eq!(session.resolver, Some(admin));
}

#[test]
fn test_open_dispute_unauthorized() {
    let (env, client, _admin, _treasury) = setup_env();
    let payer = Address::generate(&env);
    let payee = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract(Address::generate(&env));
    let token_id = Address::from_contract_id(&env, &token_contract);

    let session_id = Bytes::from_slice(&env, b"auth_test");
    client.lock_funds(&session_id, &payer, &payee, &token_id, &1000, &0);

    let unauthorized = Address::generate(&env);
    let result = client.try_open_dispute(&session_id, &unauthorized, &Bytes::new(&env));
    assert!(result.is_err());
}

#[test]
fn test_open_dispute_on_completed_session() {
    let (env, client, _admin, _treasury) = setup_env();
    let payer = Address::generate(&env);
    let payee = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract(Address::generate(&env));
    let token_id = Address::from_contract_id(&env, &token_contract);
    let token_client = token::Client::new(&env, &token_id);

    token_client.mint(&payer, &1000);
    let session_id = Bytes::from_slice(&env, b"completed_dispute");
    client.lock_funds(&session_id, &payer, &payee, &token_id, &1000, &0);

    // Mark as completed
    client.complete_session(&session_id, &payee, &1u64);

    // Open dispute
    client.open_dispute(&session_id, &payee, &Bytes::from_slice(&env, b"Completed but unhappy"));

    let session = client.get_session(&session_id).unwrap();
    assert_eq!(session.status, SessionStatus::Disputed);
}
