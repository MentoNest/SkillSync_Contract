#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, Address, Env, IntoVal,
    Symbol, Vec, token,
    contract, contracterror, contractimpl, contracttype, panic_with_error, Address, Env, Symbol,
};

pub const DISPUTE_WINDOW_MIN_SECONDS: u64 = 60;
pub const DISPUTE_WINDOW_MAX_SECONDS: u64 = 30 * 24 * 60 * 60;
pub const DEFAULT_DISPUTE_WINDOW_SECONDS: u64 = 24 * 60 * 60;

#[contract]
pub struct SkillSyncContract;

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    DisputeWindow,
    Treasury,
    Session(Vec<u8>),
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SessionStatus {
    Pending = 0,
    Completed = 1,
    Disputed = 2,
    Cancelled = 3,
}

#[contracttype]
#[derive(Clone)]
pub struct Session {
    // Version field for forward/backward compatibility.
    // - When adding/removing fields, increment `version` and provide
    //   migration helpers that can decode older versions and upgrade them.
    // - Keep changes additive where possible (append-only) to allow
    //   older contract binaries to safely decode newer data when feasible.
    // Migration path:
    // 1. Read stored `Session` and inspect `version`.
    // 2. If `version` is older, run migration logic to populate new fields
    //    with sensible defaults and re-save with the new `version`.
    // 3. Maintain tests that serialize `version=0` values and ensure
    //    decode/migration remains safe (see unit tests below).
    //
    // Session ID Generation (Recommended):
    // ====================================
    // The `session_id` field must be globally unique across all sessions.
    // To prevent duplicate session IDs, offchain clients MUST generate IDs
    // with high entropy before calling `put_session()`.
    //
    // Recommended approaches:
    // 1. UUID v4 (Random): Use a cryptographically secure random UUID.
    //    Example: Generate with `uuid::Uuid::new_v4()` in Rust/JS libraries
    //    - Probability of collision: Negligible for practical purposes
    //    - Suitable for session tracking across multiple users/trades
    //
    // 2. SHA256(random seed): Hash a high-entropy random value
    //    Example: SHA256(CSPRNG bytes) → truncate or use full hash as ID
    //    - Deterministic and verifiable if needed
    //    - Same collision guarantees as UUID v4
    //
    // 3. Concatenate entropy: timestamp + random bytes + user nonce
    //    Example: timestamp_ms (8 bytes) + CSPRNG (8 bytes) = 16 bytes
    //    - Must use cryptographically secure random source
    //    - Better than sequential or predictable IDs
    //
    // IMPORTANT: Do NOT use:
    // - Sequential IDs (1, 2, 3, ...)
    // - Hashed addresses alone (collides with multiple sessions per address)
    // - Weak randomness (Math.random(), time-only based IDs)
    // - User-controlled input without hashing/validation
    //
    // The contract enforces uniqueness via `put_session()`:
    // - Returns `DuplicateSessionId` error if session_id already exists
    // - This is a final guard; offchain systems should also validate uniqueness.

    pub version: u32,
    pub session_id: Vec<u8>,
    pub payer: Address,
    pub payee: Address,
    pub asset: Address,
    pub amount: i128,
    pub fee_bps: u32,
    pub status: SessionStatus,
    pub created_at: u64,
    pub updated_at: u64,
    pub dispute_deadline: u64,
    PlatformFee,
    Version,
}

const VERSION: u32 = 1;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidDisputeWindow = 3,
    Unauthorized = 4,
    InvalidTreasuryAddress = 5,
    DuplicateSessionId = 6,
    InvalidAmount = 7,
    InsufficientBalance = 8,
    TransferError = 9,
}

#[contractimpl]
impl SkillSyncContract {
    pub fn init(
        env: Env,
        admin: Address,
        platform_fee_bps: u32,
        treasury_address: Address,
        dispute_window_secs: u64,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::PlatformFee, &platform_fee_bps);
        env.storage()
            .instance()
            .set(&DataKey::Treasury, &treasury_address);
        env.storage()
            .instance()
            .set(&DataKey::DisputeWindow, &dispute_window_secs);
        env.storage().instance().set(&DataKey::Version, &VERSION);

        env.events().publish(
            (Symbol::new(&env, "Initialized"),),
            (
                admin,
                platform_fee_bps,
                treasury_address,
                dispute_window_secs,
                VERSION,
            ),
        );

