#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Policy {
    pub cutoff_secs: u64,
    pub late_bps: u64, // basis points (0-10000, where 10000 = 100%)
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Policy,
}

#[contract]
pub struct RefundPolicyContract;

#[contractimpl]
impl RefundPolicyContract {
    /// Initialize the contract with admin, cutoff_secs, and late_bps
    pub fn init(env: Env, admin: Address, cutoff_secs: u64, late_bps: u64) {
        // Validate late_bps <= 10000 (100%)
        if late_bps > 10000 {
            panic!("late_bps must be <= 10000");
        }

        // Store admin
        env.storage().instance().set(&DataKey::Admin, &admin);

        // Store policy
        let policy = Policy {
            cutoff_secs,
            late_bps,
        };
        env.storage().instance().set(&DataKey::Policy, &policy);

        // Emit initial policy event
        env.events().publish(
            (Symbol::new(&env, "PolicyUpdated"),),
            (cutoff_secs, late_bps),
        );
    }

    /// Set the refund policy (admin-only)
    pub fn set_policy(env: Env, cutoff_secs: u64, late_bps: u64) {
        // Check admin authorization
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized");
        
        admin.require_auth();

        // Validate late_bps <= 10000
        if late_bps > 10000 {
            panic!("late_bps must be <= 10000");
        }

        // Update policy
        let policy = Policy {
            cutoff_secs,
            late_bps,
        };
        env.storage().instance().set(&DataKey::Policy, &policy);

        // Emit policy updated event
        env.events().publish(
            (Symbol::new(&env, "PolicyUpdated"),),
            (cutoff_secs, late_bps),
        );
    }

    /// Compute refund amount based on current time, session start, and booking amount
    /// Returns the refund amount (0 if no refund)
    pub fn compute_refund(
        env: Env,
        now: u64,
        session_start: u64,
        amount: i128,
    ) -> i128 {
        let policy: Policy = env
            .storage()
            .instance()
            .get(&DataKey::Policy)
            .expect("Contract not initialized");

        // If session has already started, no refund (unless admin override in future)
        if now >= session_start {
            return 0;
        }

        // Calculate time until session start
        let time_until_start = session_start - now;

        // If before cutoff, full refund
        if time_until_start > policy.cutoff_secs {
            return amount;
        }

        // After cutoff but before start: partial refund based on late_bps
        // late_bps is in basis points (0-10000), so percentage = late_bps / 10000
        // refund = amount * late_bps / 10000
        let refund = (amount as u128)
            .checked_mul(policy.late_bps as u128)
            .and_then(|x| x.checked_div(10000))
            .unwrap_or(0);

        refund as i128
    }

    /// Get the current admin address
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized")
    }

    /// Get the current policy
    pub fn get_policy(env: Env) -> Policy {
        env.storage()
            .instance()
            .get(&DataKey::Policy)
            .expect("Contract not initialized")
    }
}

#[cfg(test)]
mod test;
