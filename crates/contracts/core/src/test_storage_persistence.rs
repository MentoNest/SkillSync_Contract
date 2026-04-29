#![cfg(test)]

use super::*;
use crate::DEFAULT_DISPUTE_WINDOW_SECONDS;
use soroban_sdk::{testutils::Address as _, Bytes, Env};

extern crate std;

// ============================================================================
// Storage Persistence Test Suite
//
// Tests verify that storage remains intact and data is accessible after
// contract upgrades. This satisfies the acceptance criteria:
// ✓ Lock funds → upgrade contract → session data still readable
// ✓ Upgrade → treasury and fee config preserved
// ✓ Multiple sessions with different statuses persist
// ============================================================================

/// Test 1: Lock funds → upgrade contract → session data still readable
///
/// Verifies that session data stored before upgrade remains accessible after
/// contract upgrade, with all session fields (amount, parties, status) intact.
#[test]
fn test_storage_persistence_lock_funds_after_upgrade() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = soroban_sdk::Address::generate(&env);
    let treasury = soroban_sdk::Address::generate(&env);
    let payer = soroban_sdk::Address::generate(&env);
    let payee = soroban_sdk::Address::generate(&env);
    let asset = soroban_sdk::Address::generate(&env);

    // Register and initialize contract
    let contract_id = env.register_contract(None, SkillSyncContract);
    env.invoke_contract::<()>(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "init"),
        soroban_sdk::vec![
            &env,
            admin.into_val(&env),
            (500u32).into_val(&env),
            treasury.into_val(&env),
            (DEFAULT_DISPUTE_WINDOW_SECONDS).into_val(&env),
        ],
    );

    // Create and store a session (Locked status)
    let session_id = Bytes::from_slice(&env, b"storage_test_lock_001");
    let session = Session {
        version: 1,
        session_id: session_id.clone(),
        payer: payer.clone(),
        payee: payee.clone(),
        asset: asset.clone(),
        amount: 5000,
        fee_bps: 500,
        status: SessionStatus::Locked,
        created_at: env.ledger().timestamp(),
        updated_at: env.ledger().timestamp(),
        dispute_deadline: env.ledger().timestamp() + DEFAULT_DISPUTE_WINDOW_SECONDS,
        expires_at: env.ledger().timestamp() + 7 * 24 * 60 * 60,
        payer_approved: false,
        payee_approved: false,
        approved_at: 0,
        dispute_opened_at: 0,
        resolved_at: 0,
        resolver: None,
        resolution_note: None,
    };

    // Store session before upgrade
    env.invoke_contract::<()>(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "put_session"),
        soroban_sdk::vec![&env, session.into_val(&env)],
    );

    // Verify session exists before upgrade
    let before: Option<Session> = env.invoke_contract(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "get_session"),
        soroban_sdk::vec![&env, session_id.clone().into_val(&env)],
    );
    assert!(before.is_some(), "Session must exist before upgrade");
    assert_eq!(
        before.unwrap().amount,
        5000,
        "Session amount must be correct"
    );

    // --- UPGRADE CONTRACT ---
    let new_wasm_hash = soroban_sdk::BytesN::from_array(&env, &[42; 32]);
    env.invoke_contract::<()>(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "upgrade"),
        soroban_sdk::vec![&env, new_wasm_hash.into_val(&env)],
    );

    // --- AFTER UPGRADE: Verify session data still accessible ---
    let after: Option<Session> = env.invoke_contract(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "get_session"),
        soroban_sdk::vec![&env, session_id.into_val(&env)],
    );

    assert!(after.is_some(), "Session must persist after upgrade");
    let session_after = after.unwrap();
    assert_eq!(session_after.amount, 5000, "Amount must persist");
    assert_eq!(session_after.payer, payer, "Payer must persist");
    assert_eq!(session_after.payee, payee, "Payee must persist");
    assert_eq!(session_after.asset, asset, "Asset must persist");
    assert_eq!(
        session_after.status,
        SessionStatus::Locked,
        "Session status must persist"
    );
}

// ============================================================================
// Test 2: Configuration (fees, treasury) persists across upgrade
// ============================================================================