        Ok(())
    }

    /// Stores a new session in persistent storage.
    ///
    /// This function implements a pre-insert guard to ensure session_id uniqueness.
    /// It prevents accidentally or maliciously creating duplicate sessions with the
    /// same session_id, which could lead to confusion or invalid state.
    ///
    /// # Arguments
    ///
    /// * `env` - The contract environment
    /// * `session` - The session to store. Must have a unique `session_id`.
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the session was successfully stored.
    /// - `Err(Error::DuplicateSessionId)` if a session with the same `session_id`
    ///   already exists in storage.
    ///
    /// # Panics
    ///
    /// This function does not panic. Use `try_put_session()` to recover from errors
    /// in calling code.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let result = contract.put_session(env, my_session);
    /// match result {
    ///     Ok(_) => {
    ///         // Session stored successfully
    ///     }
    ///     Err(Error::DuplicateSessionId) => {
    ///         // A session with this ID already exists
    ///         // Consider retrying with a new UUID
    ///     }
    ///     Err(other) => {
    ///         // Other error
    ///     }
    /// }
    /// ```
    pub fn put_session(env: Env, session: Session) -> Result<(), Error> {
        let key = DataKey::Session(session.session_id.clone());
        
        // Check if session_id already exists
        if env.storage().persistent().has(&key) {
            return Err(Error::DuplicateSessionId);
        }
        
        env.storage().persistent().set(&key, &session);
        Ok(())
    }

    pub fn get_session(env: Env, session_id: Vec<u8>) -> Option<Session> {
        env.storage().persistent().get(&DataKey::Session(session_id))
    }

    pub fn update_session_status(env: Env, session_id: Vec<u8>, new_status: SessionStatus, updated_at: u64) -> Result<(), ()> {
        let key = DataKey::Session(session_id.clone());
        match env.storage().persistent().get::<_, Session>(&key) {
            Some(mut s) => {
                s.status = new_status;
                s.updated_at = updated_at;
                env.storage().persistent().set(&key, &s);
                Ok(())
            }
            None => Err(()),
        }
    }

    pub fn ping(_env: Env) -> u32 {
        1
    }

    pub fn get_dispute_window(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::DisputeWindow)
            .unwrap_or(DEFAULT_DISPUTE_WINDOW_SECONDS)
    }

    pub fn get_treasury(env: Env) -> Address {
        // Return stored treasury address if present, otherwise fall back to admin.
        match env.storage().instance().get(&DataKey::Treasury) {
            Some(addr) => addr,
            None => match read_admin(&env) {
                Ok(admin) => admin,
                Err(_) => panic_with_error!(&env, Error::NotInitialized),
            },
        }
    }

    pub fn set_dispute_window(env: Env, seconds: u64) -> Result<(), Error> {
        let admin = read_admin(&env)?;
        admin.require_auth();

        validate_dispute_window(seconds)?;

        let old = Self::get_dispute_window(env.clone());
        env.storage()
            .instance()
            .set(&DataKey::DisputeWindow, &seconds);
        env.events()
            .publish((Symbol::new(&env, "DisputeWindowUpdated"),), (old, seconds));
        Ok(())
    }

    pub fn set_treasury(env: Env, new_addr: Address) -> Result<(), Error> {
        let admin = read_admin(&env)?;
        admin.require_auth();

        let old = match env.storage().instance().get(&DataKey::Treasury) {
            Some(addr) => addr,
            None => read_admin(&env)?,
        };

        env.storage().instance().set(&DataKey::Treasury, &new_addr);
        env.events()
            .publish((Symbol::new(&env, "TreasuryUpdated"),), (old, new_addr));
        Ok(())
    }

    /// Locks funds in escrow for a mentorship session.
    ///
    /// This function:
    /// 1. Validates all inputs (nonzero amount, distinct parties, unique session_id)
    /// 2. Checks and reserves platform fee based on fee_bps
    /// 3. Transfers total funds (amount + fee) from payer to contract's escrow
    /// 4. Creates and stores a Session struct with status=Locked
    /// 5. Emits a FundsLocked event
    ///
    /// # Arguments
    ///
    /// * `env` - The contract environment
    /// * `session_id` - Globally unique session identifier (must not already exist)
    /// * `payer` - Address of the mentor/service provider (sends funds)
    /// * `payee` - Address of the student/service receiver (receives funds on completion)
    /// * `asset` - Token address (must be a valid Soroban token contract)
    /// * `amount` - Session/service amount in stroops (must be > 0)
    /// * `fee_bps` - Platform fee in basis points (1 bps = 0.01%, max 10000 = 100%)
    ///
    /// # Returns
    ///
    /// - `Ok(())` if funds were successfully locked
    /// - `Err(Error::DuplicateSessionId)` if session_id already exists
    /// - `Err(Error::InvalidAmount)` if amount is zero or negative
    /// - `Err(Error::InsufficientBalance)` if payer doesn't have enough balance
    /// - `Err(Error::TransferError)` if token transfer fails
    ///
    /// # Events
    ///
    /// Emits `FundsLocked(session_id, payer, payee, amount, fee)` upon success
    ///
    /// # Example
    ///
    /// ```ignore
    /// let session_id = vec![&env, 0x01, 0x02, 0x03];
    /// let result = contract.lock_funds(
    ///     &env,
    ///     &session_id,
    ///     &mentor_addr,
    ///     &student_addr,
    ///     &token_addr,
    ///     10_000_000_i128,  // 10 USDC (6 decimals)
    ///     250_u32            // 2.5% fee
    /// );
    /// ```
    pub fn lock_funds(
        env: Env,
        session_id: Vec<u8>,
        payer: Address,
        payee: Address,
        asset: Address,
        amount: i128,
        fee_bps: u32,
    ) -> Result<(), Error> {
        // Validate inputs
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        if payer == payee {
            return Err(Error::InvalidAmount);
        }

        // Get current timestamp and dispute window
        let now = env.ledger().timestamp();
        let dispute_window = Self::get_dispute_window(env.clone());
        let dispute_deadline = now + dispute_window;

        // Calculate platform fee
        // fee = amount * fee_bps / 10000
        // Using checked arithmetic to prevent overflow
        let fee = amount
            .checked_mul(fee_bps as i128)
            .ok_or(Error::TransferError)?
            .checked_div(10000)
            .ok_or(Error::TransferError)?;

        let total_amount = amount
            .checked_add(fee)
            .ok_or(Error::TransferError)?;

        // Create token client for the asset
        let token_client = token::Client::new(&env, &asset);

        // Check payer's balance before transfer
        let payer_balance = token_client.balance(&payer);
        if payer_balance < total_amount {
            return Err(Error::InsufficientBalance);
        }

        // Create session struct
        let session = Session {
            version: 1,
            session_id: session_id.clone(),
            payer: payer.clone(),
            payee: payee.clone(),
            asset: asset.clone(),
            amount,
            fee_bps,
            status: SessionStatus::Locked,
            created_at: now,
            updated_at: now,
            dispute_deadline,
        };

        // Store session (this also checks for duplicate session_id)
        Self::put_session(env.clone(), session)?;

        // Transfer funds from payer to contract
        let contract_id = env.current_contract_address();
        token_client.transfer(&payer, &contract_id, &total_amount);

        // Emit FundsLocked event
        env.events().publish(
            (Symbol::new(&env, "FundsLocked"),),
            (session_id, payer, payee, amount, fee),
        );

        Ok(())
    }
}

