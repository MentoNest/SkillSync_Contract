#![cfg(test)]

use super::*;
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env};

#[test]
fn test_init() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RefundPolicyContract);
    let client = RefundPolicyContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let cutoff_secs = 3600; // 1 hour
    let late_bps = 5000; // 50%

    client.init(&admin, &cutoff_secs, &late_bps);

    // Verify admin is set
    let stored_admin = client.get_admin();
    assert_eq!(stored_admin, admin);

    // Verify policy is set
    let policy = client.get_policy();
    assert_eq!(policy.cutoff_secs, cutoff_secs);
    assert_eq!(policy.late_bps, late_bps);
}

#[test]
#[should_panic(expected = "late_bps must be <= 10000")]
fn test_init_invalid_late_bps() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RefundPolicyContract);
    let client = RefundPolicyContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let cutoff_secs = 3600;
    let late_bps = 10001; // Invalid: > 10000

    client.init(&admin, &cutoff_secs, &late_bps);
}

#[test]
fn test_full_refund_before_cutoff() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RefundPolicyContract);
    let client = RefundPolicyContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let cutoff_secs = 3600; // 1 hour cutoff
    let late_bps = 5000; // 50% after cutoff

    client.init(&admin, &cutoff_secs, &late_bps);

    let now = 1000;
    let session_start = 5000; // 4000 seconds from now
    let amount = 1000;

    // Time until start (4000) > cutoff_secs (3600), so full refund
    let refund = client.compute_refund(&now, &session_start, &amount);
    assert_eq!(refund, amount);
}

#[test]
fn test_partial_refund_after_cutoff() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RefundPolicyContract);
    let client = RefundPolicyContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let cutoff_secs = 3600; // 1 hour cutoff
    let late_bps = 5000; // 50% after cutoff

    client.init(&admin, &cutoff_secs, &late_bps);

    let now = 2000;
    let session_start = 5000; // 3000 seconds from now
    let amount = 1000;

    // Time until start (3000) <= cutoff_secs (3600), so partial refund
    // Expected: 1000 * 5000 / 10000 = 500
    let refund = client.compute_refund(&now, &session_start, &amount);
    assert_eq!(refund, 500);
}

#[test]
fn test_no_refund_after_start() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RefundPolicyContract);
    let client = RefundPolicyContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let cutoff_secs = 3600;
    let late_bps = 5000;

    client.init(&admin, &cutoff_secs, &late_bps);

    let now = 6000;
    let session_start = 5000; // Session already started
    let amount = 1000;

    // Session has started, so no refund
    let refund = client.compute_refund(&now, &session_start, &amount);
    assert_eq!(refund, 0);
}

#[test]
fn test_exact_cutoff_boundary() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RefundPolicyContract);
    let client = RefundPolicyContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let cutoff_secs = 3600;
    let late_bps = 5000;

    client.init(&admin, &cutoff_secs, &late_bps);

    // Exactly at cutoff boundary
    let now = 1000;
    let session_start = 4600; // Exactly 3600 seconds from now
    let amount = 1000;

    // Time until start (3600) == cutoff_secs (3600), so partial refund
    // (since condition is > cutoff_secs for full refund)
    let refund = client.compute_refund(&now, &session_start, &amount);
    assert_eq!(refund, 500);
}

#[test]
fn test_exact_start_boundary() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RefundPolicyContract);
    let client = RefundPolicyContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let cutoff_secs = 3600;
    let late_bps = 5000;

    client.init(&admin, &cutoff_secs, &late_bps);

    // Exactly at session start
    let now = 5000;
    let session_start = 5000;
    let amount = 1000;

    // Session just started, so no refund
    let refund = client.compute_refund(&now, &session_start, &amount);
    assert_eq!(refund, 0);
}

#[test]
fn test_set_policy() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RefundPolicyContract);
    let client = RefundPolicyContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let cutoff_secs = 3600;
    let late_bps = 5000;

    client.init(&admin, &cutoff_secs, &late_bps);

    // Update policy
    let new_cutoff_secs = 7200; // 2 hours
    let new_late_bps = 2500; // 25%

    // Mock admin auth - in tests, we need to properly authorize
    env.mock_all_auths();
    client.set_policy(&new_cutoff_secs, &new_late_bps);

    let policy = client.get_policy();
    assert_eq!(policy.cutoff_secs, new_cutoff_secs);
    assert_eq!(policy.late_bps, new_late_bps);
}

#[test]
#[should_panic(expected = "late_bps must be <= 10000")]
fn test_set_policy_invalid_late_bps() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RefundPolicyContract);
    let client = RefundPolicyContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let cutoff_secs = 3600;
    let late_bps = 5000;

    client.init(&admin, &cutoff_secs, &late_bps);

    // Try to set invalid late_bps
    let new_cutoff_secs = 7200;
    let new_late_bps = 10001; // Invalid

    env.mock_all_auths();
    client.set_policy(&new_cutoff_secs, &new_late_bps);
}

#[test]
fn test_zero_late_bps() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RefundPolicyContract);
    let client = RefundPolicyContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let cutoff_secs = 3600;
    let late_bps = 0; // 0% refund after cutoff

    client.init(&admin, &cutoff_secs, &late_bps);

    let now = 2000;
    let session_start = 5000; // 3000 seconds from now (after cutoff)
    let amount = 1000;

    // After cutoff with 0% late_bps, should return 0
    let refund = client.compute_refund(&now, &session_start, &amount);
    assert_eq!(refund, 0);
}

#[test]
fn test_full_late_bps() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RefundPolicyContract);
    let client = RefundPolicyContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let cutoff_secs = 3600;
    let late_bps = 10000; // 100% refund after cutoff

    client.init(&admin, &cutoff_secs, &late_bps);

    let now = 2000;
    let session_start = 5000; // 3000 seconds from now (after cutoff)
    let amount = 1000;

    // After cutoff with 100% late_bps, should return full amount
    let refund = client.compute_refund(&now, &session_start, &amount);
    assert_eq!(refund, amount);
}