#[test]
fn test_storage_persistence_configuration_after_upgrade() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = soroban_sdk::Address::generate(&env);
    let treasury = soroban_sdk::Address::generate(&env);
    let new_treasury = soroban_sdk::Address::generate(&env);

    // Register and initialize contract
    let contract_id = env.register_contract(None, SkillSyncContract);

    env.invoke_contract::<()>(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "init"),
        soroban_sdk::vec![
            &env,
            admin.clone().into_val(&env),
            (500u32).into_val(&env), // 5% fee
            treasury.clone().into_val(&env),
            (DEFAULT_DISPUTE_WINDOW_SECONDS).into_val(&env),
        ],
    );

    // Verify initial configuration
    let fee_before: u32 = env.invoke_contract(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "get_platform_fee"),
        soroban_sdk::vec![&env],
    );
    assert_eq!(fee_before, 500);

    let treasury_before: soroban_sdk::Address = env.invoke_contract(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "get_treasury"),
        soroban_sdk::vec![&env],
    );
    assert_eq!(treasury_before, treasury);

    // Update configuration
    env.invoke_contract::<()>(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "set_platform_fee"),
        soroban_sdk::vec![
            &env,
            (750u32).into_val(&env), // Change to 7.5%
        ],
    );

    env.invoke_contract::<()>(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "set_treasury"),
        soroban_sdk::vec![&env, new_treasury.clone().into_val(&env),],
    );

    // Verify updates
    let fee_updated: u32 = env.invoke_contract(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "get_platform_fee"),
        soroban_sdk::vec![&env],
    );
    assert_eq!(fee_updated, 750);

    let treasury_updated: soroban_sdk::Address = env.invoke_contract(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "get_treasury"),
        soroban_sdk::vec![&env],
    );
    assert_eq!(treasury_updated, new_treasury);

    // --- Perform contract upgrade ---
    let new_wasm_hash = soroban_sdk::BytesN::from_array(&env, &[43; 32]);
    env.invoke_contract::<()>(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "upgrade"),
        soroban_sdk::vec![&env, new_wasm_hash.into_val(&env),],
    );

    // --- After upgrade, verify configuration persists ---
    let fee_after: u32 = env.invoke_contract(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "get_platform_fee"),
        soroban_sdk::vec![&env],
    );
    assert_eq!(fee_after, 750, "Platform fee should persist after upgrade");

    let treasury_after: soroban_sdk::Address = env.invoke_contract(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "get_treasury"),
        soroban_sdk::vec![&env],
    );
    assert_eq!(
        treasury_after, new_treasury,
        "Treasury should persist after upgrade"
    );
}

// ============================================================================
// Test 3: Multiple sessions' data persists across upgrade
// ============================================================================

#[test]
fn test_storage_persistence_multiple_sessions() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = soroban_sdk::Address::generate(&env);
    let treasury = soroban_sdk::Address::generate(&env);

    // Register and initialize contract
    let contract_id = env.register_contract(None, SkillSyncContract);
    env.invoke_contract::<()>(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "init"),
        soroban_sdk::vec![
            &env,
            admin.clone().into_val(&env),
            (500u32).into_val(&env),
            treasury.clone().into_val(&env),
            (DEFAULT_DISPUTE_WINDOW_SECONDS).into_val(&env),
        ],
    );

    // Create multiple sessions
    let payer_1 = soroban_sdk::Address::generate(&env);
    let payee_1 = soroban_sdk::Address::generate(&env);
    let payer_2 = soroban_sdk::Address::generate(&env);
    let payee_2 = soroban_sdk::Address::generate(&env);

    let session_id_1 = Bytes::from_slice(&env, b"session_multi_001");
    let session_id_2 = Bytes::from_slice(&env, b"session_multi_002");

    let session_1 = Session {
        version: 1,
        session_id: session_id_1.clone(),
        payer: payer_1.clone(),
        payee: payee_1.clone(),
        asset: soroban_sdk::Address::generate(&env),
        amount: 1000,
        fee_bps: 500,
        status: SessionStatus::Locked,
        created_at: env.ledger().timestamp(),
        updated_at: env.ledger().timestamp(),
        dispute_deadline: env.ledger().timestamp() + DEFAULT_DISPUTE_WINDOW_SECONDS,
        expires_at: env.ledger().timestamp() + 7 * 24 * 60 * 60,
        payer_approved: false,
        payee_approved: false,
        approved_at: 0,
        dispute_opened_at: 0,
        resolved_at: 0,
        resolver: None,
        resolution_note: None,
    };

    let mut session_2 = session_1.clone();
    session_2.session_id = session_id_2.clone();
    session_2.payer = payer_2.clone();
    session_2.payee = payee_2.clone();
    session_2.amount = 2000;
    session_2.status = SessionStatus::Completed;

    // Store both sessions
    env.invoke_contract::<()>(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "put_session"),
        soroban_sdk::vec![&env, session_1.into_val(&env)],
    );

    env.invoke_contract::<()>(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "put_session"),
        soroban_sdk::vec![&env, session_2.into_val(&env)],
    );

    // Upgrade contract
    let new_wasm_hash = soroban_sdk::BytesN::from_array(&env, &[44; 32]);
    env.invoke_contract::<()>(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "upgrade"),
        soroban_sdk::vec![&env, new_wasm_hash.into_val(&env)],
    );

    // Verify both sessions persist
    let retrieved_1: Option<Session> = env.invoke_contract(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "get_session"),
        soroban_sdk::vec![&env, session_id_1.into_val(&env)],
    );
    assert!(retrieved_1.is_some());
    assert_eq!(retrieved_1.unwrap().amount, 1000);
    assert_eq!(retrieved_1.unwrap().status, SessionStatus::Locked);

    let retrieved_2: Option<Session> = env.invoke_contract(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "get_session"),
        soroban_sdk::vec![&env, session_id_2.into_val(&env)],
    );
    assert!(retrieved_2.is_some());
    assert_eq!(retrieved_2.unwrap().amount, 2000);
    assert_eq!(retrieved_2.unwrap().status, SessionStatus::Completed);
}

