use soroban_sdk::testutils::{Address as _, EnvTestConfig, Events as _};
use soroban_sdk::{token, Address, Env};

use crate::{WithdrawalContract, WithdrawalContractClient};

fn test_env() -> Env {
    Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    })
}

fn setup_token(env: &Env) -> (Address, token::Client<'_>, token::StellarAssetClient<'_>) {
    let token_admin = Address::generate(env);
    let token_id = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::Client::new(env, &token_id);
    let token_asset = token::StellarAssetClient::new(env, &token_id);
    (token_id, token_client, token_asset)
}

#[test]
fn init_sets_admin() {
    let env = test_env();
    let admin = Address::generate(&env);

    let contract_id = env.register_contract(None, WithdrawalContract);
    let client = WithdrawalContractClient::new(&env, &contract_id);
    client.init(&admin);

    assert_eq!(client.admin(), admin);
}

#[test]
fn credit_increases_balance() {
    let env = test_env();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentor = Address::generate(&env);
    let (token_id, _, _) = setup_token(&env);

    let contract_id = env.register_contract(None, WithdrawalContract);
    let client = WithdrawalContractClient::new(&env, &contract_id);
    client.init(&admin);

    assert_eq!(client.available(&mentor, &token_id), 0);

    client.credit(&mentor, &token_id, &1000);
    assert_eq!(client.available(&mentor, &token_id), 1000);

    client.credit(&mentor, &token_id, &500);
    assert_eq!(client.available(&mentor, &token_id), 1500);
}

#[test]
fn credit_requires_admin_auth() {
    let env = test_env();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentor = Address::generate(&env);
    let (token_id, _, _) = setup_token(&env);

    let contract_id = env.register_contract(None, WithdrawalContract);
    let client = WithdrawalContractClient::new(&env, &contract_id);
    client.init(&admin);

    client.credit(&mentor, &token_id, &1000);

    let auths = env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, admin);
}

#[test]
fn partial_withdrawal_updates_balance_and_transfers() {
    let env = test_env();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentor = Address::generate(&env);
    let (token_id, token_client, token_asset) = setup_token(&env);

    let contract_id = env.register_contract(None, WithdrawalContract);
    let client = WithdrawalContractClient::new(&env, &contract_id);
    client.init(&admin);

    // Credit mentor
    client.credit(&mentor, &token_id, &1000);

    // Mint tokens to the contract (simulating escrow release)
    token_asset.mint(&contract_id, &1000);

    // Partial withdrawal
    client.withdraw(&mentor, &token_id, &400);

    assert_eq!(client.available(&mentor, &token_id), 600);
    assert_eq!(token_client.balance(&mentor), 400);
    assert_eq!(token_client.balance(&contract_id), 600);
}

