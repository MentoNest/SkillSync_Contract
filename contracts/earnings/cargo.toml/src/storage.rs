use soroban_sdk::{contracttype, Address, Env};

#[contracttype]
pub enum DataKey {
    Escrow,
    Balance(Address, Address),
    Credit(Address, Address, u64),
    NextIdx(Address, Address),
}

#[contracttype]
#[derive(Clone)]
pub struct CreditRecord {
    pub amount: u128,
    pub booking_id: u64,
    pub ts: u64,
}

pub fn set_escrow(env: &Env, addr: &Address) {
    env.storage().instance().set(&DataKey::Escrow, addr);
}

pub fn get_escrow(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Escrow).unwrap()
}

pub fn has_escrow(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Escrow)
}

pub fn get_balance(env: &Env, mentor: &Address, token: &Address) -> u128 {
    env.storage()
        .persistent()
        .get(&DataKey::Balance(mentor.clone(), token.clone()))
        .unwrap_or(0)
}

pub fn set_balance(env: &Env, mentor: &Address, token: &Address, amount: u128) {
    env.storage()
        .persistent()
        .set(&DataKey::Balance(mentor.clone(), token.clone()), &amount);
}

pub fn get_next_idx(env: &Env, mentor: &Address, token: &Address) -> u64 {
    env.storage()
        .persistent()
        .get(&DataKey::NextIdx(mentor.clone(), token.clone()))
        .unwrap_or(0)
}

pub fn set_next_idx(env: &Env, mentor: &Address, token: &Address, idx: u64) {
    env.storage()
        .persistent()
        .set(&DataKey::NextIdx(mentor.clone(), token.clone()), &idx);
}

pub fn set_credit(
    env: &Env,
    mentor: &Address,
    token: &Address,
    idx: u64,
    record: &CreditRecord,
) {
    env.storage()
        .persistent()
        .set(&DataKey::Credit(mentor.clone(), token.clone(), idx), record);
}

pub fn get_credit(
    env: &Env,
    mentor: &Address,
    token: &Address,
    idx: u64,
) -> Option<CreditRecord> {
    env.storage()
        .persistent()
        .get(&DataKey::Credit(mentor.clone(), token.clone(), idx))
}
