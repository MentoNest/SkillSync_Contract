#![cfg(test)]

extern crate std;

use crate::{RegistryContract, RegistryContractClient, RegistryError};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env, Symbol};

fn create_registry_contract(env: &Env) -> RegistryContractClient<'_> {
    let contract_id = env.register(RegistryContract, ());
    RegistryContractClient::new(env, &contract_id)
}

#[test]
fn test_set_and_get_pointer() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = create_registry_contract(&env);
    registry.init(&admin).unwrap();

    let name = Symbol::new(&env, "escrow_v2");
    let addr = Address::generate(&env);

    registry.set(&name, &addr).unwrap();

    let stored = registry.get(&name).unwrap();
    assert_eq!(stored, addr);
}

#[test]
fn test_update_pointer_keeps_single_entry() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = create_registry_contract(&env);
    registry.init(&admin).unwrap();

    let name = symbol_short!("escrow");
    let addr_v1 = Address::generate(&env);
    let addr_v2 = Address::generate(&env);

    registry.set(&name, &addr_v1).unwrap();
    registry.set(&name, &addr_v2).unwrap();

    let stored = registry.get(&name).unwrap();
    assert_eq!(stored, addr_v2);

    let entries = registry.all();
    assert_eq!(entries.len(), 1);
    let entry = entries.get(0).unwrap();
    assert_eq!(entry, (name, addr_v2));
}

#[test]
fn test_all_returns_multiple_entries() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = create_registry_contract(&env);
    registry.init(&admin).unwrap();

    let name1 = symbol_short!("escrow");
    let addr1 = Address::generate(&env);
    let name2 = symbol_short!("refund");
    let addr2 = Address::generate(&env);

    registry.set(&name1, &addr1).unwrap();
    registry.set(&name2, &addr2).unwrap();

    let entries = registry.all();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries.get(0).unwrap(), (name1, addr1));
    assert_eq!(entries.get(1).unwrap(), (name2, addr2));
}

#[test]
#[should_panic]
fn test_set_requires_admin_auth() {
    let env = Env::default();

    let admin = Address::generate(&env);
    let registry = create_registry_contract(&env);
    registry.init(&admin).unwrap();

    let name = symbol_short!("escrow");
    let addr = Address::generate(&env);

    registry.set(&name, &addr).unwrap();
}

#[test]
fn test_get_missing_returns_error() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = create_registry_contract(&env);
    registry.init(&admin).unwrap();

    let name = symbol_short!("missing");
    let result = registry.try_get(&name);
    assert_eq!(result, Err(Ok(RegistryError::NotFound)));
}
