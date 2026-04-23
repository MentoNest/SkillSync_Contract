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
) {
    setup_with_fee(500) // Default 5% fee
}

fn setup_with_fee(fee_bps: u32) -> (
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

    let contract_id = env.register_contract(None, CoreContract);
    let contract = CoreContractClient::new(&env, &contract_id);
    contract.initialize(&treasury, &fee_bps);

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

fn mint_and_approve(
    asset_client: &StellarAssetClient<'static>,
    buyer: &Address,
    amount: i128,
) {
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
    contract.complete_session(&session_id);

    // Approve session
    contract.approve_session(&session_id);

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
