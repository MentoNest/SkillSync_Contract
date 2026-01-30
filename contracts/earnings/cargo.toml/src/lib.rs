#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short,
    Address, Env, Vec,
};

mod storage;
mod events;

use storage::*;
use events::*;

#[contract]
pub struct EarningsContract;

#[contractimpl]
impl EarningsContract {
    /// One-time setup to register escrow contract
    pub fn initialize(env: Env, escrow: Address) {
        if has_escrow(&env) {
            panic!("already initialized");
        }
        set_escrow(&env, &escrow);
    }

    /// Called ONLY by escrow contract
    pub fn credit(
        env: Env,
        mentor: Address,
        token: Address,
        amount: u128,
        booking_id: u64,
    ) {
        let escrow = get_escrow(&env);
        escrow.require_auth();

        // Update balance
        let balance = get_balance(&env, &mentor, &token);
        set_balance(&env, &mentor, &token, balance + amount);

        // Append credit record
        let idx = get_next_idx(&env, &mentor, &token);
        let record = CreditRecord {
            amount,
            booking_id,
            ts: env.ledger().timestamp(),
        };

        set_credit(&env, &mentor, &token, idx, &record);
        set_next_idx(&env, &mentor, &token, idx + 1);

        // Emit event
        emit_credited(&env, &mentor, &token, amount, booking_id);
    }

    /// Available earnings per mentor per token
    pub fn available(env: Env, mentor: Address, token: Address) -> u128 {
        get_balance(&env, &mentor, &token)
    }

    /// Paginated credit history
    pub fn history(
        env: Env,
        mentor: Address,
        token: Address,
        page: u64,
        limit: u64,
    ) -> Vec<CreditRecord> {
        let start = page * limit;
        let end = start + limit;

        let mut res = Vec::new(&env);
        let next = get_next_idx(&env, &mentor, &token);

        let mut i = start;
        while i < end && i < next {
            if let Some(r) = get_credit(&env, &mentor, &token, i) {
                res.push_back(r);
            }
            i += 1;
        }

        res
    }
}
