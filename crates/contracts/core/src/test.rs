#![cfg(test)]

extern crate std;

use crate::{CoreContract, CoreContractClient, SessionStatus};
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env,
};

fn setup() -> (
    Env,
    CoreContractClient<'static>,
    TokenClient<'static>,
    StellarAssetClient<'static>,
    Address,
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
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token_address = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_address);
    let asset_client = StellarAssetClient::new(&env, &token_address);

    asset_client.mint(&buyer, &1_000);

    let contract_id = env.register_contract(None, CoreContract);
    let contract = CoreContractClient::new(&env, &contract_id);
    contract.initialize(&admin, &treasury, &500);

    (
        env, contract, token_client, asset_client, buyer, seller, treasury, admin, contract_id,
    )
}

#[test]
fn approve_session_releases_payout_fee_and_event() {
    let (env, contract, token_client, _, buyer, seller, treasury, _admin, contract_id) = setup();

    contract.lock_funds(&1, &buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&1);
    contract.approve_session(&1);

    let session = contract.get_session(&1);
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
    let (env, contract, token_client, _, buyer, seller, _treasury, _admin, _contract_id) = setup();

    contract.lock_funds(&1, &buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&1);
    contract.approve_session(&1);

    let snapshot = env.to_snapshot();
    let approve_auth = snapshot.auth.0.last().unwrap();
    let auth_debug = std::format!("{:?}", approve_auth);
    assert!(auth_debug.contains("approve_session"));
}

#[test]
fn apply_fee_zero_fee() {
    let (_env, contract, token_client, _, buyer, seller, treasury, _admin, _contract_id) = setup();
    contract.set_fee_bps(&0);

    contract.lock_funds(&1, &buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&1);
    contract.approve_session(&1);

    assert_eq!(token_client.balance(&seller), 1_000);
    assert_eq!(token_client.balance(&treasury), 0);
}

#[test]
fn apply_fee_max_fee() {
    let (_env, contract, token_client, _, buyer, seller, treasury, _admin, _contract_id) = setup();
    contract.set_fee_bps(&10_000);

    contract.lock_funds(&1, &buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&1);
    contract.approve_session(&1);

    assert_eq!(token_client.balance(&seller), 0);
    assert_eq!(token_client.balance(&treasury), 1_000);
}

#[test]
fn apply_fee_rounding() {
    let (_env, contract, token_client, _, buyer, seller, treasury, _admin, _contract_id) = setup();
    contract.set_fee_bps(&1);

    contract.lock_funds(&1, &buyer, &seller, &token_client.address, &10_000);
    contract.complete_session(&1);
    contract.approve_session(&1);

    assert_eq!(token_client.balance(&seller), 9_999);
    assert_eq!(token_client.balance(&treasury), 1);
}

