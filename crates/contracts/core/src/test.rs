#![cfg(test)]

use super::*;
use crate::{SkillSyncContract, SkillSyncContractClient, DEFAULT_DISPUTE_WINDOW_SECONDS, DEFAULT_DISPUTE_WINDOW_LEDGERS, DISPUTE_WINDOW_MIN_LEDGERS, DISPUTE_WINDOW_MAX_LEDGERS};
use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, BytesN, Env};
extern crate std;

use crate::{AutoRefundExecutedEvent, CoreContract, CoreContractClient, SessionStatus};
use soroban_sdk::{
    bytesn,
    testutils::{Address as _, Events as _},
    testutils::{Address as _, Events as _, Ledger as _},
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
    setup_with_fee(500) // Default 5% fee
}

fn setup_with_admin() -> (
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

    let contract_id = env.register_contract(None, SkillSyncContract);
    let contract = SkillSyncContractClient::new(&env, &contract_id);
    contract.init(&admin, &500, &treasury, &DEFAULT_DISPUTE_WINDOW_LEDGERS);

    (
        env,
        contract,
        token_client,
        asset_client,
        buyer,
        seller,
        treasury,
        contract_id,
        admin,
    )
}

fn setup_with_fee(
    fee_bps: u32,
) -> (
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
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token_address = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_address);
    let asset_client = StellarAssetClient::new(&env, &token_address);

    let contract_id = env.register_contract(None, SkillSyncContract);
    let contract = SkillSyncContractClient::new(&env, &contract_id);
    contract.init(&admin, &fee_bps, &treasury, &DEFAULT_DISPUTE_WINDOW_LEDGERS);

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

fn mint_and_approve(asset_client: &StellarAssetClient<'static>, buyer: &Address, amount: i128) {
    asset_client.mint(buyer, &amount);
}

// ============================================================================
// Test 5a: Happy Path - Create → Complete → Approve
// ============================================================================

#[test]
fn test_happy_path_create_complete_approve() {
    let (env, contract, token_client, asset_client, buyer, seller, treasury, contract_id) = setup();

    // Fund buyer
    mint_and_approve(&asset_client, &buyer, 1_000);

    // Create session
    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &1_000);
    assert_eq!(token_client.balance(&buyer), 0);
    assert_eq!(token_client.balance(&contract_id), 1_000);

    // Complete session
    contract.complete_session(&session_id, &seller, &0);

    // Approve session
    contract.approve_session(&session_id, &buyer, &1);

    // Verify balances
    let session = contract.get_session(&session_id);
    assert!(matches!(session.status, SessionStatus::Approved));
    assert_eq!(token_client.balance(&buyer), 0);
    assert_eq!(token_client.balance(&seller), 950); // 1000 - 5% fee
    assert_eq!(token_client.balance(&treasury), 50); // 5% fee
    assert_eq!(token_client.balance(&contract_id), 0);

    // Verify event
    let events = env.events().all();
    let last_event = events.last().unwrap();
    assert_eq!(last_event.0, contract_id);
    assert!(std::format!("{:?}", last_event.1).contains("approved"));
}

#[test]
fn test_pause_blocks_state_changes_but_allows_view() {
    let (env, contract, token_client, asset_client, buyer, seller, treasury, contract_id, _admin) =
        setup_with_admin();

    mint_and_approve(&asset_client, &buyer, 1_000);
    contract.pause().unwrap();
    assert!(contract.is_paused());
    assert_eq!(contract.get_platform_fee(), 500);

    let result = env.try_invoke_contract(
        &contract_id,
        &"create_session",
        buyer.clone().into_val(&env),
        seller.clone().into_val(&env),
        token_client.address.into_val(&env),
        (1_000_i128).into_val(&env),
    );
    assert!(result.is_err());

    contract.unpause().unwrap();
    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &1_000);
    assert_eq!(token_client.balance(&contract_id), 1_000);
    assert!(!contract.is_paused());
}

// ============================================================================
// Test 5b: Refund Path - Create → Refund Initiate → Refund Approve
// ============================================================================