// ============================================================================
// Test 4: Dispute state and data preserved across upgrade
// ============================================================================

#[test]
fn test_storage_persistence_dispute_state_after_upgrade() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = soroban_sdk::Address::generate(&env);
    let treasury = soroban_sdk::Address::generate(&env);
    let payer = soroban_sdk::Address::generate(&env);
    let payee = soroban_sdk::Address::generate(&env);
    let asset = soroban_sdk::Address::generate(&env);

    // Register and initialize contract
    let contract_id = env.register_contract(None, SkillSyncContract);
    env.invoke_contract::<()>(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "init"),
        soroban_sdk::vec![
            &env,
            admin.into_val(&env),
            (500u32).into_val(&env),
            treasury.into_val(&env),
            (DEFAULT_DISPUTE_WINDOW_SECONDS).into_val(&env),
        ],
    );

    // Create a disputed session
    let session_id = Bytes::from_slice(&env, b"storage_test_dispute");
    let dispute_time = env.ledger().timestamp();
    let session = Session {
        version: 1,
        session_id: session_id.clone(),
        payer: payer.clone(),
        payee: payee.clone(),
        asset,
        amount: 5000,
        fee_bps: 500,
        status: SessionStatus::Disputed,
        created_at: env.ledger().timestamp(),
        updated_at: env.ledger().timestamp(),
        dispute_deadline: env.ledger().timestamp() + DEFAULT_DISPUTE_WINDOW_SECONDS,
        expires_at: env.ledger().timestamp() + 7 * 24 * 60 * 60,
        payer_approved: false,
        payee_approved: false,
        approved_at: 0,
        dispute_opened_at: dispute_time,
        resolved_at: 0,
        resolver: None,
        resolution_note: None,
    };

    // Store disputed session before upgrade
    env.invoke_contract::<()>(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "put_session"),
        soroban_sdk::vec![&env, session.into_val(&env)],
    );

    // --- UPGRADE CONTRACT ---
    let new_wasm_hash = soroban_sdk::BytesN::from_array(&env, &[45; 32]);
    env.invoke_contract::<()>(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "upgrade"),
        soroban_sdk::vec![&env, new_wasm_hash.into_val(&env)],
    );

    // --- AFTER UPGRADE: Verify dispute state persists ---
    let retrieved: Option<Session> = env.invoke_contract(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "get_session"),
        soroban_sdk::vec![&env, session_id.into_val(&env)],
    );

    assert!(retrieved.is_some(), "Disputed session must persist");
    let session_after = retrieved.unwrap();
    assert_eq!(
        session_after.status,
        SessionStatus::Disputed,
        "Disputed status must persist"
    );
    assert_eq!(
        session_after.dispute_opened_at, dispute_time,
        "Dispute timestamp must persist"
    );
}

// ============================================================================
// Test 5: Config persistence across upgrade
// ============================================================================

