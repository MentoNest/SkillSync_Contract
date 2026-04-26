use soroban_sdk::{contracttype, token, Address, Bytes, Env, Vec};

#[contracttype]
#[derive(Clone)]
pub struct LockEntry {
    pub session_id: Bytes,
    pub buyer: Address,
    pub amount: i128,
}

/// Locks funds for multiple sessions atomically.
/// Any single failure panics and reverts the entire batch.
pub fn batch_lock_funds(env: &Env, sessions: Vec<LockEntry>, token_id: Address, from: Address) {
    for i in 0..sessions.len() {
        let entry = sessions.get(i).unwrap();
        let client = token::Client::new(env, &token_id);
        client.transfer(&from, &env.current_contract_address(), &entry.amount);
    }
}

/// Approves (marks complete on buyer side) a batch of session IDs.
/// Any single failure panics and reverts the entire batch.
pub fn batch_approve(env: &Env, session_ids: Vec<Bytes>, buyer: Address) {
    buyer.require_auth();
    for i in 0..session_ids.len() {
        let session_id = session_ids.get(i).unwrap();
        // Mark buyer approval in storage.
        env.storage()
            .persistent()
            .set(&(soroban_sdk::symbol_short!("bapprv"), session_id), &buyer);
    }
}

/// Completes (marks released on seller side) a batch of session IDs.
/// Any single failure panics and reverts the entire batch.
pub fn batch_complete(env: &Env, session_ids: Vec<Bytes>, seller: Address) {
    seller.require_auth();
    for i in 0..session_ids.len() {
        let session_id = session_ids.get(i).unwrap();
        // Mark seller completion in storage.
        env.storage()
            .persistent()
            .set(&(soroban_sdk::symbol_short!("bcmpl"), session_id), &seller);
    }
}