#[test]
fn test_refund_path_initiate_approve() {
    let (env, contract, token_client, asset_client, buyer, seller, treasury, contract_id) = setup();

    // Fund buyer
    mint_and_approve(&asset_client, &buyer, 1_000);

    // Create session
    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &1_000);
    assert_eq!(token_client.balance(&contract_id), 1_000);

    // Buyer initiates refund
    contract.refund_initiate(&session_id);
    let session = contract.get_session(&session_id);
    assert!(matches!(session.status, SessionStatus::RefundRequested));

    // Seller approves refund
    contract.refund_approve(&session_id);

    // Verify balances - full refund, no fees
    let session = contract.get_session(&session_id);
    assert!(matches!(session.status, SessionStatus::Refunded));
    assert_eq!(token_client.balance(&buyer), 1_000);
    assert_eq!(token_client.balance(&seller), 0);
    assert_eq!(token_client.balance(&treasury), 0);
    assert_eq!(token_client.balance(&contract_id), 0);
}

// ============================================================================
// Test 5c: Refund After Completion
// ============================================================================

#[test]
fn test_refund_after_completion() {
    let (env, contract, token_client, asset_client, buyer, seller, treasury, contract_id) = setup();

    // Fund buyer
    mint_and_approve(&asset_client, &buyer, 1_000);

    // Create and complete session
    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&session_id);

    // Buyer initiates refund after completion
    contract.refund_initiate(&session_id);

    // Seller approves refund
    contract.refund_approve(&session_id);

    // Verify full refund
    let session = contract.get_session(&session_id);
    assert!(matches!(session.status, SessionStatus::Refunded));
    assert_eq!(token_client.balance(&buyer), 1_000);
    assert_eq!(token_client.balance(&seller), 0);
    assert_eq!(token_client.balance(&treasury), 0);
}

// ============================================================================
// Test 5d: Dispute Path - Create → Complete → Dispute → Resolve
// ============================================================================

#[test]
fn test_dispute_initiate_and_resolve() {
    let (env, contract, token_client, asset_client, buyer, seller, treasury, contract_id) = setup();

    // Fund buyer
    mint_and_approve(&asset_client, &buyer, 1_000);

    // Create and complete session
    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&session_id);

    // Buyer initiates dispute
    contract.dispute_initiate(&session_id);
    let session = contract.get_session(&session_id);
    assert!(matches!(session.status, SessionStatus::Disputed));

    // Treasury resolves: 600 to buyer, 400 to seller
    contract.dispute_resolve(&session_id, &600);

    // Verify distribution
    let session = contract.get_session(&session_id);
    assert!(matches!(session.status, SessionStatus::Approved));
    assert_eq!(token_client.balance(&buyer), 600);
    assert_eq!(token_client.balance(&seller), 400);
    assert_eq!(token_client.balance(&treasury), 0); // No fee on disputed amount
    assert_eq!(token_client.balance(&contract_id), 0);
}

// ============================================================================
// Test 5e: Multiple Concurrent Sessions - No Interference
// ============================================================================

#[test]
fn test_multiple_concurrent_sessions_no_interference() {
    let (env, contract, token_client, asset_client, buyer, seller, treasury, contract_id) = setup();

    // Create 3 different buyers and sellers
    let buyer2 = Address::generate(&env);
    let buyer3 = Address::generate(&env);
    let seller2 = Address::generate(&env);
    let seller3 = Address::generate(&env);

    // Fund all buyers
    mint_and_approve(&asset_client, &buyer, 500);
    mint_and_approve(&asset_client, &buyer2, 1_000);
    mint_and_approve(&asset_client, &buyer3, 1_500);

    // Create 3 sessions with different amounts
    let session1 = contract.create_session(&buyer, &seller, &token_client.address, &500);
    let session2 = contract.create_session(&buyer2, &seller2, &token_client.address, &1_000);
    let session3 = contract.create_session(&buyer3, &seller3, &token_client.address, &1_500);

    // Complete in random order: 2, 1, 3
    contract.complete_session(&session2);
    contract.complete_session(&session1);
    contract.complete_session(&session3);

    // Approve in different order: 3, 1, 2
    contract.approve_session(&session3);
    contract.approve_session(&session1);
    contract.approve_session(&session2);

    // Verify each session independently
    let s1 = contract.get_session(&session1);
    let s2 = contract.get_session(&session2);
    let s3 = contract.get_session(&session3);

    assert!(matches!(s1.status, SessionStatus::Approved));
    assert!(matches!(s2.status, SessionStatus::Approved));
    assert!(matches!(s3.status, SessionStatus::Approved));

    // Verify balances
    assert_eq!(token_client.balance(&seller), 475); // 500 - 5%
    assert_eq!(token_client.balance(&seller2), 950); // 1000 - 5%
    assert_eq!(token_client.balance(&seller3), 1425); // 1500 - 5%

    // Total treasury: 25 + 50 + 75 = 150
    assert_eq!(token_client.balance(&treasury), 150);
    assert_eq!(token_client.balance(&contract_id), 0);
}