#[test]
fn early_refund_skips_fee() {
    let (_env, contract, token_client, _, buyer, seller, treasury, _admin, contract_id) = setup();

    contract.lock_funds(&1, &buyer, &seller, &token_client.address, &1_000);
    contract.refund_session(&1);

    let session = contract.get_session(&1);
    assert!(matches!(session.status, SessionStatus::Refunded));
    assert_eq!(token_client.balance(&buyer), 1_000);
    assert_eq!(token_client.balance(&seller), 0);
    assert_eq!(token_client.balance(&treasury), 0);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn late_refund_applies_fee() {
    let (_env, contract, token_client, _, buyer, seller, treasury, _admin, contract_id) = setup();

    contract.lock_funds(&1, &buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&1);
    contract.refund_session(&1);

    let session = contract.get_session(&1);
    assert!(matches!(session.status, SessionStatus::Refunded));
    assert_eq!(token_client.balance(&buyer), 950);
    assert_eq!(token_client.balance(&seller), 0);
    assert_eq!(token_client.balance(&treasury), 50);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn resolve_dispute_applies_fee_and_splits() {
    let (_env, contract, token_client, _, buyer, seller, treasury, _admin, contract_id) = setup();

    contract.lock_funds(&1, &buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&1);
    contract.resolve_dispute(&1, &400);

    let session = contract.get_session(&1);
    assert!(matches!(session.status, SessionStatus::Resolved));
    assert_eq!(token_client.balance(&buyer), 400);
    assert_eq!(token_client.balance(&seller), 550);
    assert_eq!(token_client.balance(&treasury), 50);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn resolve_dispute_full_refund_to_buyer() {
    let (_env, contract, token_client, _, buyer, seller, treasury, _admin, contract_id) = setup();

    contract.lock_funds(&1, &buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&1);
    contract.resolve_dispute(&1, &950);

    let session = contract.get_session(&1);
    assert!(matches!(session.status, SessionStatus::Resolved));
    assert_eq!(token_client.balance(&buyer), 950);
    assert_eq!(token_client.balance(&seller), 0);
    assert_eq!(token_client.balance(&treasury), 50);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn resolve_dispute_zero_buyer_refund() {
    let (_env, contract, token_client, _, buyer, seller, treasury, _admin, contract_id) = setup();

    contract.lock_funds(&1, &buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&1);
    contract.resolve_dispute(&1, &0);

    let session = contract.get_session(&1);
    assert!(matches!(session.status, SessionStatus::Resolved));
    assert_eq!(token_client.balance(&buyer), 0);
    assert_eq!(token_client.balance(&seller), 950);
    assert_eq!(token_client.balance(&treasury), 50);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn initialize_sets_admin_treasury_and_fee_bps() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, CoreContract);
    let contract = CoreContractClient::new(&env, &contract_id);
    contract.initialize(&admin, &treasury, &500);

    assert_eq!(contract.admin(), admin);
    assert_eq!(contract.treasury(), treasury);
    assert_eq!(contract.fee_bps(), 500);
}

#[test]
#[should_panic(expected = "contract already initialized")]
fn initialize_twice_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, CoreContract);
    let contract = CoreContractClient::new(&env, &contract_id);
    contract.initialize(&admin, &treasury, &500);
    contract.initialize(&admin, &treasury, &500);
}

#[test]
fn initialize_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, CoreContract);
    let contract = CoreContractClient::new(&env, &contract_id);
    contract.initialize(&admin, &treasury, &500);

    let events = env.events().all();
    let init_event = events.iter().find(|e| {
        std::format!("{:?}", e.1).contains("init")
    }).unwrap();
    assert_eq!(init_event.0, contract_id);
}

#[test]
#[should_panic(expected = "contract not initialized")]
fn uninitialized_lock_funds_reverts() {
    let env = Env::default();
    env.mock_all_auths();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);

    let contract_id = env.register_contract(None, CoreContract);
    let contract = CoreContractClient::new(&env, &contract_id);
    contract.lock_funds(&1, &buyer, &seller, &buyer, &100);
}

#[test]
#[should_panic(expected = "contract not initialized")]
fn uninitialized_get_session_reverts() {
    let env = Env::default();
    let contract_id = env.register_contract(None, CoreContract);
    let contract = CoreContractClient::new(&env, &contract_id);
    contract.get_session(&1);
}

#[test]
#[should_panic(expected = "contract not initialized")]
fn uninitialized_admin_reverts() {
    let env = Env::default();
    let contract_id = env.register_contract(None, CoreContract);
    let contract = CoreContractClient::new(&env, &contract_id);
    contract.admin();
}

#[test]
fn happy_path_lock_complete_approve_payout_and_events() {
    let (env, contract, token_client, _, buyer, seller, treasury, _admin, contract_id) = setup();

    contract.lock_funds(&1, &buyer, &seller, &token_client.address, &1_000);
    assert!(matches!(contract.get_session(&1).status, SessionStatus::Pending));

    contract.complete_session(&1);
    assert!(matches!(contract.get_session(&1).status, SessionStatus::Completed));

    contract.approve_session(&1);
    assert!(matches!(contract.get_session(&1).status, SessionStatus::Approved));

    assert_eq!(token_client.balance(&buyer), 0);
    assert_eq!(token_client.balance(&seller), 950);
    assert_eq!(token_client.balance(&treasury), 50);
    assert_eq!(token_client.balance(&contract_id), 0);

    let events = env.events().all();
    let topics: Vec<String> = events.iter().map(|e| std::format!("{:?}", e.1)).collect();
    let completed_idx = topics.iter().position(|t| t.contains("completed")).unwrap();
    let fee_idx = topics.iter().position(|t| t.contains("fee_deducted")).unwrap();
    let approved_idx = topics.iter().position(|t| t.contains("approved")).unwrap();

    assert!(completed_idx < fee_idx, "completed event should come before fee_deducted");
    assert!(fee_idx < approved_idx, "fee_deducted event should come before approved");
}

