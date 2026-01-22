#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, token, Address, Env,
    Symbol,
};

const BPS_SCALE: u32 = 10_000;

#[contracttype]
pub enum DataKey {
    Admin,
    Treasury,
    FeeBps,
}

#[contracttype]
pub struct FeeUpdated {
    pub fee_bps: u32,
}

#[contracttype]
pub struct TreasuryUpdated {
    pub addr: Address,
}

#[contracttype]
pub struct PayoutSplit {
    pub booking_id: Option<u64>,
    pub mentor_share: i128,
    pub platform_fee: i128,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidFeeBps = 3,
    NegativeAmount = 4,
}

#[contract]
pub struct FeeSplitContract;

impl FeeSplitContract {
    /// Load the admin address or panic if the contract is not initialize.
    fn read_admin(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized))
    }

    /// Load the treasury address or panic if the contract is not initialized.
    fn read_treasury(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Treasury)
            .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized))
    }

    /// Load the fee bps or panic if the contract is not initialized.
    fn read_fee_bps(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::FeeBps)
            .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized))
    }

    /// Require authorization from the stored admin address.
    fn require_admin(env: &Env) {
        let admin = Self::read_admin(env);
        admin.require_auth();
    }

    /// Persist fee bps after validating it is within bounds.
    fn write_fee_bps(env: &Env, fee_bps: u32) {
        if fee_bps > BPS_SCALE {
            panic_with_error!(env, Error::InvalidFeeBps);
        }
        env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
    }

    /// Compute mentor share and platform fee for a given amount.
    fn split_amount(env: &Env, amount: i128) -> (i128, i128) {
        if amount < 0 {
            panic_with_error!(env, Error::NegativeAmount);
        }
        let fee_bps = Self::read_fee_bps(env);
        let keep_bps = i128::from(BPS_SCALE - fee_bps);
        let scale = i128::from(BPS_SCALE);
        let quotient = amount / scale;
        let remainder = amount % scale;
        let mentor_share = (quotient * keep_bps) + ((remainder * keep_bps) / scale);
        let platform_fee = amount - mentor_share;
        (mentor_share, platform_fee)
    }
}

#[contractimpl]
impl FeeSplitContract {
    /// Initialize admin, treasury, and fee bps exactly once.
    pub fn init(env: Env, admin: Address, treasury: Address, fee_bps: u32) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(env, Error::AlreadyInitialized);
        }
        if fee_bps > BPS_SCALE {
            panic_with_error!(env, Error::InvalidFeeBps);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::Treasury, &treasury);
        env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
    }

    /// Update fee bps (admin only).
    pub fn set_fee_bps(env: Env, fee_bps: u32) {
        Self::require_admin(&env);
        Self::write_fee_bps(&env, fee_bps);
        env.events()
            .publish((Symbol::new(&env, "fee_updated"),), FeeUpdated { fee_bps });
    }

    /// Update treasury address (admin only).
    pub fn set_treasury(env: Env, addr: Address) {
        Self::require_admin(&env);
        env.storage().instance().set(&DataKey::Treasury, &addr);
        env.events().publish(
            (Symbol::new(&env, "treasury_updated"),),
            TreasuryUpdated { addr },
        );
    }

    /// Return split amounts and emit a payout event without transferring funds.
    pub fn split(env: Env, amount: i128) -> (i128, i128) {
        let (mentor_share, platform_fee) = Self::split_amount(&env, amount);
        env.events().publish(
            (Symbol::new(&env, "payout_split"),),
            PayoutSplit {
                booking_id: None,
                mentor_share,
                platform_fee,
            },
        );
        (mentor_share, platform_fee)
    }

    /// Transfer split funds from contract balance to mentor and treasury.
    pub fn release(
        env: Env,
        token: Address,
        mentor: Address,
        amount: i128,
        booking_id: Option<u64>,
    ) -> (i128, i128) {
        let (mentor_share, platform_fee) = Self::split_amount(&env, amount);
        let treasury = Self::read_treasury(&env);
        let token_client = token::Client::new(&env, &token);
        let contract = env.current_contract_address();
        token_client.transfer(&contract, &mentor, &mentor_share);
        if platform_fee > 0 {
            token_client.transfer(&contract, &treasury, &platform_fee);
        }
        env.events().publish(
            (Symbol::new(&env, "payout_split"),),
            PayoutSplit {
                booking_id,
                mentor_share,
                platform_fee,
            },
        );
        (mentor_share, platform_fee)
    }

    /// Read the admin address.
    pub fn admin(env: Env) -> Address {
        Self::read_admin(&env)
    }

    /// Read the treasury address.
    pub fn treasury(env: Env) -> Address {
        Self::read_treasury(&env)
    }

    /// Read the current fee bps.
    pub fn fee_bps(env: Env) -> u32 {
        Self::read_fee_bps(&env)
    }
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests;
