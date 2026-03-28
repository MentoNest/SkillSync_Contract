#![no_std]

use soroban_sdk::{contract, contractimpl, Env, Address};

mod storage;
mod reputation;

#[contract]
pub struct SkillSync;

#[contractimpl]
impl SkillSync {
    pub fn get_reputation(env: Env, mentor: Address) -> u32 {
        reputation::get_reputation(env, mentor)
    }

    pub fn increment_reputation(env: Env, caller: Address, mentor: Address, amount: u32) {
        reputation::increment_reputation(env, caller, mentor, amount)
    }

    pub fn decrement_reputation(env: Env, caller: Address, mentor: Address, amount: u32) {
        reputation::decrement_reputation(env, caller, mentor, amount)
    }
}