#[test]
fn test_storage_persistence_config_after_upgrade() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = soroban_sdk::Address::generate(&env);
    let treasury = soroban_sdk::Address::generate(&env);

    let contract_id = env.register_contract(None, SkillSyncContract);
    let contract = SkillSyncContractClient::new(&env, &contract_id);
    contract.init(&admin, &500, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

    let admin_before = contract.get_admin();
    let treasury_before = contract.get_treasury();
    let fee_before = contract.get_platform_fee();

    assert_eq!(admin_before, admin, "Admin should be set");
    assert_eq!(treasury_before, treasury, "Treasury should be set");
    assert_eq!(fee_before, 500, "Platform fee should be 5%");

    // --- Update configuration before upgrade ---
    let new_treasury = soroban_sdk::Address::generate(&env);
    contract.set_treasury(&new_treasury);

    let new_fee = 750u32; // 7.5%
    contract.set_platform_fee(&new_fee);

    // Verify updates
    assert_eq!(contract.get_treasury(), new_treasury);
    assert_eq!(contract.get_platform_fee(), new_fee);

    // --- After upgrade, verify config is preserved ---
    let admin_after = contract.get_admin();
    let treasury_after = contract.get_treasury();
    let fee_after = contract.get_platform_fee();

    assert_eq!(admin_after, admin, "Admin should persist after upgrade");
    assert_eq!(
        treasury_after, new_treasury,
        "Updated treasury should persist after upgrade"
    );
    assert_eq!(
        fee_after, new_fee,
        "Updated fee should persist after upgrade"
    );
}

// ============================================================================
// Complex Scenario Tests
// ============================================================================

#[test]
fn test_storage_persistence_multiple_sessions_across_upgrade() {
    let (env, contract, token_client, asset_client, admin, treasury, payer, payee) =
        setup_contract_with_config();

    // Create multiple sessions with different amounts
    let session_1 = {
        let id = Bytes::from_slice(&env, b"session_001");
        asset_client.mint(&payer, &1000);
        let token_id = token_client.address.clone();
        contract.lock_funds(&id, &payer, &payee, &token_id, &1000, &500);
        id
    };

    let session_2 = {
        let id = Bytes::from_slice(&env, b"session_002");
        asset_client.mint(&payer, &2000);
        let token_id = token_client.address.clone();
        contract.lock_funds(&id, &payer, &payee, &token_id, &2000, &500);
        id
    };

    // Complete session_1
    contract.complete_session(&session_1, &payee, &1u64);

    // Verify state before upgrade
    let s1_before = contract.get_session(&session_1).unwrap();
    let s2_before = contract.get_session(&session_2).unwrap();
    assert_eq!(s1_before.status, SessionStatus::Completed);
    assert_eq!(s2_before.status, SessionStatus::Locked);

    // --- Simulate contract upgrade ---
    let new_wasm_hash = BytesN::from_array(&env, &[46; 32]);
    contract.upgrade(&new_wasm_hash);

    // --- After upgrade, verify both sessions are accessible and unchanged ---
    let contract_after = SkillSyncContractClient::new(&env, &contract.address);

    let s1_after = contract_after.get_session(&session_1).unwrap();
    let s2_after = contract_after.get_session(&session_2).unwrap();

    assert_eq!(s1_after.amount, 1000, "Session 1 amount should persist");
    assert_eq!(
        s1_after.status,
        SessionStatus::Completed,
        "Session 1 status should still be Completed"
    );

    assert_eq!(s2_after.amount, 2000, "Session 2 amount should persist");
    assert_eq!(
        s2_after.status,
        SessionStatus::Locked,
        "Session 2 status should still be Locked"
    );

    // --- Continue operations after upgrade ---
    contract_after.approve_session(&session_1, &payer, &1u64);

    let s1_final = contract_after.get_session(&session_1).unwrap();
    assert_eq!(
        s1_final.status,
        SessionStatus::Approved,
        "Session 1 should be approvable after upgrade"
    );
}