// ============================================================================
// Test 5f: Fee Accumulation in Treasury
// ============================================================================

#[test]
fn test_fee_accumulation_in_treasury() {
    let (env, contract, token_client, asset_client, buyer, seller, treasury, contract_id) =
        setup_with_fee(300);

    // Create 5 sessions with varying amounts
    let amounts = vec![200, 400, 600, 800, 1000];
    let mut expected_treasury: i128 = 0;

    for amount in &amounts {
        mint_and_approve(&asset_client, &buyer, *amount);
        let session_id = contract.create_session(&buyer, &seller, &token_client.address, &amount);
        contract.complete_session(&session_id);
        contract.approve_session(&session_id);

        // Fee = amount * 300 / 10000
        expected_treasury += amount * 300 / 10_000;
    }

    // Verify treasury accumulation
    assert_eq!(token_client.balance(&treasury), expected_treasury);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn test_fee_edge_cases() {
    // Test with 0% fee
    let (env, contract, token_client, asset_client, buyer, seller, treasury, contract_id) =
        setup_with_fee(0);

    mint_and_approve(&asset_client, &buyer, 1_000);
    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&session_id);
    contract.approve_session(&session_id);

    assert_eq!(token_client.balance(&seller), 1_000);
    assert_eq!(token_client.balance(&treasury), 0);

    // Test with 100% fee (10000 bps)
    let (env, contract, token_client, asset_client, buyer, seller, treasury, contract_id) =
        setup_with_fee(10_000);

    mint_and_approve(&asset_client, &buyer, 1_000);
    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&session_id);
    contract.approve_session(&session_id);

    assert_eq!(token_client.balance(&seller), 0);
    assert_eq!(token_client.balance(&treasury), 1_000);
}

// ============================================================================
// Test 5g: Invalid State Transitions
// ============================================================================

#[test]
#[should_panic(expected = "session must be pending")]
fn test_cannot_complete_already_completed() {
    let (_, contract, token_client, asset_client, buyer, seller, _, _) = setup();

    mint_and_approve(&asset_client, &buyer, 1_000);
    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&session_id);

    // Try to complete again - should panic
    contract.complete_session(&session_id);
}

#[test]
#[should_panic(expected = "session must be completed")]
fn test_cannot_approve_pending_session() {
    let (_, contract, token_client, asset_client, buyer, seller, _, _) = setup();

    mint_and_approve(&asset_client, &buyer, 1_000);
    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &1_000);

    // Try to approve without completing - should panic
    contract.approve_session(&session_id);
}

#[test]
#[should_panic(expected = "session must be pending or completed")]
fn test_cannot_refund_approved_session() {
    let (_, contract, token_client, asset_client, buyer, seller, _, _) = setup();

    mint_and_approve(&asset_client, &buyer, 1_000);
    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&session_id);
    contract.approve_session(&session_id);

    // Try to refund after approval - should panic
    contract.refund_initiate(&session_id);
}

#[test]
#[should_panic(expected = "session must be pending or completed")]
fn test_cannot_dispute_refunded_session() {
    let (_, contract, token_client, asset_client, buyer, seller, _, _) = setup();

    mint_and_approve(&asset_client, &buyer, 1_000);
    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &1_000);
    contract.refund_initiate(&session_id);
    contract.refund_approve(&session_id);

    // Try to dispute after refund - should panic
    contract.dispute_initiate(&session_id);
}