#[test]
fn full_withdrawal_updates_balance_and_transfers() {
    let env = test_env();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentor = Address::generate(&env);
    let (token_id, token_client, token_asset) = setup_token(&env);

    let contract_id = env.register_contract(None, WithdrawalContract);
    let client = WithdrawalContractClient::new(&env, &contract_id);
    client.init(&admin);

    // Credit mentor
    client.credit(&mentor, &token_id, &1000);

    // Mint tokens to the contract
    token_asset.mint(&contract_id, &1000);

    // Full withdrawal using withdraw
    client.withdraw(&mentor, &token_id, &1000);

    assert_eq!(client.available(&mentor, &token_id), 0);
    assert_eq!(token_client.balance(&mentor), 1000);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn withdraw_all_transfers_entire_balance() {
    let env = test_env();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentor = Address::generate(&env);
    let (token_id, token_client, token_asset) = setup_token(&env);

    let contract_id = env.register_contract(None, WithdrawalContract);
    let client = WithdrawalContractClient::new(&env, &contract_id);
    client.init(&admin);

    // Credit mentor
    client.credit(&mentor, &token_id, &5000);

    // Mint tokens to the contract
    token_asset.mint(&contract_id, &5000);

    // Withdraw all
    client.withdraw_all(&mentor, &token_id);

    assert_eq!(client.available(&mentor, &token_id), 0);
    assert_eq!(token_client.balance(&mentor), 5000);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn withdraw_requires_mentor_auth() {
    let env = test_env();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentor = Address::generate(&env);
    let (token_id, _, token_asset) = setup_token(&env);

    let contract_id = env.register_contract(None, WithdrawalContract);
    let client = WithdrawalContractClient::new(&env, &contract_id);
    client.init(&admin);

    client.credit(&mentor, &token_id, &1000);
    token_asset.mint(&contract_id, &1000);

    client.withdraw(&mentor, &token_id, &500);

    let auths = env.auths();
    // Last auth should be from mentor
    let mentor_auth = auths.iter().find(|(addr, _)| *addr == mentor);
    assert!(mentor_auth.is_some());
}

#[test]
fn withdraw_all_requires_mentor_auth() {
    let env = test_env();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentor = Address::generate(&env);
    let (token_id, _, token_asset) = setup_token(&env);

    let contract_id = env.register_contract(None, WithdrawalContract);
    let client = WithdrawalContractClient::new(&env, &contract_id);
    client.init(&admin);

    client.credit(&mentor, &token_id, &1000);
    token_asset.mint(&contract_id, &1000);

    client.withdraw_all(&mentor, &token_id);

    let auths = env.auths();
    let mentor_auth = auths.iter().find(|(addr, _)| *addr == mentor);
    assert!(mentor_auth.is_some());
}

#[test]
fn emits_withdrawal_event() {
    let env = test_env();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentor = Address::generate(&env);
    let (token_id, _, token_asset) = setup_token(&env);

    let contract_id = env.register_contract(None, WithdrawalContract);
    let client = WithdrawalContractClient::new(&env, &contract_id);
    client.init(&admin);

    client.credit(&mentor, &token_id, &1000);
    token_asset.mint(&contract_id, &1000);

    client.withdraw(&mentor, &token_id, &500);

    let events = env.events().all();
    // Should have credited event + withdrawal event + token transfer events
    assert!(events.len() >= 2);
}

#[test]
fn emits_credited_event() {
    let env = test_env();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentor = Address::generate(&env);
    let (token_id, _, _) = setup_token(&env);

    let contract_id = env.register_contract(None, WithdrawalContract);
    let client = WithdrawalContractClient::new(&env, &contract_id);
    client.init(&admin);

    client.credit(&mentor, &token_id, &1000);

    let events = env.events().all();
    assert!(!events.is_empty());
}

#[test]
fn multiple_mentors_have_separate_balances() {
    let env = test_env();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentor1 = Address::generate(&env);
    let mentor2 = Address::generate(&env);
    let (token_id, token_client, token_asset) = setup_token(&env);

    let contract_id = env.register_contract(None, WithdrawalContract);
    let client = WithdrawalContractClient::new(&env, &contract_id);
    client.init(&admin);

    // Credit different amounts to different mentors
    client.credit(&mentor1, &token_id, &1000);
    client.credit(&mentor2, &token_id, &2000);

    assert_eq!(client.available(&mentor1, &token_id), 1000);
    assert_eq!(client.available(&mentor2, &token_id), 2000);

    // Mint tokens
    token_asset.mint(&contract_id, &3000);

    // Withdraw from mentor1
    client.withdraw(&mentor1, &token_id, &500);

    assert_eq!(client.available(&mentor1, &token_id), 500);
    assert_eq!(client.available(&mentor2, &token_id), 2000);
    assert_eq!(token_client.balance(&mentor1), 500);
    assert_eq!(token_client.balance(&mentor2), 0);
}

#[test]
fn multiple_tokens_have_separate_balances() {
    let env = test_env();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentor = Address::generate(&env);
    let (token1_id, token1_client, token1_asset) = setup_token(&env);
    let (token2_id, token2_client, token2_asset) = setup_token(&env);

    let contract_id = env.register_contract(None, WithdrawalContract);
    let client = WithdrawalContractClient::new(&env, &contract_id);
    client.init(&admin);

    // Credit different tokens
    client.credit(&mentor, &token1_id, &1000);
    client.credit(&mentor, &token2_id, &5000);

    assert_eq!(client.available(&mentor, &token1_id), 1000);
    assert_eq!(client.available(&mentor, &token2_id), 5000);

    // Mint tokens
    token1_asset.mint(&contract_id, &1000);
    token2_asset.mint(&contract_id, &5000);

    // Withdraw from token1 only
    client.withdraw(&mentor, &token1_id, &300);

    assert_eq!(client.available(&mentor, &token1_id), 700);
    assert_eq!(client.available(&mentor, &token2_id), 5000);
    assert_eq!(token1_client.balance(&mentor), 300);
    assert_eq!(token2_client.balance(&mentor), 0);
}

#[test]
fn sequential_withdrawals_work_correctly() {
    let env = test_env();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentor = Address::generate(&env);
    let (token_id, token_client, token_asset) = setup_token(&env);

    let contract_id = env.register_contract(None, WithdrawalContract);
    let client = WithdrawalContractClient::new(&env, &contract_id);
    client.init(&admin);

    // Credit mentor
    client.credit(&mentor, &token_id, &1000);
    token_asset.mint(&contract_id, &1000);

    // First withdrawal
    client.withdraw(&mentor, &token_id, &200);
    assert_eq!(client.available(&mentor, &token_id), 800);
    assert_eq!(token_client.balance(&mentor), 200);

    // Second withdrawal
    client.withdraw(&mentor, &token_id, &300);
    assert_eq!(client.available(&mentor, &token_id), 500);
    assert_eq!(token_client.balance(&mentor), 500);

    // Third withdrawal
    client.withdraw(&mentor, &token_id, &500);
    assert_eq!(client.available(&mentor, &token_id), 0);
    assert_eq!(token_client.balance(&mentor), 1000);
}
