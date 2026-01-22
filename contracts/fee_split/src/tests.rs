use soroban_sdk::testutils::{Address as _, EnvTestConfig, Events as _};
use soroban_sdk::{token, Address, Env};

use crate::{FeeSplitContract, FeeSplitContractClient};

fn test_env() -> Env {
    Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    })
}

#[test]
fn split_math_edges_and_midpoint() {
    let env = test_env();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_zero = env.register_contract(None, FeeSplitContract);
    let client_zero = FeeSplitContractClient::new(&env, &contract_zero);
    client_zero.init(&admin, &treasury, &0);
    let (mentor_share, platform_fee) = client_zero.split(&1_000);
    assert_eq!(mentor_share, 1_000);
    assert_eq!(platform_fee, 0);

    let contract_full = env.register_contract(None, FeeSplitContract);
    let client_full = FeeSplitContractClient::new(&env, &contract_full);
    client_full.init(&admin, &treasury, &10_000);
    let (mentor_share, platform_fee) = client_full.split(&1_000);
    assert_eq!(mentor_share, 0);
    assert_eq!(platform_fee, 1_000);

    let contract_mid = env.register_contract(None, FeeSplitContract);
    let client_mid = FeeSplitContractClient::new(&env, &contract_mid);
    client_mid.init(&admin, &treasury, &250);
    let (mentor_share, platform_fee) = client_mid.split(&20_000);
    assert_eq!(mentor_share, 19_500);
    assert_eq!(platform_fee, 500);
}

#[test]
fn admin_only_updates_are_enforced() {
    let env = test_env();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let new_treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, FeeSplitContract);
    let client = FeeSplitContractClient::new(&env, &contract_id);
    client.init(&admin, &treasury, &100);

    env.mock_all_auths();
    client.set_fee_bps(&200);
    let auths = env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, admin);
    assert_eq!(client.fee_bps(), 200);

    client.set_treasury(&new_treasury);
    let auths = env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, admin);
    assert_eq!(client.treasury(), new_treasury);
}

#[test]
fn emits_events_for_updates_and_splits() {
    let env = test_env();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, FeeSplitContract);
    let client = FeeSplitContractClient::new(&env, &contract_id);
    client.init(&admin, &treasury, &150);

    client.set_fee_bps(&250);
    client.set_treasury(&Address::generate(&env));
    client.split(&10_000);

    let events = env.events().all();
    assert_eq!(events.len(), 3);
}

#[test]
fn release_sends_fee_to_treasury() {
    let env = test_env();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let mentor = Address::generate(&env);

    let contract_id = env.register_contract(None, FeeSplitContract);
    let client = FeeSplitContractClient::new(&env, &contract_id);
    client.init(&admin, &treasury, &1_000);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::Client::new(&env, &token_id);
    let token_asset = token::StellarAssetClient::new(&env, &token_id);

    let amount: i128 = 1_000_000;
    token_asset.mint(&contract_id, &amount);

    let (mentor_share, platform_fee) = client.release(&token_id, &mentor, &amount, &Some(42));
    assert_eq!(token_client.balance(&mentor), mentor_share);
    assert_eq!(token_client.balance(&treasury), platform_fee);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn split_rounds_down_and_assigns_remainder_to_fee() {
    let env = test_env();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, FeeSplitContract);
    let client = FeeSplitContractClient::new(&env, &contract_id);
    client.init(&admin, &treasury, &3_333);

    let (mentor_share, platform_fee) = client.split(&100);
    assert_eq!(mentor_share, 66);
    assert_eq!(platform_fee, 34);
}

#[test]
fn release_with_zero_fee_sends_all_to_mentor() {
    let env = test_env();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let mentor = Address::generate(&env);

    let contract_id = env.register_contract(None, FeeSplitContract);
    let client = FeeSplitContractClient::new(&env, &contract_id);
    client.init(&admin, &treasury, &0);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::Client::new(&env, &token_id);
    let token_asset = token::StellarAssetClient::new(&env, &token_id);

    let amount: i128 = 10_000;
    token_asset.mint(&contract_id, &amount);

    let (mentor_share, platform_fee) = client.release(&token_id, &mentor, &amount, &None);
    assert_eq!(mentor_share, amount);
    assert_eq!(platform_fee, 0);
    assert_eq!(token_client.balance(&mentor), amount);
    assert_eq!(token_client.balance(&treasury), 0);
}

#[test]
fn release_with_full_fee_sends_all_to_treasury() {
    let env = test_env();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let mentor = Address::generate(&env);

    let contract_id = env.register_contract(None, FeeSplitContract);
    let client = FeeSplitContractClient::new(&env, &contract_id);
    client.init(&admin, &treasury, &10_000);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::Client::new(&env, &token_id);
    let token_asset = token::StellarAssetClient::new(&env, &token_id);

    let amount: i128 = 10_000;
    token_asset.mint(&contract_id, &amount);

    let (mentor_share, platform_fee) = client.release(&token_id, &mentor, &amount, &None);
    assert_eq!(mentor_share, 0);
    assert_eq!(platform_fee, amount);
    assert_eq!(token_client.balance(&mentor), 0);
    assert_eq!(token_client.balance(&treasury), amount);
}

#[test]
fn split_handles_large_amounts() {
    let env = test_env();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, FeeSplitContract);
    let client = FeeSplitContractClient::new(&env, &contract_id);
    client.init(&admin, &treasury, &0);

    let amount = i128::MAX;
    let (mentor_share, platform_fee) = client.split(&amount);
    assert!(mentor_share >= 0);
    assert!(platform_fee >= 0);
    assert_eq!(mentor_share + platform_fee, amount);
}
