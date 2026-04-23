#![cfg(test)]

use crate::{CoreContract, CoreContractClient};
use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, BytesN, Env};

#[test]
fn test_hello() {
    let env = Env::default();
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