#[test]
#[should_panic(expected = "buyer and seller must differ")]
fn test_cannot_create_session_with_same_buyer_seller() {
    let (_, contract, token_client, asset_client, buyer, _, _, _) = setup();

    mint_and_approve(&asset_client, &buyer, 1_000);

    // Try to create session where buyer == seller - should panic
    contract.create_session(&buyer, &buyer, &token_client.address, &1_000);
}

#[test]
fn test_authorization_recorded_for_approve() {
    let (env, contract, token_client, asset_client, buyer, seller, _, _) = setup();

    mint_and_approve(&asset_client, &buyer, 1_000);
    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&session_id);
    contract.approve_session(&session_id);

    let snapshot = env.to_snapshot();
    let approve_auth = snapshot.auth.0.last().unwrap();
    let auth_debug = std::format!("{:?}", approve_auth);
    assert!(auth_debug.contains("approve_session"));
}

#[test]
fn refund_session_buyer_can_refund_before_completion() {
    let (env, contract, token_client, _, buyer, seller, treasury, contract_id) = setup();

    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &1_000);

    // Verify initial state
    let session = contract.get_session(&session_id);
    assert!(matches!(session.status, SessionStatus::Pending));
    assert_eq!(token_client.balance(&buyer), 0);
    assert_eq!(token_client.balance(&contract_id), 1_000);

    // Buyer refunds the session
    contract.refund_session(&session_id);

    // Verify session is refunded
    let refunded_session = contract.get_session(&session_id);
    assert!(matches!(refunded_session.status, SessionStatus::Approved));

    // Verify full amount returned to buyer, no fee deducted
    assert_eq!(token_client.balance(&buyer), 1_000);
    assert_eq!(token_client.balance(&seller), 0);
    assert_eq!(token_client.balance(&treasury), 0);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn refund_session_full_amount_no_fee() {
    let (env, contract, token_client, _, buyer, seller, treasury, contract_id) = setup();

    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &2_500);

    // Buyer refunds
    contract.refund_session(&session_id);

    // Verify full amount refunded (no 5% fee applied)
    assert_eq!(token_client.balance(&buyer), 2_500);
    assert_eq!(token_client.balance(&treasury), 0);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn refund_session_reverts_if_already_completed() {
    let (env, contract, token_client, _, buyer, seller, _, _) = setup();

    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&session_id);

    // Try to refund completed session - should fail
    let result = env.try_invoke_contract(
        &contract.address,
        &"refund_session",
        session_id.into_val(&env),
    );

    assert!(result.is_err());

    // Verify error message contains expected panic
    let error = result.unwrap_err();
    let error_str = std::format!("{:?}", error);
    assert!(error_str.contains("refund only allowed for pending sessions"));
}

#[test]
fn refund_session_reverts_if_already_approved() {
    let (env, contract, token_client, _, buyer, seller, _, _) = setup();

    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&session_id);
    contract.approve_session(&session_id);

    // Try to refund approved session - should fail
    let result = env.try_invoke_contract(
        &contract.address,
        &"refund_session",
        session_id.into_val(&env),
    );

    assert!(result.is_err());

    // Verify error message contains expected panic
    let error = result.unwrap_err();
    let error_str = std::format!("{:?}", error);
    assert!(error_str.contains("refund only allowed for pending sessions"));
}

#[test]
fn refund_session_emits_session_refunded_event() {
    let (env, contract, token_client, _, buyer, seller, treasury, contract_id) = setup();

    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &1_000);

    // Buyer refunds
    contract.refund_session(&session_id);

    // Verify SessionRefunded event was emitted
    let events = env.events().all();
    assert_eq!(events.len(), 2); // create_session and refund_session events

    let refund_event = &events[1];
    assert_eq!(refund_event.0, contract_id);
    assert!(std::format!("{:?}", refund_event.1).contains("refunded"));

    // Verify event data contains session_id
    let event_topics = refund_event.1.clone();
    assert_eq!(event_topics.topics.get(1), Some(&session_id.into_val(&env)));
}

#[test]
fn refund_session_requires_buyer_authorization() {
    let (env, contract, token_client, _, buyer, seller, _, _) = setup();

    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &1_000);

    // Try to refund as seller (not buyer) - should fail due to auth
    env.mock_auths(&[]);
    env.mock_auths(&[
        &seller, // Only seller authorized, not buyer
    ]);

    let result = env.try_invoke_contract(
        &contract.address,
        &"refund_session",
        session_id.into_val(&env),
    );

    assert!(result.is_err());
}