#[test]
#[should_panic(expected = "session not found")]
fn seller_cannot_complete_without_lock() {
    let (_, contract, _, _, _, _, _, _, _) = setup();
    contract.complete_session(&999);
}

#[test]
#[should_panic(expected = "session must be pending")]
fn seller_cannot_complete_twice() {
    let (_, contract, token_client, _, buyer, seller, _, _, _) = setup();
    contract.lock_funds(&1, &buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&1);
    contract.complete_session(&1);
}

#[test]
#[should_panic(expected = "session must be completed")]
fn buyer_cannot_approve_before_completion() {
    let (_, contract, token_client, _, buyer, seller, _, _, _) = setup();
    contract.lock_funds(&1, &buyer, &seller, &token_client.address, &1_000);
    contract.approve_session(&1);
}

#[test]
fn platform_fee_deducted_correctly() {
    let (_, contract, token_client, _, buyer, seller, treasury, _, _) = setup();
    contract.lock_funds(&1, &buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&1);
    contract.approve_session(&1);
    assert_eq!(token_client.balance(&treasury), 50);
}

#[test]
fn seller_receives_correct_payout() {
    let (_, contract, token_client, _, buyer, seller, _, _, _) = setup();
    contract.lock_funds(&1, &buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&1);
    contract.approve_session(&1);
    assert_eq!(token_client.balance(&seller), 950);
}

// TASK 4: lock_funds tests

#[test]
fn lock_funds_with_sufficient_balance() {
    let (_, contract, token_client, _, buyer, seller, _, _, contract_id) = setup();
    contract.lock_funds(&1, &buyer, &seller, &token_client.address, &1_000);

    let session = contract.get_session(&1);
    assert!(matches!(session.status, SessionStatus::Pending));
    assert_eq!(token_client.balance(&contract_id), 1_000);
    assert_eq!(token_client.balance(&buyer), 0);
}

#[test]
fn contract_balance_increases_by_exact_amount() {
    let (_, contract, token_client, _, buyer, seller, _, _, contract_id) = setup();
    let before = token_client.balance(&contract_id);
    contract.lock_funds(&1, &buyer, &seller, &token_client.address, &1_000);
    let after = token_client.balance(&contract_id);
    assert_eq!(after - before, 1_000);
}

#[test]
#[should_panic(expected = "session already exists")]
fn duplicate_session_id_reverts() {
    let (_, contract, token_client, _, buyer, seller, _, _, _) = setup();
    contract.lock_funds(&1, &buyer, &seller, &token_client.address, &1_000);
    contract.lock_funds(&1, &buyer, &seller, &token_client.address, &2_000);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn zero_amount_reverts() {
    let (_, contract, token_client, _, buyer, seller, _, _, _) = setup();
    contract.lock_funds(&1, &buyer, &seller, &token_client.address, &0);
}

#[test]
#[should_panic]
fn insufficient_buyer_balance_reverts() {
    let env = Env::default();
    env.mock_all_auths();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let treasury = Address::generate(&env);
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token_address = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_address);
    let asset_client = StellarAssetClient::new(&env, &token_address);

    asset_client.mint(&buyer, &500);

    let contract_id = env.register_contract(None, CoreContract);
    let contract = CoreContractClient::new(&env, &contract_id);
    contract.initialize(&admin, &treasury, &500);

    contract.lock_funds(&1, &buyer, &seller, &token_client.address, &1_000);
}

#[test]
fn funds_locked_event_emitted() {
    let (env, contract, token_client, _, buyer, seller, _, _, contract_id) = setup();
    contract.lock_funds(&1, &buyer, &seller, &token_client.address, &1_000);

    let events = env.events().all();
    let locked_event = events.iter().find(|e| {
        std::format!("{:?}", e.1).contains("locked")
    }).unwrap();
    assert_eq!(locked_event.0, contract_id);
}
