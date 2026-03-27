use soroban_sdk::{contracttype, Address, Env, Symbol};

#[contracttype]
#[derive(Clone)]
pub struct MentorReputation(pub u32);

fn rep_key() -> Symbol {
    Symbol::short("rep")
}

pub fn get_rep(e: &Env, mentor: &Address) -> u32 {
    e.storage()
        .persistent()
        .get(&(rep_key(), mentor))
        .unwrap_or(MentorReputation(0))
        .0
}

pub fn set_rep(e: &Env, mentor: &Address, value: u32) {
    e.storage()
        .persistent()
        .set(&(rep_key(), mentor), &MentorReputation(value));
}