#[test]
fn refund_session_after_refund_cannot_refund_again() {
    let (env, contract, token_client, _, buyer, seller, _, _) = setup();

    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &1_000);

    // First refund succeeds
    contract.refund_session(&session_id);

    // Try to refund again - should fail
    let result = env.try_invoke_contract(
        &contract.address,
        &"refund_session",
        session_id.into_val(&env),
    );

    assert!(result.is_err());

    // Verify error message
    let error = result.unwrap_err();
    let error_str = std::format!("{:?}", error);
    assert!(error_str.contains("refund only allowed for pending sessions"));
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
    client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_LEDGERS);
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

    // 4. Advance ledger sequence beyond dispute window
    env.ledger().set(LedgerInfo {
        timestamp: env.ledger().timestamp() + 10000, // Advance time as well
        protocol_version: 20,
        sequence_number: DEFAULT_DISPUTE_WINDOW_LEDGERS + 100,
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

        // Advance ledger sequence beyond dispute window
        env.ledger().set(LedgerInfo {
            timestamp: env.ledger().timestamp() + 10000, // Advance time as well
            protocol_version: 20,
            sequence_number: DEFAULT_DISPUTE_WINDOW_LEDGERS + 100,
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
    fn test_upgrade() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_LEDGERS);

        let admin = Address::generate(&env);
        client.init(&admin);

        let result = client.hello(&symbol_short!("World"));
        assert_eq!(
            result,
            vec![&env, symbol_short!("Hello"), symbol_short!("World")]
        );
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
    contract.initialize(&admin, &treasury, &500);

    (
        env,
        contract,
        token_client,
        asset_client,
        buyer,
        seller,
        treasury,
        admin,
        contract_id,
    )
}

#[test]
fn approve_session_releases_payout_fee_and_event() {
    let (env, contract, token_client, _, buyer, seller, treasury, _admin, contract_id) = setup();

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
fn approve_session_records_buyer_authorization() {
    let (env, contract, token_client, _, buyer, seller, _treasury, _admin, _contract_id) = setup();

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
    client.open_dispute(
        &session_id,
        &payee,
        &Bytes::from_slice(&env, b"Completed but unhappy"),
    );

    let session = client.get_session(&session_id).unwrap();
    assert_eq!(session.status, SessionStatus::Disputed);
    let snapshot = env.to_snapshot();
    let approve_auth = snapshot.auth.0.last().unwrap();
    let auth_debug = std::format!("{:?}", approve_auth);
    assert!(auth_debug.contains("approve_session"));
}

#[test]
fn auto_refund_works_after_dispute_window_expires() {
    let (mut env, contract, token_client, _, buyer, seller, _, contract_id) = setup();

    // Create and complete session
    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&session_id);

    // Verify session is completed
    let session = contract.get_session(&session_id);
    assert!(matches!(session.status, SessionStatus::Completed));

    // Get the dispute window duration (604800 seconds = 7 days)
    let dispute_window = contract.dispute_window_secs();
    let completed_at = session.completed_at;

    // Advance time past the dispute window
    env.ledger()
        .set_timestamp(completed_at + dispute_window + 1);

    // Execute auto-refund
    contract.auto_refund(&session_id);

    // Verify session status is now Refunded
    let session = contract.get_session(&session_id);
    assert!(matches!(session.status, SessionStatus::Refunded));
}

#[test]
fn auto_refund_does_not_trigger_before_window_expires() {
    let (mut env, contract, token_client, _, buyer, seller, _, contract_id) = setup();

    // Create and complete session
    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&session_id);

    // Get the completed timestamp
    let session = contract.get_session(&session_id);
    let completed_at = session.completed_at;
    let dispute_window = contract.dispute_window_secs();

    // Advance time to just before the window expires
    env.ledger()
        .set_timestamp(completed_at + dispute_window - 1);

    // Try to auto-refund - should panic
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        contract.auto_refund(&session_id);
    }));

    assert!(
        result.is_err(),
        "auto_refund should fail before window expires"
    );

    // Verify session is still completed
    let session = contract.get_session(&session_id);
    assert!(matches!(session.status, SessionStatus::Completed));
}

