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
