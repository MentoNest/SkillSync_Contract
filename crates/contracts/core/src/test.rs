#![cfg(test)]

extern crate std;

use super::{CoreContract, CoreContractClient, SessionStatus};
use soroban_sdk::{bytesn, testutils::Address as _, Address, BytesN, Env};

fn session_id(env: &Env) -> BytesN<32> {
    bytesn!(
        env,
        0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20
    )
}

#[test]
fn lock_funds_stores_locked_session() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CoreContract);
    let client = CoreContractClient::new(&env, &contract_id);

    let seller = Address::generate(&env);
    let session_id = session_id(&env);

    client.lock_funds(&session_id, &seller, &500_i128);

    let session = client.get_session(&session_id).unwrap();
    assert_eq!(session.seller, seller);
    assert_eq!(session.amount, 500_i128);
    assert_eq!(session.status, SessionStatus::Locked);
}

#[test]
#[should_panic]
fn lock_funds_rejects_duplicate_session_ids() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CoreContract);
    let client = CoreContractClient::new(&env, &contract_id);

    let seller = Address::generate(&env);
    let session_id = session_id(&env);

    client.lock_funds(&session_id, &seller, &500_i128);
    client.lock_funds(&session_id, &seller, &500_i128);
}

#[test]
#[should_panic]
fn lock_funds_requires_positive_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CoreContract);
    let client = CoreContractClient::new(&env, &contract_id);

    let seller = Address::generate(&env);

    client.lock_funds(&session_id(&env), &seller, &0_i128);
}
