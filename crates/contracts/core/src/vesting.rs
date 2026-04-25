//! Time-locked vesting release for sellers.
//!
//! Supports linear vesting with an optional cliff period.
//! After the cliff, the seller may claim the linearly-vested portion at any time.
//! If the session is disputed, unvested funds return to the buyer.

use soroban_sdk::{contracttype, Address, Env, Symbol, symbol_short};

const KEY_VESTING: Symbol = symbol_short!("vesting");

/// Vesting schedule attached to a session.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct VestingSchedule {
    pub session_id: soroban_sdk::Bytes,
    pub seller: Address,
    pub buyer: Address,
    pub total_amount: i128,
    /// Ledger number after which vesting begins.
    pub cliff_ledger: u64,
    /// Total ledgers over which the full amount vests linearly.
    pub vesting_duration: u64,
    /// Ledger when this schedule was created.
    pub start_ledger: u64,
    /// Amount already claimed by the seller.
    pub claimed_amount: i128,
    pub is_disputed: bool,
}

fn vesting_key(env: &Env, session_id: &soroban_sdk::Bytes) -> (Symbol, soroban_sdk::Bytes) {
    (KEY_VESTING, session_id.clone())
}

/// Lock funds with a vesting schedule for `session_id`.
pub fn lock_funds_with_vesting(
    env: &Env,
    session_id: soroban_sdk::Bytes,
    seller: Address,
    buyer: Address,
    amount: i128,
    cliff_ledgers: u64,
    vesting_duration: u64,
) {
    let schedule = VestingSchedule {
        session_id: session_id.clone(),
        seller,
        buyer,
        total_amount: amount,
        cliff_ledger: env.ledger().sequence() as u64 + cliff_ledgers,
        vesting_duration,
        start_ledger: env.ledger().sequence() as u64,
        claimed_amount: 0,
        is_disputed: false,
    };
    env.storage().persistent().set(&vesting_key(env, &session_id), &schedule);
}

/// Returns how much the seller can currently claim.
pub fn claimable_amount(env: &Env, schedule: &VestingSchedule) -> i128 {
    let current = env.ledger().sequence() as u64;
    if current < schedule.cliff_ledger || schedule.is_disputed {
        return 0;
    }
    let elapsed = current.saturating_sub(schedule.start_ledger);
    let vested = if elapsed >= schedule.vesting_duration {
        schedule.total_amount
    } else {
        (schedule.total_amount as u128 * elapsed as u128 / schedule.vesting_duration as u128) as i128
    };
    (vested - schedule.claimed_amount).max(0)
}

/// Seller claims vested amount. Returns the amount to transfer.
pub fn claim_vested(env: &Env, session_id: soroban_sdk::Bytes, seller: &Address) -> i128 {
    seller.require_auth();
    let key = vesting_key(env, &session_id);
    let mut schedule: VestingSchedule = env.storage().persistent().get(&key).expect("no vesting schedule");
    assert!(&schedule.seller == seller, "not the seller");
    let amount = claimable_amount(env, &schedule);
    assert!(amount > 0, "nothing to claim");
    schedule.claimed_amount += amount;
    env.storage().persistent().set(&key, &schedule);
    amount
}

/// Mark a session as disputed — unvested funds become returnable to buyer.
pub fn mark_disputed(env: &Env, session_id: soroban_sdk::Bytes) {
    let key = vesting_key(env, &session_id);
    let mut schedule: VestingSchedule = env.storage().persistent().get(&key).expect("no vesting schedule");
    schedule.is_disputed = true;
    env.storage().persistent().set(&key, &schedule);
}

/// Returns unvested amount owed back to buyer after dispute.
pub fn unvested_amount(schedule: &VestingSchedule) -> i128 {
    (schedule.total_amount - schedule.claimed_amount).max(0)
}