#[test]
fn auto_refund_buyer_receives_full_amount() {
    let (mut env, contract, token_client, _, buyer, seller, _, contract_id) = setup();

    let amount = 1_000;
    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &amount);
    contract.complete_session(&session_id);

    // Record buyer balance before refund
    let buyer_balance_before = token_client.balance(&buyer);
    assert_eq!(buyer_balance_before, 0); // Buyer spent the amount

    // Advance time past dispute window
    let session = contract.get_session(&session_id);
    let completed_at = session.completed_at;
    let dispute_window = contract.dispute_window_secs();
    env.ledger()
        .set_timestamp(completed_at + dispute_window + 1);

    // Execute auto-refund
    contract.auto_refund(&session_id);

    // Verify buyer received full amount back
    let buyer_balance_after = token_client.balance(&buyer);
    assert_eq!(buyer_balance_after, amount);

    // Verify contract balance is 0
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn auto_refund_emits_autorefundexecuted_event() {
    let (mut env, contract, token_client, _, buyer, seller, _, contract_id) = setup();

    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&session_id);

    // Advance time past dispute window
    let session = contract.get_session(&session_id);
    let completed_at = session.completed_at;
    let dispute_window = contract.dispute_window_secs();
    env.ledger()
        .set_timestamp(completed_at + dispute_window + 1);

    // Execute auto-refund
    contract.auto_refund(&session_id);

    // Check for AutoRefundExecuted event
    let events = env.events().all();
    let last_event = events.last().unwrap();

    // Verify event is from our contract
    assert_eq!(last_event.0, contract_id);

    // Verify event topic contains "autorefund"
    let topics_str = std::format!("{:?}", last_event.1);
    assert!(topics_str.contains("autorefund"));

    // Verify event data can be decoded as AutoRefundExecutedEvent
    let event_data: AutoRefundExecutedEvent = last_event.2.clone().unwrap();
    assert_eq!(event_data.session_id, session_id);
    assert_eq!(event_data.amount, 1_000);
}

#[test]
fn session_cannot_be_approved_after_auto_refund() {
    let (mut env, contract, token_client, _, buyer, seller, _, _) = setup();

    let session_id = contract.create_session(&buyer, &seller, &token_client.address, &1_000);
    contract.complete_session(&session_id);

    // Advance time past dispute window
    let session = contract.get_session(&session_id);
    let completed_at = session.completed_at;
    let dispute_window = contract.dispute_window_secs();
    env.ledger()
        .set_timestamp(completed_at + dispute_window + 1);

    // Execute auto-refund
    contract.auto_refund(&session_id);

    // Try to approve the session - should fail
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        contract.approve_session(&session_id);
    }));

    assert!(
        result.is_err(),
        "approve_session should fail after auto-refund"
    );

    // Verify session is still in Refunded state
    let session = contract.get_session(&session_id);
    assert!(matches!(session.status, SessionStatus::Refunded));
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
    let init_event = events
        .iter()
        .find(|e| std::format!("{:?}", e.1).contains("init"))
        .unwrap();
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
    assert!(matches!(
        contract.get_session(&1).status,
        SessionStatus::Pending
    ));

    contract.complete_session(&1);
    assert!(matches!(
        contract.get_session(&1).status,
        SessionStatus::Completed
    ));

    contract.approve_session(&1);
    assert!(matches!(
        contract.get_session(&1).status,
        SessionStatus::Approved
    ));

    assert_eq!(token_client.balance(&buyer), 0);
    assert_eq!(token_client.balance(&seller), 950);
    assert_eq!(token_client.balance(&treasury), 50);
    assert_eq!(token_client.balance(&contract_id), 0);

    let events = env.events().all();
    let topics: Vec<String> = events.iter().map(|e| std::format!("{:?}", e.1)).collect();
    let completed_idx = topics.iter().position(|t| t.contains("completed")).unwrap();
    let fee_idx = topics
        .iter()
        .position(|t| t.contains("fee_deducted"))
        .unwrap();
    let approved_idx = topics.iter().position(|t| t.contains("approved")).unwrap();

    assert!(
        completed_idx < fee_idx,
        "completed event should come before fee_deducted"
    );
    assert!(
        fee_idx < approved_idx,
        "fee_deducted event should come before approved"
    );
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
    let locked_event = events
        .iter()
        .find(|e| std::format!("{:?}", e.1).contains("locked"))
        .unwrap();
    assert_eq!(locked_event.0, contract_id);
}