fn read_admin(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotInitialized)
}

fn validate_dispute_window(seconds: u64) -> Result<(), Error> {
    if !(DISPUTE_WINDOW_MIN_SECONDS..=DISPUTE_WINDOW_MAX_SECONDS).contains(&seconds) {
        return Err(Error::InvalidDisputeWindow);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events},
        vec, Address, Env, IntoVal,
    };

    #[test]
    fn test_ping() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        assert_eq!(client.ping(), 1);
    }

    #[test]
    fn test_get_and_set_dispute_window_persists() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);
        assert_eq!(client.get_dispute_window(), DEFAULT_DISPUTE_WINDOW_SECONDS);

        let updated = 120_u64;
        client.set_dispute_window(&updated);
        assert_eq!(client.get_dispute_window(), updated);
    }

    #[test]
    fn test_set_dispute_window_below_min_reverts() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);
        let result = client.try_set_dispute_window(&(DISPUTE_WINDOW_MIN_SECONDS - 1));
        assert_eq!(result, Err(Ok(Error::InvalidDisputeWindow)));
    }

    #[test]
    fn test_set_dispute_window_above_max_reverts() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);
        let result = client.try_set_dispute_window(&(DISPUTE_WINDOW_MAX_SECONDS + 1));
        assert_eq!(result, Err(Ok(Error::InvalidDisputeWindow)));
    }

    #[test]
    fn test_set_dispute_window_requires_admin_auth() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);
        client.set_dispute_window(&120_u64);

        let auths = env.auths();
        assert_eq!(auths.len(), 1);
        assert_eq!(auths[0].0, admin);
    }

    #[test]
    fn test_set_dispute_window_emits_event_with_old_and_new() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        let old = DEFAULT_DISPUTE_WINDOW_SECONDS;
        let new = 600_u64;
        client.set_dispute_window(&new);

        assert_eq!(
            env.events().all(),
            vec![
                &env,
                (
                    contract_id.clone(),
                    (Symbol::new(&env, "Initialized"),).into_val(&env),
                    (admin, 100_u32, treasury, DEFAULT_DISPUTE_WINDOW_SECONDS, VERSION).into_val(&env)
                ),
                (
                    contract_id.clone(),
                    (Symbol::new(&env, "DisputeWindowUpdated"),).into_val(&env),
                    (old, new).into_val(&env)
                )
            ]
        );
    }

    #[test]
    fn test_get_and_set_treasury_persists() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);
        // Initially treasury should default to stored treasury
        assert_eq!(client.get_treasury(), treasury);

        let new_treasury = Address::generate(&env);
        client.set_treasury(&new_treasury);
        assert_eq!(client.get_treasury(), new_treasury);
    }

    #[test]
    fn test_set_treasury_requires_admin_auth() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);
        let new_treasury = Address::generate(&env);
        client.set_treasury(&new_treasury);

        let auths = env.auths();
        assert_eq!(auths.len(), 1);
        assert_eq!(auths[0].0, admin);
    }

    #[test]
    fn test_set_treasury_emits_event_with_old_and_new() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        let old = treasury.clone();
        let new = Address::generate(&env);
        client.set_treasury(&new);

        assert_eq!(
            env.events().all(),
            vec![
                &env,
                (
                    contract_id.clone(),
                    (Symbol::new(&env, "Initialized"),).into_val(&env),
                    (admin, 100_u32, treasury.clone(), DEFAULT_DISPUTE_WINDOW_SECONDS, VERSION).into_val(&env)
                ),
                (
                    contract_id.clone(),
                    (Symbol::new(&env, "TreasuryUpdated"),).into_val(&env),
                    (old, new).into_val(&env)
                )
            ]
        );
    }

    #[test]
    fn test_session_encode_decode_and_update() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let asset = Address::generate(&env);
        let session_id = vec![&env, 1u8, 2u8, 3u8];
        let amount: i128 = 1_000_000;
        let fee_bps: u32 = 250;
        let created_at: u64 = 1_000_000;

        let s = Session {
            version: 1,
            session_id: session_id.clone(),
            payer: payer.clone(),
            payee: payee.clone(),
            asset: asset.clone(),
            amount,
            fee_bps,
            status: SessionStatus::Pending,
            created_at,
            updated_at: created_at,
            dispute_deadline: created_at + DEFAULT_DISPUTE_WINDOW_SECONDS,
        };

        client.put_session(&s).unwrap();

        let got = client.get_session(&session_id);
        assert!(got.is_some());
        let got = got.unwrap();
        assert_eq!(got.version, 1);
        assert_eq!(got.session_id, session_id);
        assert_eq!(got.payer, payer);
        assert_eq!(got.payee, payee);
        assert_eq!(got.asset, asset);
        assert_eq!(got.amount, amount);
        assert_eq!(got.fee_bps, fee_bps);
        assert_eq!(got.status, SessionStatus::Pending);

        // update status
        let new_updated_at = created_at + 10;
        client.update_session_status(&session_id, &SessionStatus::Completed, &new_updated_at).unwrap();
        let got2 = client.get_session(&session_id).unwrap();
        assert_eq!(got2.status, SessionStatus::Completed);
        assert_eq!(got2.updated_at, new_updated_at);
    }

    #[test]
    fn test_session_storage_keys_are_collision_free() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let base_addr = Address::generate(&env);
        let sid1 = vec![&env, 1u8];
        let sid2 = vec![&env, 2u8];

        let s1 = Session {
            version: 1,
            session_id: sid1.clone(),
            payer: base_addr.clone(),
            payee: base_addr.clone(),
            asset: base_addr.clone(),
            amount: 10,
            fee_bps: 0,
            status: SessionStatus::Pending,
            created_at: 0,
            updated_at: 0,
            dispute_deadline: 0,
        };

        let s2 = Session { session_id: sid2.clone(), ..s1.clone() };

        client.put_session(&s1).unwrap();
        client.put_session(&s2).unwrap();

        let g1 = client.get_session(&sid1).unwrap();
        let g2 = client.get_session(&sid2).unwrap();
        assert_eq!(g1.session_id, sid1);
        assert_eq!(g2.session_id, sid2);
    }

    #[test]
    fn test_session_migration_compatibility_old_version_decodes() {
    fn test_init_stores_correct_values_and_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let platform_fee_bps = 250_u32;
        let dispute_window = 3600_u64;

        client.init(&admin, &platform_fee_bps, &treasury, &dispute_window);

        // Verify stored values. For admin/fee/version, there are no getters yet,
        // but getting dispute_window and treasury verifies they are stored correctly.
        assert_eq!(client.get_dispute_window(), dispute_window);
        assert_eq!(client.get_treasury(), treasury);

        // Verify event emitted
        let events = env.events().all();
        // Event should be the Initialized event
        assert_eq!(
            events,
            vec![
                &env,
                (
                    contract_id,
                    (Symbol::new(&env, "Initialized"),).into_val(&env),
                    (admin, platform_fee_bps, treasury, dispute_window, VERSION).into_val(&env)
                )
            ]
        );
    }

    #[test]
    fn test_init_twice_fails() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let addr = Address::generate(&env);
        let sid = vec![&env, 9u8, 9u8];

        // Simulate older-version session (version 0)
        let old = Session {
            version: 0,
            session_id: sid.clone(),
            payer: addr.clone(),
            payee: addr.clone(),
            asset: addr.clone(),
            amount: 0,
            fee_bps: 0,
            status: SessionStatus::Pending,
            created_at: 0,
            updated_at: 0,
            dispute_deadline: 0,
        };

        // store and ensure we can read back (decode) older versions
        client.put_session(&old).unwrap();
        let got = client.get_session(&sid).unwrap();
        assert_eq!(got.version, 0);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Second init should revert
        let result = client.try_init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);
        assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
    }

    #[test]
    fn test_put_session_happy_path() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let addr = Address::generate(&env);
        let sid = vec![&env, 42u8, 7u8, 13u8];

        let session = Session {
            version: 1,
            session_id: sid.clone(),
            payer: addr.clone(),
            payee: addr.clone(),
            asset: addr.clone(),
            amount: 500_000,
            fee_bps: 100,
            status: SessionStatus::Pending,
            created_at: 1000,
            updated_at: 1000,
            dispute_deadline: 86400,
        };

        // First insertion should succeed
        let result = client.put_session(&session);
        assert!(result.is_ok());

        // Verify session was stored
        let stored = client.get_session(&sid);
        assert!(stored.is_some());
        assert_eq!(stored.unwrap().session_id, sid);
    }

    #[test]
    fn test_put_session_rejects_duplicate_id() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let addr = Address::generate(&env);
        let sid = vec![&env, 99u8, 88u8];

        let session1 = Session {
            version: 1,
            session_id: sid.clone(),
            payer: addr.clone(),
            payee: addr.clone(),
            asset: addr.clone(),
            amount: 1_000_000,
            fee_bps: 250,
            status: SessionStatus::Pending,
            created_at: 5000,
            updated_at: 5000,
            dispute_deadline: 91400,
        };

        let mut session2 = session1.clone();
        session2.amount = 2_000_000; // Different amount, same ID

        // First insertion should succeed
        let result1 = client.put_session(&session1);
        assert!(result1.is_ok());

        // Second insertion with same session_id should fail
        let result2 = client.put_session(&session2);
        assert!(result2.is_err());
        assert_eq!(result2.unwrap_err(), Ok(Error::DuplicateSessionId));
    }

    #[test]
    fn test_put_session_allows_different_ids() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let addr = Address::generate(&env);
        let sid1 = vec![&env, 1u8, 1u8];
        let sid2 = vec![&env, 2u8, 2u8];
        let sid3 = vec![&env, 3u8, 3u8];

        let session1 = Session {
            version: 1,
            session_id: sid1.clone(),
            payer: addr.clone(),
            payee: addr.clone(),
            asset: addr.clone(),
            amount: 100,
            fee_bps: 0,
            status: SessionStatus::Pending,
            created_at: 0,
            updated_at: 0,
            dispute_deadline: 0,
        };

        let session2 = Session { session_id: sid2.clone(), ..session1.clone() };
        let session3 = Session { session_id: sid3.clone(), ..session1.clone() };

        // All different session_ids should be accepted
        assert!(client.put_session(&session1).is_ok());
        assert!(client.put_session(&session2).is_ok());
        assert!(client.put_session(&session3).is_ok());

        // Verify all three are stored
        assert!(client.get_session(&sid1).is_some());
        assert!(client.get_session(&sid2).is_some());
        assert!(client.get_session(&sid3).is_some());
    }

    #[test]
    fn test_put_session_multiple_duplicates_all_rejected() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let addr = Address::generate(&env);
        let sid = vec![&env, 123u8, 45u8, 67u8];

        let session = Session {
            version: 1,
            session_id: sid.clone(),
            payer: addr.clone(),
            payee: addr.clone(),
            asset: addr.clone(),
            amount: 1000,
            fee_bps: 50,
            status: SessionStatus::Pending,
            created_at: 0,
            updated_at: 0,
            dispute_deadline: 0,
        };

        // First insertion succeeds
        assert!(client.put_session(&session).is_ok());

        // All subsequent attempts with same ID should fail
        for _ in 0..3 {
            let mut session_attempt = session.clone();
            session_attempt.amount += 100; // Modify to try to sneak through
            assert_eq!(
                client.try_put_session(&session_attempt),
                Err(Ok(Error::DuplicateSessionId))
            );
        }
    }

    // Property-based tests with randomized session IDs
    // These tests verify that the duplicate check works correctly with various ID patterns

    #[test]
    fn test_put_session_randomized_ids_single_byte() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let addr = Address::generate(&env);
        let mut stored_ids = Vec::new();

        // Test with multiple single-byte session IDs (0-255 pattern)
        for i in 0u8..10u8 {
            let sid = vec![&env, i];
            let session = Session {
                version: 1,
                session_id: sid.clone(),
                payer: addr.clone(),
                payee: addr.clone(),
                asset: addr.clone(),
                amount: (i as i128) * 1000,
                fee_bps: 0,
                status: SessionStatus::Pending,
                created_at: i as u64,
                updated_at: i as u64,
                dispute_deadline: (i as u64) + 86400,
            };

            // Each unique ID should be accepted
            assert!(client.put_session(&session).is_ok(), 
                "Failed to insert session with ID {}", i);
            
            // Verify storage
            assert!(client.get_session(&sid).is_some());
            stored_ids.push(sid);
        }

        // Verify all IDs remain stored (one more check)
        for (idx, sid) in stored_ids.iter().enumerate() {
            let stored = client.get_session(sid);
            assert!(stored.is_some());
            assert_eq!(stored.unwrap().amount, (idx as i128) * 1000);
        }
    }

    #[test]
    fn test_put_session_randomized_ids_multi_byte() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let addr = Address::generate(&env);

        // Test with various multi-byte patterns simulating UUIDs or random IDs
        let id_patterns = vec![
            vec![&env, 0u8, 1u8, 2u8, 3u8],
            vec![&env, 255u8, 254u8, 253u8],
            vec![&env, 0x12u8, 0x34u8, 0x56u8, 0x78u8, 0x9au8],
            vec![&env, 0xddu8, 0xeeu8, 0xffu8],
            vec![&env, 1u8, 1u8, 1u8, 1u8, 1u8],
            vec![&env, 0u8, 0u8, 0u8, 0u8],
            vec![&env, 128u8, 64u8, 32u8, 16u8, 8u8, 4u8, 2u8, 1u8],
            vec![&env, 7u8, 14u8, 21u8, 28u8, 35u8],
        ];

        for (idx, sid) in id_patterns.iter().enumerate() {
            let session = Session {
                version: 1,
                session_id: sid.clone(),
                payer: addr.clone(),
                payee: addr.clone(),
                asset: addr.clone(),
                amount: (idx as i128) * 10000,
                fee_bps: 100,
                status: SessionStatus::Pending,
                created_at: (idx as u64) * 1000,
                updated_at: (idx as u64) * 1000,
                dispute_deadline: (idx as u64) * 1000 + 86400,
            };

            // Each unique pattern should be accepted
            assert!(client.put_session(&session).is_ok(),
                "Failed to insert session with pattern index {}", idx);

            // Verify it's stored
            assert!(client.get_session(sid).is_some());
        }

        // Verify none of them can be inserted again (duplicate check)
        for sid in id_patterns.iter() {
            let session = Session {
                version: 1,
                session_id: sid.clone(),
                payer: addr.clone(),
                payee: addr.clone(),
                asset: addr.clone(),
                amount: 999_999,
                fee_bps: 1,
                status: SessionStatus::Pending,
                created_at: 0,
                updated_at: 0,
                dispute_deadline: 0,
            };

            let result = client.try_put_session(&session);
            assert_eq!(result, Err(Ok(Error::DuplicateSessionId)),
                "Expected DuplicateSessionId error for existing ID");
        }
    }

    #[test]
    fn test_put_session_randomized_ids_large_ids() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let addr = Address::generate(&env);

        // Simulate large ID patterns (like SHA256 hashes or UUIDs)
        let large_ids = vec![
            vec![&env, 0x4du8, 0x6fu8, 0x9eu8, 0x8bu8, 0xcdu8, 0xf4u8, 0x2bu8, 0xa0u8, 
                 0x45u8, 0xcfu8, 0x15u8, 0x11u8, 0x6au8, 0x7bu8, 0xd8u8, 0xe9u8],
            vec![&env, 0xffu8, 0xeeu8, 0xddu8, 0xccu8, 0xbbu8, 0xaau8, 0x99u8, 0x88u8,
                 0x77u8, 0x66u8, 0x55u8, 0x44u8, 0x33u8, 0x22u8, 0x11u8, 0x00u8],
            vec![&env, 0x00u8, 0x11u8, 0x22u8, 0x33u8, 0x44u8, 0x55u8, 0x66u8, 0x77u8,
                 0x88u8, 0x99u8, 0xaau8, 0xbbu8, 0xccu8, 0xddu8, 0xeeu8, 0xffu8],
        ];

        for (idx, sid) in large_ids.iter().enumerate() {
            let session = Session {
                version: 1,
                session_id: sid.clone(),
                payer: addr.clone(),
                payee: addr.clone(),
                asset: addr.clone(),
                amount: 5_000_000 + (idx as i128),
                fee_bps: 250,
                status: SessionStatus::Pending,
                created_at: 1_000_000,
                updated_at: 1_000_000,
                dispute_deadline: 1_086_400,
            };

            assert!(client.put_session(&session).is_ok(),
                "Failed to insert large ID pattern {}", idx);
            assert!(client.get_session(sid).is_some());
        }

        // Verify none can be re-inserted
        for sid in large_ids.iter() {
            let session = Session {
                version: 1,
                session_id: sid.clone(),
                payer: addr.clone(),
                payee: addr.clone(),
                asset: addr.clone(),
                amount: 1,
                fee_bps: 0,
                status: SessionStatus::Pending,
                created_at: 0,
                updated_at: 0,
                dispute_deadline: 0,
            };

            assert_eq!(
                client.try_put_session(&session),
                Err(Ok(Error::DuplicateSessionId))
            );
        }
    }

    #[test]
    fn test_put_session_edge_case_empty_like_id() {
        // Test with minimal-length IDs to ensure edge cases are covered
        let env = Env::default();
        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let addr = Address::generate(&env);

        // Single byte minimal ID
        let sid_min = vec![&env, 0u8];
        let session_min = Session {
            version: 1,
            session_id: sid_min.clone(),
            payer: addr.clone(),
            payee: addr.clone(),
            asset: addr.clone(),
            amount: 100,
            fee_bps: 0,
            status: SessionStatus::Pending,
            created_at: 0,
            updated_at: 0,
            dispute_deadline: 0,
        };

        assert!(client.put_session(&session_min).is_ok());
        assert!(client.get_session(&sid_min).is_some());

        // Attempting duplicate should fail
        assert_eq!(
            client.try_put_session(&session_min),
            Err(Ok(Error::DuplicateSessionId))
        );
    }

    // Tests for lock_funds functionality
    // =================================

    #[test]
    fn test_lock_funds_happy_path() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Setup addresses and token
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_admin = Address::generate(&env);

        // Create a token client with test utils
        let token_id = Address::from_contract_id(&env, &token_contract);
        let token_client = token::Client::new(&env, &token_id);

        // Mint tokens to payer
        token_client.mint(&payer, &(10_000_000_i128));

        let session_id = vec![&env, 1u8, 2u8, 3u8];
        let amount = 1_000_000_i128;
        let fee_bps = 250u32; // 2.5%

        let result = client.lock_funds(
            &session_id,
            &payer,
            &payee,
            &token_id,
            &amount,
            &fee_bps,
        );

        assert!(result.is_ok());

        // Verify session was created and stored
        let stored_session = client.get_session(&session_id);
        assert!(stored_session.is_some());
        let session = stored_session.unwrap();
        assert_eq!(session.session_id, session_id);
        assert_eq!(session.payer, payer);
        assert_eq!(session.payee, payee);
        assert_eq!(session.asset, token_id);
        assert_eq!(session.amount, amount);
        assert_eq!(session.fee_bps, fee_bps);
        assert_eq!(session.status, SessionStatus::Locked);
    }

    #[test]
    fn test_lock_funds_rejects_zero_amount() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);
        let session_id = vec![&env, 5u8, 6u8];

        let result = client.try_lock_funds(
            &session_id,
            &payer,
            &payee,
            &token_id,
            &0i128, // Zero amount
            &100u32,
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Ok(Error::InvalidAmount));
    }

    #[test]
    fn test_lock_funds_rejects_negative_amount() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);
        let session_id = vec![&env, 7u8, 8u8];

        let result = client.try_lock_funds(
            &session_id,
            &payer,
            &payee,
            &token_id,
            &(-1_000_000i128), // Negative amount
            &100u32,
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Ok(Error::InvalidAmount));
    }

    #[test]
    fn test_lock_funds_rejects_duplicate_session_id() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let payee2 = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);

        // Mint tokens to payer
        let token_client = token::Client::new(&env, &token_id);
        token_client.mint(&payer, &(50_000_000_i128));

        let session_id = vec![&env, 10u8, 11u8];
        let amount = 1_000_000_i128;
        let fee_bps = 100u32;

        // First lock should succeed
        let result1 = client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);
        assert!(result1.is_ok());

        // Second lock with same session_id should fail
        let result2 = client.try_lock_funds(
            &session_id,
            &payer,
            &payee2,
            &token_id,
            &amount,
            &fee_bps,
        );
        assert!(result2.is_err());
        assert_eq!(result2.unwrap_err(), Ok(Error::DuplicateSessionId));
    }

    #[test]
    fn test_lock_funds_sufficient_balance() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);

        // Mint exactly enough for amount + fee
        let token_client = token::Client::new(&env, &token_id);
        let amount = 1_000_000_i128;
        let fee_bps = 250u32; // 2.5%
        let fee = (amount * fee_bps as i128) / 10000; // 25000
        let total = amount + fee;

        token_client.mint(&payer, &total);

        let session_id = vec![&env, 12u8, 13u8, 14u8];
        let result = client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        assert!(result.is_ok());
    }

    #[test]
    fn test_lock_funds_insufficient_balance() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);

        // Mint tokens but not enough for amount + fee
        let token_client = token::Client::new(&env, &token_id);
        let amount = 1_000_000_i128;
        let fee_bps = 250u32;
        let fee = (amount * fee_bps as i128) / 10000;
        let total_needed = amount + fee;

        // Only mint 90% of needed amount
        token_client.mint(&payer, &(total_needed * 9 / 10));

        let session_id = vec![&env, 15u8, 16u8];
        let result = client.try_lock_funds(
            &session_id,
            &payer,
            &payee,
            &token_id,
            &amount,
            &fee_bps,
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Ok(Error::InsufficientBalance));
    }

    #[test]
    fn test_lock_funds_platform_fee_calculation() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);

        let token_client = token::Client::new(&env, &token_id);

        // Test various fee scenarios
        let test_cases = vec![
            (1_000_000i128, 0u32, 0i128),        // 0% fee
            (1_000_000i128, 100u32, 10_000i128), // 1% fee = 10,000
            (1_000_000i128, 250u32, 25_000i128), // 2.5% fee = 25,000
            (1_000_000i128, 500u32, 50_000i128), // 5% fee = 50,000
            (1_000_000i128, 1000u32, 100_000i128), // 10% fee = 100,000
            (10_000_000i128, 500u32, 500_000i128), // 5% of 10M = 500,000
        ];

        for (idx, (amount, fee_bps, expected_fee)) in test_cases.iter().enumerate() {
            token_client.mint(&payer, &(amount + expected_fee + 100_000)); // Add buffer

            let session_id = vec![&env, 20u8 + (idx as u8), 21u8];
            let result = client.lock_funds(&session_id, &payer, &payee, &token_id, amount, fee_bps);
            assert!(result.is_ok(), "Failed for test case {}", idx);

            // Verify stored session has correct amounts
            let session = client.get_session(&session_id).unwrap();
            assert_eq!(session.amount, *amount);
            assert_eq!(session.fee_bps, *fee_bps);
        }
    }

    #[test]
    fn test_lock_funds_creates_session_with_correct_timestamp() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);

        let token_client = token::Client::new(&env, &token_id);
        token_client.mint(&payer, &(10_000_000_i128));

        // Set a specific ledger timestamp
        let (current_block, _slot) = env.ledger().sequence_and_timestamp();
        let timestamp = 1_000_000u64;
        env.ledger().set_timestamp(timestamp);

        let session_id = vec![&env, 30u8, 31u8];
        let amount = 1_000_000i128;

        let result = client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &100u32);
        assert!(result.is_ok());

        let session = client.get_session(&session_id).unwrap();
        assert_eq!(session.created_at, timestamp);
        assert_eq!(session.updated_at, timestamp);
        assert_eq!(session.status, SessionStatus::Locked);
    }

    #[test]
    fn test_lock_funds_emits_event() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);

        let token_client = token::Client::new(&env, &token_id);
        token_client.mint(&payer, &(10_000_000_i128));

        env.events().publish((), ()); // Clear event buffer

        let session_id = vec![&env, 40u8, 41u8, 42u8];
        let amount = 1_000_000i128;
        let fee_bps = 250u32;
        let expected_fee = (amount * fee_bps as i128) / 10000;

        let result = client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);
        assert!(result.is_ok());

        // Verify FundsLocked event was emitted
        let events = env.events().all();
        
        // Find the FundsLocked event (skip the mint events)
        let mut found_event = false;
        for event in events {
            if let Some(topics) = event.2.get(0) {
                if let Ok(symbol) = Symbol::try_from(topics) {
                    if symbol.to_string(&env) == Some("FundsLocked".to_string()) {
                        found_event = true;
                        break;
                    }
                }
            }
        }
        assert!(found_event, "FundsLocked event not found");
    }

    #[test]
    fn test_lock_funds_multiple_sessions_different_parties() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let token_contract = env.register_stellar_asset_contract(Address::generate(&env));
        let token_id = Address::from_contract_id(&env, &token_contract);
        let token_client = token::Client::new(&env, &token_id);

        // Create multiple sessions with different parties
        let base_payer = Address::generate(&env);
        token_client.mint(&base_payer, &(100_000_000_i128));

        for i in 0..5 {
            let payer = if i == 0 { base_payer.clone() } else { Address::generate(&env) };
            if i > 0 {
                token_client.mint(&payer, &(10_000_000_i128));
            }

            let payee = Address::generate(&env);
            let session_id = vec![&env, 50u8 + (i as u8), 51u8];
            let amount = 1_000_000i128 + (i as i128 * 100_000);

            let result = client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &100u32);
            assert!(result.is_ok(), "Failed to lock funds for session {}", i);

            let session = client.get_session(&session_id).unwrap();
            assert_eq!(session.payer, payer);
            assert_eq!(session.payee, payee);
            assert_eq!(session.amount, amount);
        }
    }

    #[test]
    fn test_lock_funds_max_fee_calculation() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);

        let token_client = token::Client::new(&env, &token_id);

        // Test maximum fee (10000 bps = 100%)
        let amount = 1_000_000i128;
        let fee_bps = 10000u32; // 100% fee
        let expected_fee = amount; // 100% of amount

        token_client.mint(&payer, &(amount * 2 + 100_000)); // Need double for 100% fee

        let session_id = vec![&env, 60u8, 61u8];
        let result = client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);
        assert!(result.is_ok());

        let session = client.get_session(&session_id).unwrap();
        assert_eq!(session.fee_bps, fee_bps);
    }
}