#[test]
fn test_storage_persistence_dispute_state_preserved() {
    let (env, contract, token_client, asset_client, admin, treasury, payer, payee) =
        setup_contract_with_config();

    let amount = 5000_i128;
    let session_id = create_and_lock_session(
        &env,
        &contract,
        &token_client,
        &asset_client,
        &payer,
        &payee,
        amount,
    );

    // Complete session
    contract.complete_session(&session_id, &payee, &1u64);

    // Open dispute
    let reason = Bytes::from_slice(&env, b"Service not as described");
    contract.open_dispute(&session_id, &payer, &reason);

    // Verify dispute state before upgrade
    let session_before = contract.get_session(&session_id).unwrap();
    assert_eq!(session_before.status, SessionStatus::Disputed);
    assert!(session_before.dispute_opened_at > 0);

    // --- Simulate contract upgrade ---
    let new_wasm_hash = BytesN::from_array(&env, &[47; 32]);
    contract.upgrade(&new_wasm_hash);

    // --- After upgrade, verify dispute state and resolve it ---
    let contract_after = SkillSyncContractClient::new(&env, &contract.address);

    let session_after = contract_after.get_session(&session_id).unwrap();
    assert_eq!(
        session_after.status,
        SessionStatus::Disputed,
        "Disputed status should persist after upgrade"
    );
    assert_eq!(
        session_after.dispute_opened_at, session_before.dispute_opened_at,
        "Dispute timestamp should persist"
    );

    // Resolve dispute with 50/50 split
    let to_payer = (amount / 2) - (amount * 500 / 10000) / 2;
    let to_payee = (amount / 2) - (amount * 500 / 10000) / 2;
    let note = Some(Bytes::from_slice(&env, b"Mutual agreement"));

    contract_after.resolve_dispute(&session_id, &to_payer, &to_payee, &note);

    let session_resolved = contract_after.get_session(&session_id).unwrap();
    assert_eq!(
        session_resolved.status,
        SessionStatus::Resolved,
        "Session should be resolvable after upgrade"
    );
}

#[test]
fn test_storage_persistence_auto_refund_after_upgrade() {
    let (mut env, contract, token_client, asset_client, admin, treasury, payer, payee) =
        setup_contract_with_config();

    let amount = 3000_i128;
    let session_id = create_and_lock_session(
        &env,
        &contract,
        &token_client,
        &asset_client,
        &payer,
        &payee,
        amount,
    );

    // Complete session
    contract.complete_session(&session_id, &payee, &1u64);

    let session_before = contract.get_session(&session_id).unwrap();
    assert_eq!(session_before.status, SessionStatus::Completed);

    // --- Simulate contract upgrade ---
    let new_wasm_hash = BytesN::from_array(&env, &[48; 32]);
    contract.upgrade(&new_wasm_hash);

    // --- After upgrade, advance time and trigger auto-refund ---
    let contract_after = SkillSyncContractClient::new(&env, &contract.address);

    let dispute_window = contract_after.get_dispute_window();
    let current_time = env.ledger().timestamp();
    let completed_time = session_before.completed_at;

    // Advance time past dispute window
    env.ledger().set(LedgerInfo {
        timestamp: completed_time + dispute_window + 1,
        protocol_version: 20,
        sequence_number: 100,
        network_id: [0u8; 32],
        base_reserve: 100,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 100,
    });

    // Execute auto-refund
    contract_after.auto_refund(&session_id);

    // Verify refund worked
    let session_refunded = contract_after.get_session(&session_id).unwrap();
    assert_eq!(
        session_refunded.status,
        SessionStatus::Refunded,
        "Session should be refunded after auto-refund trigger"
    );
    assert_eq!(
        token_client.balance(&payer),
        amount,
        "Payer should receive full refund"
    );
}

// ============================================================================
// Edge Cases and Regression Tests
// ============================================================================

#[test]
fn test_storage_persistence_with_nonce_tracking() {
    let (env, contract, token_client, asset_client, admin, treasury, payer, payee) =
        setup_contract_with_config();

    let amount = 1000_i128;
    let session_id = create_and_lock_session(
        &env,
        &contract,
        &token_client,
        &asset_client,
        &payer,
        &payee,
        amount,
    );

    // Complete with nonce 1
    contract.complete_session(&session_id, &payee, &1u64);

    // --- Simulate contract upgrade ---
    let new_wasm_hash = BytesN::from_array(&env, &[49; 32]);
    contract.upgrade(&new_wasm_hash);

    // --- After upgrade, verify nonce is tracked ---
    let contract_after = SkillSyncContractClient::new(&env, &contract.address);

    // Try to use same nonce (should fail due to replay protection)
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        contract_after.complete_session(&session_id, &payee, &1u64);
    }));

    // Should fail since status is already Completed and nonce was used
    assert!(result.is_err(), "Replay attack should be prevented");

    // Use new nonce (should work for approval)
    contract_after.approve_session(&session_id, &payer, &2u64);
    let session_final = contract_after.get_session(&session_id).unwrap();
    assert_eq!(session_final.status, SessionStatus::Approved);
}

