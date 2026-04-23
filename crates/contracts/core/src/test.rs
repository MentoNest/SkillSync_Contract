#![cfg(test)]

use super::*;
use crate::{CoreContract, CoreContractClient};
use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, BytesN, Env};
extern crate std;

use crate::{CoreContract, CoreContractClient, SessionStatus};
use soroban_sdk::{
    bytesn,
    testutils::{Address as _, Events as _},
    token::{Client as TokenClient, StellarAssetClient},
    Address, BytesN, Env,
};

fn lock_session_id(env: &Env) -> BytesN<32> {
    bytesn!(
        env,
        0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20
    )
}

fn setup() -> (
    Env,
    CoreContractClient<'static>,
    TokenClient<'static>,
    StellarAssetClient<'static>,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token_address = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_address);
    let asset_client = StellarAssetClient::new(&env, &token_address);

    asset_client.mint(&buyer, &1_000);

    let contract_id = env.register_contract(None, CoreContract);
    let contract = CoreContractClient::new(&env, &contract_id);
    contract.initialize(&treasury, &500);

    (
        env,
        contract,
        token_client,
        asset_client,
        buyer,
        seller,
        treasury,
        contract_id,
    )
}

#[test]
fn approve_session_releases_payout_fee_and_event() {
    let (env, contract, token_client, _, buyer, seller, treasury, contract_id) = setup();

    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&session_id);
    contract.approve_session(&session_id);

    let session = contract.get_session(&session_id);
    assert!(matches!(session.status, SessionStatus::Approved));
    assert_eq!(token_client.balance(&buyer), 0);
    assert_eq!(token_client.balance(&seller), 950);
    assert_eq!(token_client.balance(&treasury), 50);
    assert_eq!(token_client.balance(&contract_id), 0);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    assert_eq!(last_event.0, contract_id);
    assert!(std::format!("{:?}", last_event.1).contains("approved"));
}

#[test]
fn approve_session_records_buyer_authorization() {
    let (env, contract, token_client, _, buyer, seller, _, _) = setup();

    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&session_id);
    contract.approve_session(&session_id);

    let snapshot = env.to_snapshot();
    let approve_auth = snapshot.auth.0.last().unwrap();
    let auth_debug = std::format!("{:?}", approve_auth);
    assert!(auth_debug.contains("approve_session"));
}

#[test]
fn lock_funds_stores_locked_session() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CoreContract);
    let client = CoreContractClient::new(&env, &contract_id);

    let seller = Address::generate(&env);
    let session_id = lock_session_id(&env);

    client.lock_funds(&session_id, &seller, &500_i128);

    let session = client.get_locked_session(&session_id).unwrap();
    assert_eq!(session.seller, seller);
    assert_eq!(session.amount, 500_i128);
    assert_eq!(session.status, SessionStatus::Locked);
}

#[test]
#[should_panic]
fn lock_funds_rejects_duplicate_session_ids() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CoreContract);
    let client = CoreContractClient::new(&env, &contract_id);

    let seller = Address::generate(&env);
    let session_id = lock_session_id(&env);

    client.lock_funds(&session_id, &seller, &500_i128);
    client.lock_funds(&session_id, &seller, &500_i128);
}

#[test]
#[should_panic]
fn lock_funds_requires_positive_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CoreContract);
    let client = CoreContractClient::new(&env, &contract_id);

    let seller = Address::generate(&env);

    client.lock_funds(&lock_session_id(&env), &seller, &0_i128);

use crate::{CoreContract, CoreContractClient, SessionStatus};
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
    let token_address = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_address);
    let asset_client = StellarAssetClient::new(&env, &token_address);

    asset_client.mint(&buyer, &1_000);

    let contract_id = env.register_contract(None, CoreContract);
    let client = CoreContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    client.init(&admin);

    let result = client.hello(&symbol_short!("World"));
    assert_eq!(result, vec![&env, symbol_short!("Hello"), symbol_short!("World")]);
}

#[test]
fn test_upgrade() {
    let env = Env::default();
    env.mock_all_auths();
    
    let contract_id = env.register_contract(None, CoreContract);
    let client = CoreContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    client.init(&admin);

    let new_wasm_hash = BytesN::from_array(&env, &[1; 32]);
    client.upgrade(&new_wasm_hash);
    
    // Auth should be checked
    assert_eq!(
        env.auths(),
        alloc::vec![(
            admin.clone(),
            soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "upgrade",
                args: vec![&env, new_wasm_hash.clone().into_val(&env)],
                sub_invokes: &[],
            }
        )]
    );
}
    let contract = CoreContractClient::new(&env, &contract_id);
    contract.initialize(&treasury, &500);

    (
        env,
        contract,
        token_client,
        asset_client,
        buyer,
        seller,
        treasury,
        contract_id,
    )
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
    let snapshot = env.to_snapshot();
    let approve_auth = snapshot.auth.0.last().unwrap();
    let auth_debug = std::format!("{:?}", approve_auth);
    assert!(auth_debug.contains("approve_session"));

}
