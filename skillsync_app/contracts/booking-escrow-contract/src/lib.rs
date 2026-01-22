#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env, Symbol,
};

mod test;

// Storage keys
const ADMIN_KEY: Symbol = symbol_short!("ADMIN");

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Escrow(u64),
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EscrowStatus {
    Funded = 0,
    Released = 1,
    Refunded = 2,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Escrow {
    pub booking_id: u64,
    pub mentee: Address,
    pub mentor: Address,
    pub token: Address,
    pub amount: i128,
    pub status: EscrowStatus,
    pub created_at: u64,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EscrowError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    EscrowNotFound = 3,
    EscrowAlreadyExists = 4,
    InvalidAmount = 5,
    EscrowAlreadyTerminal = 6,
    Unauthorized = 7,
}

// Event types
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowFundedEvent {
    pub booking_id: u64,
    pub mentee: Address,
    pub mentor: Address,
    pub token: Address,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowReleasedEvent {
    pub booking_id: u64,
    pub to: Address,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowRefundedEvent {
    pub booking_id: u64,
    pub to: Address,
    pub amount: i128,
}

#[contract]
pub struct BookingEscrowContract;

#[contractimpl]
impl BookingEscrowContract {
    /// Initialize the contract with an admin address.
    /// The admin is authorized to release or refund escrows.
    pub fn init(env: Env, admin: Address) -> Result<(), EscrowError> {
        if env.storage().instance().has(&ADMIN_KEY) {
            return Err(EscrowError::AlreadyInitialized);
        }

        env.storage().instance().set(&ADMIN_KEY, &admin);
        env.storage().instance().extend_ttl(100, 100);

        Ok(())
    }

    /// Fund an escrow for a booking.
    /// Only the mentee can fund an escrow. Transfers tokens from mentee to this contract.
    pub fn fund(
        env: Env,
        booking_id: u64,
        mentee: Address,
        mentor: Address,
        token: Address,
        amount: i128,
    ) -> Result<(), EscrowError> {
        // Check contract is initialized
        if !env.storage().instance().has(&ADMIN_KEY) {
            return Err(EscrowError::NotInitialized);
        }

        // Validate amount
        if amount <= 0 {
            return Err(EscrowError::InvalidAmount);
        }

        // Check escrow doesn't already exist
        let escrow_key = DataKey::Escrow(booking_id);
        if env.storage().persistent().has(&escrow_key) {
            return Err(EscrowError::EscrowAlreadyExists);
        }

        // Require authorization from the mentee
        mentee.require_auth();

        // Create the escrow record first (CEI pattern: checks-effects-interactions)
        let escrow = Escrow {
            booking_id,
            mentee: mentee.clone(),
            mentor: mentor.clone(),
            token: token.clone(),
            amount,
            status: EscrowStatus::Funded,
            created_at: env.ledger().timestamp(),
        };

        env.storage().persistent().set(&escrow_key, &escrow);
        env.storage().persistent().extend_ttl(&escrow_key, 100, 100);

        // Transfer tokens from mentee to this contract (interaction last)
        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&mentee, &env.current_contract_address(), &amount);

        // Emit event
        env.events().publish(
            (Symbol::new(&env, "EscrowFunded"),),
            EscrowFundedEvent {
                booking_id,
                mentee,
                mentor,
                token,
                amount,
            },
        );

        Ok(())
    }

    /// Release escrow funds to the mentor.
    /// Only the admin can call this function.
    pub fn release(env: Env, booking_id: u64) -> Result<(), EscrowError> {
        // Get admin and require authorization
        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN_KEY)
            .ok_or(EscrowError::NotInitialized)?;
        admin.require_auth();

        // Get escrow
        let escrow_key = DataKey::Escrow(booking_id);
        let mut escrow: Escrow = env
            .storage()
            .persistent()
            .get(&escrow_key)
            .ok_or(EscrowError::EscrowNotFound)?;

        // Check escrow is not already terminal (idempotency)
        if escrow.status != EscrowStatus::Funded {
            return Err(EscrowError::EscrowAlreadyTerminal);
        }

        // Update status first (CEI pattern)
        escrow.status = EscrowStatus::Released;
        env.storage().persistent().set(&escrow_key, &escrow);

        // Transfer tokens to mentor (interaction last)
        let token_client = token::Client::new(&env, &escrow.token);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.mentor,
            &escrow.amount,
        );

        // Emit event
        env.events().publish(
            (Symbol::new(&env, "EscrowReleased"),),
            EscrowReleasedEvent {
                booking_id,
                to: escrow.mentor,
                amount: escrow.amount,
            },
        );

        Ok(())
    }

    /// Refund escrow funds to the mentee.
    /// Only the admin can call this function.
    pub fn refund(env: Env, booking_id: u64) -> Result<(), EscrowError> {
        // Get admin and require authorization
        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN_KEY)
            .ok_or(EscrowError::NotInitialized)?;
        admin.require_auth();

        // Get escrow
        let escrow_key = DataKey::Escrow(booking_id);
        let mut escrow: Escrow = env
            .storage()
            .persistent()
            .get(&escrow_key)
            .ok_or(EscrowError::EscrowNotFound)?;

        // Check escrow is not already terminal (idempotency)
        if escrow.status != EscrowStatus::Funded {
            return Err(EscrowError::EscrowAlreadyTerminal);
        }

        // Update status first (CEI pattern)
        escrow.status = EscrowStatus::Refunded;
        env.storage().persistent().set(&escrow_key, &escrow);

        // Transfer tokens to mentee (interaction last)
        let token_client = token::Client::new(&env, &escrow.token);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.mentee,
            &escrow.amount,
        );

        // Emit event
        env.events().publish(
            (Symbol::new(&env, "EscrowRefunded"),),
            EscrowRefundedEvent {
                booking_id,
                to: escrow.mentee,
                amount: escrow.amount,
            },
        );

        Ok(())
    }

    /// Get escrow details by booking ID.
    pub fn get(env: Env, booking_id: u64) -> Result<Escrow, EscrowError> {
        let escrow_key = DataKey::Escrow(booking_id);
        env.storage()
            .persistent()
            .get(&escrow_key)
            .ok_or(EscrowError::EscrowNotFound)
    }

    /// Get escrow status by booking ID.
    pub fn status(env: Env, booking_id: u64) -> Result<EscrowStatus, EscrowError> {
        let escrow_key = DataKey::Escrow(booking_id);
        let escrow: Escrow = env
            .storage()
            .persistent()
            .get(&escrow_key)
            .ok_or(EscrowError::EscrowNotFound)?;
        Ok(escrow.status)
    }

    /// Get the admin address.
    pub fn get_admin(env: Env) -> Result<Address, EscrowError> {
        env.storage()
            .instance()
            .get(&ADMIN_KEY)
            .ok_or(EscrowError::NotInitialized)
    }
}