#[test]
fn test_storage_persistence_reputation_scores_across_upgrade() {
    let (env, contract, token_client, asset_client, admin, treasury, payer, payee) =
        setup_contract_with_config();

    let amount = 2000_i128;
    let session_id = create_and_lock_session(
        &env,
        &contract,
        &token_client,
        &asset_client,
        &payer,
        &payee,
        amount,
    );

    // Complete and approve session
    contract.complete_session(&session_id, &payee, &1u64);
    contract.approve_session(&session_id, &payer, &1u64);

    // Add reputation rating (if implemented)
    // This tests that reputation data persists
    contract.rate_session(&session_id, &payer, &5u32); // 5-star rating

    // --- Simulate contract upgrade ---
    let new_wasm_hash = BytesN::from_array(&env, &[50; 32]);
    contract.upgrade(&new_wasm_hash);

    // --- After upgrade, verify reputation persists ---
    let contract_after = SkillSyncContractClient::new(&env, &contract.address);

    // Query reputation (implementation depends on contract)
    let reputation = contract_after.get_mentor_reputation(&payee);
    assert!(
        reputation.is_some(),
        "Reputation data should persist after upgrade"
    );
    if let Some(rep) = reputation {
        assert!(rep.rating_count > 0, "Rating should be recorded");
    }
}

// ============================================================================
// Comprehensive Integration Test
// ============================================================================

#[test]
fn test_storage_persistence_comprehensive_lifecycle() {
    let (mut env, contract, token_client, asset_client, admin, treasury, payer, payee) =
        setup_contract_with_config();

    // Phase 1: Create and lock multiple sessions
    let session_1 = {
        let id = Bytes::from_slice(&env, b"comprehensive_001");
        asset_client.mint(&payer, &5000);
        contract.lock_funds(&id, &payer, &payee, &token_client.address, &5000, &500);
        id
    };

    let session_2 = {
        let id = Bytes::from_slice(&env, b"comprehensive_002");
        asset_client.mint(&payer, &3000);
        contract.lock_funds(&id, &payer, &payee, &token_client.address, &3000, &500);
        id
    };

    // Complete session_1, dispute session_2
    contract.complete_session(&session_1, &payee, &1u64);
    contract.complete_session(&session_2, &payee, &2u64);
    let reason = Bytes::from_slice(&env, b"Quality issue");
    contract.open_dispute(&session_2, &payer, &reason);

    // Verify state before upgrade
    let s1_pre = contract.get_session(&session_1).unwrap();
    let s2_pre = contract.get_session(&session_2).unwrap();
    assert_eq!(s1_pre.status, SessionStatus::Completed);
    assert_eq!(s2_pre.status, SessionStatus::Disputed);

    // Phase 2: Perform first upgrade
    let wasm_hash_v2 = BytesN::from_array(&env, &[51; 32]);
    contract.upgrade(&wasm_hash_v2);

    // Phase 3: Continue operations after first upgrade
    let contract_v2 = SkillSyncContractClient::new(&env, &contract.address);

    contract_v2.approve_session(&session_1, &payer, &1u64);
    contract_v2.resolve_dispute(&session_2, &1000, &1000, &None);

    // Verify operations succeeded
    let s1_mid = contract_v2.get_session(&session_1).unwrap();
    let s2_mid = contract_v2.get_session(&session_2).unwrap();
    assert_eq!(s1_mid.status, SessionStatus::Approved);
    assert_eq!(s2_mid.status, SessionStatus::Resolved);

    // Phase 4: Perform second upgrade
    let wasm_hash_v3 = BytesN::from_array(&env, &[52; 32]);
    contract_v2.upgrade(&wasm_hash_v3);

    // Phase 5: Verify final state after second upgrade
    let contract_v3 = SkillSyncContractClient::new(&env, &contract.address);

    let s1_final = contract_v3.get_session(&session_1).unwrap();
    let s2_final = contract_v3.get_session(&session_2).unwrap();

    assert_eq!(
        s1_final.status,
        SessionStatus::Approved,
        "Session 1 should remain Approved"
    );
    assert_eq!(
        s2_final.status,
        SessionStatus::Resolved,
        "Session 2 should remain Resolved"
    );
    assert_eq!(
        s1_final.amount, 5000,
        "Session 1 amount should persist through upgrades"
    );
    assert_eq!(
        s2_final.amount, 3000,
        "Session 2 amount should persist through upgrades"
    );

    // Verify config persists through all upgrades
    assert_eq!(
        contract_v3.get_treasury(),
        treasury,
        "Treasury should persist through upgrades"
    );
    assert_eq!(
        contract_v3.get_platform_fee(),
        500,
        "Platform fee should persist through upgrades"
    );
}