// ============================================================================
// Test: Dispute Window Configuration
// ============================================================================

#[test]
fn test_get_dispute_window_returns_default() {
    let (env, contract, _, _, _, _, _, _, _) = setup_with_admin();
    
    // Should return default value of 1000 ledgers
    let window = contract.get_dispute_window();
    assert_eq!(window, DEFAULT_DISPUTE_WINDOW_LEDGERS);
}

#[test]
fn test_set_dispute_window_updates_value() {
    let (env, contract, _, _, _, _, _, _, admin) = setup_with_admin();
    
    // Set new dispute window
    let new_window: u32 = 2000;
    contract.set_dispute_window(&new_window);
    
    // Verify it was updated
    let window = contract.get_dispute_window();
    assert_eq!(window, new_window);
}

#[test]
fn test_set_dispute_window_emits_event() {
    let (env, contract, _, _, _, _, _, _, admin) = setup_with_admin();
    
    let old_window = contract.get_dispute_window();
    let new_window: u32 = 1500;
    
    contract.set_dispute_window(&new_window);
    
    // Check that DisputeWindowUpdated event was emitted
    let events = env.events().all();
    let event = events.iter().find(|e| {
        std::format!("{:?}", e.1).contains("DisputeWindowUpdated")
    });
    
    assert!(event.is_some(), "DisputeWindowUpdated event should be emitted");
}

#[test]
#[should_panic(expected = "InvalidDisputeWindow")]
fn test_set_dispute_window_rejects_too_small() {
    let (env, contract, _, _, _, _, _, _, admin) = setup_with_admin();
    
    // Try to set window below minimum (10 ledgers)
    let invalid_window: u32 = 5;
    contract.set_dispute_window(&invalid_window);
}

#[test]
#[should_panic(expected = "InvalidDisputeWindow")]
fn test_set_dispute_window_rejects_too_large() {
    let (env, contract, _, _, _, _, _, _, admin) = setup_with_admin();
    
    // Try to set window above maximum (100,000 ledgers)
    let invalid_window: u32 = 150_000;
    contract.set_dispute_window(&invalid_window);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_set_dispute_window_requires_admin() {
    let (env, contract, _, _, buyer, _, _, _, admin) = setup_with_admin();
    
    // Try to set dispute window as non-admin (should fail)
    env.mock_all_auths_allowing_non_root_auth();
    let new_window: u32 = 2000;
    
    // This should panic because buyer is not admin
    contract.set_dispute_window(&new_window);
}

#[test]
fn test_set_dispute_window_accepts_minimum_value() {
    let (env, contract, _, _, _, _, _, _, admin) = setup_with_admin();
    
    // Set to minimum allowed value
    let min_window: u32 = DISPUTE_WINDOW_MIN_LEDGERS;
    contract.set_dispute_window(&min_window);
    
    let window = contract.get_dispute_window();
    assert_eq!(window, min_window);
}

#[test]
fn test_set_dispute_window_accepts_maximum_value() {
    let (env, contract, _, _, _, _, _, _, admin) = setup_with_admin();
    
    // Set to maximum allowed value
    let max_window: u32 = DISPUTE_WINDOW_MAX_LEDGERS;
    contract.set_dispute_window(&max_window);
    
    let window = contract.get_dispute_window();
    assert_eq!(window, max_window);
}

#[test]
fn test_dispute_window_persists_across_calls() {
    let (env, contract, _, _, _, _, _, _, admin) = setup_with_admin();
    
    // Set dispute window
    let new_window: u32 = 3000;
    contract.set_dispute_window(&new_window);
    
    // Verify it persists
    let window1 = contract.get_dispute_window();
    let window2 = contract.get_dispute_window();
    
    assert_eq!(window1, new_window);
    assert_eq!(window2, new_window);
}
