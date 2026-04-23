#![cfg(test)]

use crate::{CoreContract, CoreContractClient};
use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, BytesN, Env};
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
