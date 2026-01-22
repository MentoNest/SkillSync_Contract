#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, token, Address, Env,
    Symbol,
};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Balance(Address, Address), // (mentor, token) -> balance
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Withdrawal {
    pub mentor: Address,
    pub token: Address,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Credited {
    pub mentor: Address,
    pub token: Address,
    pub amount: i128,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidAmount = 3,
    InsufficientBalance = 4,
    Unauthorized = 5,
}

#[contract]
pub struct WithdrawalContract;

impl WithdrawalContract {
    fn read_admin(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized))
    }

    fn require_admin(env: &Env) {
        let admin = Self::read_admin(env);
        admin.require_auth();
    }

    fn balance_key(mentor: &Address, token: &Address) -> DataKey {
        DataKey::Balance(mentor.clone(), token.clone())
    }

    fn read_balance(env: &Env, mentor: &Address, token: &Address) -> i128 {
        let key = Self::balance_key(mentor, token);
        env.storage().persistent().get(&key).unwrap_or(0)
    }

    fn write_balance(env: &Env, mentor: &Address, token: &Address, balance: i128) {
        let key = Self::balance_key(mentor, token);
        env.storage().persistent().set(&key, &balance);
        env.storage().persistent().extend_ttl(&key, 100, 100);
    }
}

#[contractimpl]
impl WithdrawalContract {
    /// Initialize the contract with an admin address.
    /// Admin can credit earnings to mentor accounts.
    pub fn init(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(env, Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().extend_ttl(100, 100);
    }

    /// Credit earnings to a mentor's balance.
    /// Only admin can call this function (typically called by escrow release).
    pub fn credit(env: Env, mentor: Address, token: Address, amount: i128) {
        Self::require_admin(&env);

        if amount <= 0 {
            panic_with_error!(env, Error::InvalidAmount);
        }

        let current_balance = Self::read_balance(&env, &mentor, &token);
        let new_balance = current_balance + amount;
        Self::write_balance(&env, &mentor, &token, new_balance);

        env.events().publish(
            (Symbol::new(&env, "credited"),),
            Credited {
                mentor,
                token,
                amount,
            },
        );
    }

    /// Withdraw a specific amount of tokens.
    /// Mentor must authorize this call to withdraw their own funds.
    pub fn withdraw(env: Env, mentor: Address, token: Address, amount: i128) {
        mentor.require_auth();

        if amount <= 0 {
            panic_with_error!(&env, Error::InvalidAmount);
        }

        let current_balance = Self::read_balance(&env, &mentor, &token);
        if amount > current_balance {
            panic_with_error!(&env, Error::InsufficientBalance);
        }

        let new_balance = current_balance - amount;
        Self::write_balance(&env, &mentor, &token, new_balance);

        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&env.current_contract_address(), &mentor, &amount);

        env.events().publish(
            (Symbol::new(&env, "withdrawal"),),
            Withdrawal {
                mentor,
                token,
                amount,
            },
        );
    }

    /// Withdraw all available tokens for a mentor.
    /// Mentor must authorize this call.
    pub fn withdraw_all(env: Env, mentor: Address, token: Address) {
        mentor.require_auth();

        let balance = Self::read_balance(&env, &mentor, &token);
        if balance <= 0 {
            panic_with_error!(&env, Error::InsufficientBalance);
        }

        Self::write_balance(&env, &mentor, &token, 0);

        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&env.current_contract_address(), &mentor, &balance);

        env.events().publish(
            (Symbol::new(&env, "withdrawal"),),
            Withdrawal {
                mentor,
                token,
                amount: balance,
            },
        );
    }

    /// Get available balance for a mentor and token.
    /// Pass-through from earnings tracking.
    pub fn available(env: Env, mentor: Address, token: Address) -> i128 {
        Self::read_balance(&env, &mentor, &token)
    }

    /// Read the admin address.
    pub fn admin(env: Env) -> Address {
        Self::read_admin(&env)
    }
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests;
