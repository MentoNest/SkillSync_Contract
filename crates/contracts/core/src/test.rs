#![cfg(test)]

extern crate std;

use crate::{AutoRefundExecutedEvent, CoreContract, CoreContractClient, SessionStatus};
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
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
    env.ledger().set_timestamp(completed_at + dispute_window + 1);

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

    assert!(result.is_err(), "auto_refund should fail before window expires");

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
    env.ledger().set_timestamp(completed_at + dispute_window + 1);

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
    env.ledger().set_timestamp(completed_at + dispute_window + 1);

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
    env.ledger().set_timestamp(completed_at + dispute_window + 1);

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
