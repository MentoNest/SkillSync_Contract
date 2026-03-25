#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, token, Address, Bytes,
    Env, Map, Symbol, Vec,
};

pub const DISPUTE_WINDOW_MIN_SECONDS: u64 = 60;
pub const DISPUTE_WINDOW_MAX_SECONDS: u64 = 30 * 24 * 60 * 60;
pub const DEFAULT_DISPUTE_WINDOW_SECONDS: u64 = 24 * 60 * 60;
pub const PLATFORM_FEE_MAX_BPS: u32 = 1000; // 10%
pub const ESCROW_DURATION_SECONDS: u64 = 7 * 24 * 60 * 60; // Default 7 days
pub const SECONDS_PER_DAY: u64 = 24 * 60 * 60;
pub const MIN_UPGRADE_TIMELOCK_SECONDS: u64 = 60; // Minimum 1 minute timelock
pub const DEFAULT_UPGRADE_TIMELOCK_SECONDS: u64 = 24 * 60 * 60; // Default 1 day timelock

#[contract]
pub struct SkillSyncContract;

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    DisputeWindow,
    PlatformFee,
    Treasury,
    Version,
    Session(Bytes),
    // Expiry index: groups sessions by expiry day bucket (timestamp / SECONDS_PER_DAY)
    ExpiryIndex(u64),
    // Track which day buckets have been processed for pagination
    LastProcessedExpiryBucket,
    // Upgradeability storage keys
    PendingUpgrade,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SessionStatus {
    Pending = 0,
    Completed = 1,
    Disputed = 2,
    Cancelled = 3,
    Locked = 4,
}

/// Pending upgrade information for 2-phase commit upgrade pattern
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingUpgrade {
    /// Hash of the new WASM code to upgrade to
    pub new_wasm_hash: Bytes,
    /// Timestamp when upgrade was proposed
    pub proposed_at: u64,
    /// Timestamp when upgrade can be applied (deadline)
    /// If 0, no timelock is enforced
    pub deadline: u64,
    /// Block height at proposal time for additional safety
    pub proposed_at_ledger: u32,
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
    pub session_id: Bytes,
    pub payer: Address,
    pub payee: Address,
    pub asset: Address,
    pub amount: i128,
    pub fee_bps: u32,
    pub status: SessionStatus,
    pub created_at: u64,
    pub updated_at: u64,
    pub dispute_deadline: u64,
    pub expires_at: u64, // Timestamp when escrow can be auto-refunded
    pub payer_approved: bool,
    pub payee_approved: bool,
    pub approved_at: u64,
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
    InvalidFeeBps = 10,
    SessionNotFound = 11,
    DisputeWindowNotElapsed = 12,
    NotAuthorizedParty = 13,
    AlreadyApproved = 14,
    InvalidSessionStatus = 15,
    SessionNotExpired = 16,     // Session has not yet expired
    RefundFailed = 17,          // Failed to refund escrow
    NothingToSweep = 18,        // No expired sessions to sweep
    UpgradeNotProposed = 19,    // No upgrade has been proposed
    UpgradeNotReady = 20,       // Upgrade timelock has not elapsed
    UpgradeDeadlinePassed = 21, // Upgrade deadline has passed
    InvalidTimelock = 22,       // Invalid timelock duration
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

        validate_platform_fee_bps(platform_fee_bps)?;
        validate_dispute_window(dispute_window_secs)?;

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
    /// Enforces session_id uniqueness via a pre-insert guard.
    /// Returns `Err(DuplicateSessionId)` if the session_id already exists.
    pub fn put_session(env: Env, session: Session) -> Result<(), Error> {
        let key = DataKey::Session(session.session_id.clone());

        // Check if session_id already exists
        if env.storage().persistent().has(&key) {
            return Err(Error::DuplicateSessionId);
        }

        env.storage().persistent().set(&key, &session);
        Ok(())
    }

    pub fn get_session(env: Env, session_id: Bytes) -> Option<Session> {
        env.storage()
            .persistent()
            .get(&DataKey::Session(session_id))
    }

    pub fn update_session_status(
        env: Env,
        session_id: Bytes,
        new_status: SessionStatus,
        updated_at: u64,
    ) -> Result<(), Error> {
        let key = DataKey::Session(session_id.clone());
        match env.storage().persistent().get::<_, Session>(&key) {
            Some(mut s) => {
                s.status = new_status;
                s.updated_at = updated_at;
                env.storage().persistent().set(&key, &s);
                Ok(())
            }
            None => Err(Error::SessionNotFound),
        }
    }

    // ============================================================================
    // Upgradeability Functions
    // ============================================================================

    /// Proposes a contract upgrade with optional timelock.
    ///
    /// This function implements the first phase of a 2-phase commit upgrade pattern.
    /// Only the admin can propose upgrades. The timelock provides a safety window
    /// during which the upgrade can be reviewed before application.
    ///
    /// # Arguments
    ///
    /// * `env` - The contract environment
    /// * `new_wasm_hash` - The hash of the new WASM code to upgrade to
    /// * `timelock_seconds` - Optional timelock duration in seconds. If 0, uses default.
    ///
    /// # Returns
    ///
    /// - `Ok(())` if upgrade was successfully proposed
    /// - `Err(Error::Unauthorized)` if caller is not admin
    /// - `Err(Error::InvalidTimelock)` if timelock is too short
    ///
    /// # Events
    ///
    /// Emits `UpgradeProposed(new_wasm_hash, deadline, proposed_at)` upon success
    pub fn propose_upgrade(
        env: Env,
        new_wasm_hash: Bytes,
        timelock_seconds: u64,
    ) -> Result<(), Error> {
        // Require admin authorization
        let admin = read_admin(&env)?;
        admin.require_auth();

        // Validate timelock (use default if 0)
        let timelock = if timelock_seconds == 0 {
            DEFAULT_UPGRADE_TIMELOCK_SECONDS
        } else {
            timelock_seconds
        };

        if timelock < MIN_UPGRADE_TIMELOCK_SECONDS {
            return Err(Error::InvalidTimelock);
        }

        let now = env.ledger().timestamp();
        let deadline = now + timelock;
        let proposed_at_ledger = env.ledger().sequence();

        // Store pending upgrade
        let pending = PendingUpgrade {
            new_wasm_hash: new_wasm_hash.clone(),
            proposed_at: now,
            deadline,
            proposed_at_ledger,
        };

        env.storage()
            .instance()
            .set(&DataKey::PendingUpgrade, &pending);

        // Emit UpgradeProposed event
        env.events().publish(
            (Symbol::new(&env, "UpgradeProposed"),),
            (new_wasm_hash, deadline, now, proposed_at_ledger),
        );

        Ok(())
    }

    /// Applies a previously proposed upgrade.
    ///
    /// This function implements the second phase of the 2-phase commit upgrade pattern.
    /// Can only be called after the timelock has elapsed but before the deadline.
    /// Only the admin can apply upgrades.
    ///
    /// # Arguments
    ///
    /// * `env` - The contract environment
    ///
    /// # Returns
    ///
    /// - `Ok(())` if upgrade was successfully applied
    /// - `Err(Error::Unauthorized)` if caller is not admin
    /// - `Err(Error::UpgradeNotProposed)` if no upgrade has been proposed
    /// - `Err(Error::UpgradeNotReady)` if timelock has not elapsed
    /// - `Err(Error::UpgradeDeadlinePassed)` if deadline has passed
    ///
    /// # Events
    ///
    /// Emits `Upgraded(old_version, new_version, wasm_hash)` upon success
    pub fn apply_upgrade(env: Env) -> Result<(), Error> {
        // Require admin authorization
        let admin = read_admin(&env)?;
        admin.require_auth();

        // Get pending upgrade
        let pending: PendingUpgrade = env
            .storage()
            .instance()
            .get(&DataKey::PendingUpgrade)
            .ok_or(Error::UpgradeNotProposed)?;

        let now = env.ledger().timestamp();

        // Check timelock has elapsed
        if now < pending.deadline {
            return Err(Error::UpgradeNotReady);
        }

        // Get current version and calculate new version
        let old_version: u32 = env
            .storage()
            .instance()
            .get(&DataKey::Version)
            .unwrap_or(VERSION);
        let new_version = old_version + 1;

        // Update version
        env.storage()
            .instance()
            .set(&DataKey::Version, &new_version);

        // Clear pending upgrade
        env.storage().instance().remove(&DataKey::PendingUpgrade);

        // Emit Upgraded event
        env.events().publish(
            (Symbol::new(&env, "Upgraded"),),
            (old_version, new_version, pending.new_wasm_hash, now),
        );

        Ok(())
    }

    /// Returns the current contract version.
    pub fn get_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::Version)
            .unwrap_or(VERSION)
    }

    /// Returns the pending upgrade information if any.
    pub fn get_pending_upgrade(env: Env) -> Option<PendingUpgrade> {
        env.storage().instance().get(&DataKey::PendingUpgrade)
    }

    /// Cancels a pending upgrade.
    ///
    /// Only the admin can cancel an upgrade.
    ///
    /// # Arguments
    ///
    /// * `env` - The contract environment
    ///
    /// # Returns
    ///
    /// - `Ok(())` if upgrade was successfully cancelled
    /// - `Err(Error::Unauthorized)` if caller is not admin
    /// - `Err(Error::UpgradeNotProposed)` if no upgrade has been proposed
    ///
    /// # Events
    ///
    /// Emits `UpgradeCancelled(wasm_hash, cancelled_at)` upon success
    pub fn cancel_upgrade(env: Env) -> Result<(), Error> {
        // Require admin authorization
        let admin = read_admin(&env)?;
        admin.require_auth();

        // Get pending upgrade
        let pending: PendingUpgrade = env
            .storage()
            .instance()
            .get(&DataKey::PendingUpgrade)
            .ok_or(Error::UpgradeNotProposed)?;

        let now = env.ledger().timestamp();

        // Clear pending upgrade
        env.storage().instance().remove(&DataKey::PendingUpgrade);

        // Emit UpgradeCancelled event
        env.events().publish(
            (Symbol::new(&env, "UpgradeCancelled"),),
            (pending.new_wasm_hash, now),
        );

        Ok(())
    }

    // ============================================================================
    // End Upgradeability Functions
    // ============================================================================

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

    pub fn get_platform_fee_bps(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::PlatformFee)
            .unwrap_or(0)
    }

    pub fn set_platform_fee_bps(env: Env, new_bps: u32) -> Result<(), Error> {
        let admin = read_admin(&env)?;
        admin.require_auth();

        validate_platform_fee_bps(new_bps)?;

        let old = Self::get_platform_fee_bps(env.clone());
        env.storage()
            .instance()
            .set(&DataKey::PlatformFee, &new_bps);
        env.events()
            .publish((Symbol::new(&env, "PlatformFeeUpdated"),), (old, new_bps));
        Ok(())
    }

    /// Locks funds in escrow for a mentorship session.
    ///
    /// Validates inputs, calculates the platform fee (`amount * fee_bps / 10000`),
    /// checks payer balance, stores the session, transfers `amount + fee` from
    /// payer to the contract, and emits a `FundsLocked` event.
    ///
    /// Errors: `InvalidAmount`, `DuplicateSessionId`, `InsufficientBalance`,
    /// `TransferError`.
    pub fn lock_funds(
        env: Env,
        session_id: Bytes,
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
        let expires_at = now + ESCROW_DURATION_SECONDS; // Default escrow duration

        // Calculate platform fee
        // fee = amount * fee_bps / 10000
        // Using checked arithmetic to prevent overflow
        let fee = amount
            .checked_mul(fee_bps as i128)
            .ok_or(Error::TransferError)?
            .checked_div(10000)
            .ok_or(Error::TransferError)?;

        let total_amount = amount.checked_add(fee).ok_or(Error::TransferError)?;

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
            expires_at,
            payer_approved: false,
            payee_approved: false,
            approved_at: 0,
        };

        // Store session (this also checks for duplicate session_id)
        Self::put_session(env.clone(), session.clone())?;

        // Add session to expiry index
        Self::add_to_expiry_index(env.clone(), session_id.clone(), expires_at)?;

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

    /// Completes a session and releases escrowed funds to the payee.
    ///
    /// This function:
    /// 1. Validates session exists and status is Locked
    /// 2. Checks that dispute window has elapsed or both parties agreed
    /// 3. Transfers net amount (amount) to payee
    /// 4. Transfers platform fee to treasury
    /// 5. Updates session status to Completed
    /// 6. Emits a SessionCompleted event
    ///
    /// # Arguments
    ///
    /// * `env` - The contract environment
    /// * `session_id` - The unique session identifier
    /// * `caller` - Address initiating the completion (must be authorized)
    ///
    /// # Returns
    ///
    /// - `Ok(())` if session was successfully completed
    /// - `Err(Error::SessionNotFound)` if session doesn't exist
    /// - `Err(Error::InvalidSessionStatus)` if session status is not Locked
    /// - `Err(Error::DisputeWindowNotElapsed)` if dispute window hasn't passed
    /// - `Err(Error::TransferError)` if token transfer fails
    ///
    /// # Events
    ///
    /// Emits `SessionCompleted(session_id, payee, amount, fee)` upon success
    pub fn complete_session(env: Env, session_id: Vec<u8>, caller: Address) -> Result<(), Error> {
        // Require caller authorization
        caller.require_auth();

        // Retrieve session
        let mut session =
            Self::get_session(env.clone(), session_id.clone()).ok_or(Error::SessionNotFound)?;

        // Validate session status is Locked
        if session.status != SessionStatus::Locked {
            return Err(Error::InvalidSessionStatus);
        }

        // Check dispute window has elapsed OR both parties approved
        let now = env.ledger().timestamp();
        let both_approved = session.payer_approved && session.payee_approved;

        if !both_approved && now < session.dispute_deadline {
            return Err(Error::DisputeWindowNotElapsed);
        }

        // Calculate fee
        let fee = session
            .amount
            .checked_mul(session.fee_bps as i128)
            .ok_or(Error::TransferError)?
            .checked_div(10000)
            .ok_or(Error::TransferError)?;

        // Get treasury address
        let treasury = Self::get_treasury(env.clone());

        // Create token client
        let token_client = token::Client::new(&env, &session.asset);
        let contract_id = env.current_contract_address();

        // Transfer net amount to payee
        token_client.transfer(&contract_id, &session.payee, &session.amount);

        // Transfer fee to treasury
        if fee > 0 {
            token_client.transfer(&contract_id, &treasury, &fee);
        }

        // Update session status
        session.status = SessionStatus::Completed;
        session.updated_at = now;

        let key = DataKey::Session(session_id.clone());
        env.storage().persistent().set(&key, &session);

        // Remove from expiry index since session is completed
        Self::remove_from_expiry_index(env.clone(), session_id.clone(), session.expires_at)?;

        // Emit SessionCompleted event
        env.events().publish(
            (Symbol::new(&env, "SessionCompleted"),),
            (session_id, session.payee.clone(), session.amount, fee),
        );

        Ok(())
    }

    /// Approves a session by one of the parties (payer or payee).
    ///
    /// This function:
    /// 1. Validates session exists and status is Locked
    /// 2. Verifies caller is either payer or payee
    /// 3. Prevents duplicate approvals by the same party
    /// 4. Marks the appropriate approval flag (payer_approved or payee_approved)
    /// 5. If both parties approve, sets approved_at timestamp
    /// 6. Emits SessionApproved event
    ///
    /// When both parties approve, the session can be completed early
    /// (before dispute window ends) via complete_session().
    ///
    /// # Arguments
    ///
    /// * `env` - The contract environment
    /// * `session_id` - The unique session identifier
    /// * `approver` - Address of the party approving (must be payer or payee)
    ///
    /// # Returns
    ///
    /// - `Ok(())` if approval was successfully recorded
    /// - `Err(Error::SessionNotFound)` if session doesn't exist
    /// - `Err(Error::InvalidSessionStatus)` if session status is not Locked
    /// - `Err(Error::NotAuthorizedParty)` if approver is neither payer nor payee
    /// - `Err(Error::AlreadyApproved)` if this party already approved
    ///
    /// # Events
    ///
    /// Emits `SessionApproved(session_id, approver, both_approved)` upon success
    pub fn approve_session(env: Env, session_id: Vec<u8>, approver: Address) -> Result<(), Error> {
        // Require approver authorization
        approver.require_auth();

        // Retrieve session
        let mut session =
            Self::get_session(env.clone(), session_id.clone()).ok_or(Error::SessionNotFound)?;

        // Validate session status is Locked
        if session.status != SessionStatus::Locked {
            return Err(Error::InvalidSessionStatus);
        }

        // Determine which party is approving
        let is_payer = approver == session.payer;
        let is_payee = approver == session.payee;

        if !is_payer && !is_payee {
            return Err(Error::NotAuthorizedParty);
        }

        // Check for duplicate approval
        if is_payer && session.payer_approved {
            return Err(Error::AlreadyApproved);
        }
        if is_payee && session.payee_approved {
            return Err(Error::AlreadyApproved);
        }

        // Mark approval
        if is_payer {
            session.payer_approved = true;
        }
        if is_payee {
            session.payee_approved = true;
        }

        // Update timestamp
        let now = env.ledger().timestamp();
        session.updated_at = now;

        // If both parties approved, set approved_at
        let both_approved = session.payer_approved && session.payee_approved;
        if both_approved && session.approved_at == 0 {
            session.approved_at = now;
        }

        // Save updated session
        let key = DataKey::Session(session_id.clone());
        env.storage().persistent().set(&key, &session);

        // Emit SessionApproved event
        env.events().publish(
            (Symbol::new(&env, "SessionApproved"),),
            (session_id, approver, both_approved),
        );

        Ok(())
    }

    /// Adds a session to the expiry index for timeout tracking.
    /// Sessions are grouped by day bucket for efficient pagination.
    fn add_to_expiry_index(env: Env, session_id: Bytes, expires_at: u64) -> Result<(), Error> {
        // Calculate day bucket (expires_at / seconds per day)
        let day_bucket = expires_at / SECONDS_PER_DAY;
        let key = DataKey::ExpiryIndex(day_bucket);

        // Get or create the list of session IDs for this bucket
        let mut session_ids: Vec<Bytes> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));

        // Add session_id if not already present
        if !session_ids.contains(&session_id) {
            session_ids.push_back(session_id);
            env.storage().persistent().set(&key, &session_ids);
        }

        Ok(())
    }

    /// Removes a session from the expiry index.
    fn remove_from_expiry_index(env: Env, session_id: Bytes, expires_at: u64) -> Result<(), Error> {
        let day_bucket = expires_at / SECONDS_PER_DAY;
        let key = DataKey::ExpiryIndex(day_bucket);

        if let Some(mut session_ids) = env.storage().persistent().get::<_, Vec<Bytes>>(&key) {
            // Create new vec without the session_id
            let mut new_ids = Vec::new(&env);
            for i in 0..session_ids.len() {
                let id = session_ids.get(i).unwrap();
                if id != session_id {
                    new_ids.push_back(id);
                }
            }

            if new_ids.is_empty() {
                env.storage().persistent().remove(&key);
            } else {
                env.storage().persistent().set(&key, &new_ids);
            }
        }

        Ok(())
    }

    /// Sweeps expired escrows and refunds funds to payers.
    ///
    /// This function:
    /// 1. Gets the current ledger timestamp
    /// 2. Iterates over expiry day buckets up to the current day
    /// 3. For each expired session, calls internal_refund
    /// 4. Returns the number of sessions processed
    ///
    /// The `batch` parameter limits how many sessions to process in one call
    /// to stay within budget limits.
    ///
    /// Anyone can call this function - it's permissionless for automation.
    pub fn sweep_timeouts(env: Env, batch: u32) -> Result<u32, Error> {
        let now = env.ledger().timestamp();
        let current_day_bucket = now / SECONDS_PER_DAY;

        // Get the last processed bucket (start from 0 if not set)
        let last_processed = env
            .storage()
            .instance()
            .get::<_, u64>(&DataKey::LastProcessedExpiryBucket)
            .unwrap_or(0);

        let mut processed: u32 = 0;
        let mut current_bucket = last_processed;

        // Iterate through day buckets up to current day, respecting batch limit
        while current_bucket < current_day_bucket && processed < batch {
            let key = DataKey::ExpiryIndex(current_bucket);

            if let Some(session_ids) = env.storage().persistent().get::<_, Vec<Bytes>>(&key) {
                // Process each session in this bucket
                for i in 0..session_ids.len() {
                    if processed >= batch {
                        break;
                    }

                    let session_id = session_ids.get(i).unwrap();

                    // Try to refund this session if it exists and is expired
                    if let Some(session) = Self::get_session(env.clone(), session_id.clone()) {
                        // Check if session is actually expired (status is Locked and past expires_at)
                        if session.status == SessionStatus::Locked && session.expires_at <= now {
                            // Attempt refund - internal_refund will handle idempotency
                            match Self::internal_refund(env.clone(), session_id.clone()) {
                                Ok(_) => {
                                    processed += 1;
                                }
                                Err(e) => {
                                    // Log error but continue with next session
                                    // This ensures partial success even if one fails
                                    if e != Error::SessionNotFound
                                        && e != Error::InvalidSessionStatus
                                    {
                                        return Err(e);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            current_bucket += 1;
        }

        // Update the last processed bucket
        if current_bucket > last_processed {
            // Only update if we made progress
            let new_last = if processed >= batch {
                // If we hit batch limit, stay at current bucket for next call
                current_bucket - 1
            } else {
                current_bucket
            };
            env.storage()
                .instance()
                .set(&DataKey::LastProcessedExpiryBucket, &new_last);
        }

        Ok(processed)
    }

    /// Internal function to refund an expired escrow.
    /// Transfers funds back to the payer.
    ///
    /// This function is idempotent - calling it multiple times on the same
    /// session will not cause issues (the session will be marked as Cancelled
    /// after first successful refund).
    fn internal_refund(env: Env, session_id: Bytes) -> Result<i128, Error> {
        // Retrieve session
        let session =
            Self::get_session(env.clone(), session_id.clone()).ok_or(Error::SessionNotFound)?;

        // Only refund Locked sessions that have expired
        if session.status != SessionStatus::Locked {
            return Err(Error::InvalidSessionStatus);
        }

        let now = env.ledger().timestamp();
        if session.expires_at > now {
            return Err(Error::SessionNotExpired);
        }

        // Calculate total amount to refund (amount + fee)
        let fee = session
            .amount
            .checked_mul(session.fee_bps as i128)
            .ok_or(Error::TransferError)?
            .checked_div(10000)
            .ok_or(Error::TransferError)?;

        let total_refund = session
            .amount
            .checked_add(fee)
            .ok_or(Error::TransferError)?;

        // Create token client
        let token_client = token::Client::new(&env, &session.asset);
        let contract_id = env.current_contract_address();

        // Transfer refund to payer
        token_client.transfer(&contract_id, &session.payer, &total_refund);

        // Update session status to Cancelled
        let mut updated_session = session.clone();
        updated_session.status = SessionStatus::Cancelled;
        updated_session.updated_at = now;

        let key = DataKey::Session(session_id.clone());
        env.storage().persistent().set(&key, &updated_session);

        // Remove from expiry index
        Self::remove_from_expiry_index(env.clone(), session_id.clone(), session.expires_at)?;

        // Emit TimeoutRefunded event
        env.events().publish(
            (Symbol::new(&env, "TimeoutRefunded"),),
            (session_id, session.payer, total_refund),
        );

        Ok(total_refund)
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

fn validate_platform_fee_bps(bps: u32) -> Result<(), Error> {
    if bps > PLATFORM_FEE_MAX_BPS {
        return Err(Error::InvalidFeeBps);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events, Ledger as _},
        token, vec, Address, Bytes, Env, IntoVal, Symbol,
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
        env.mock_all_auths_allowing_non_root_auth();

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
        env.mock_all_auths_allowing_non_root_auth();

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
        env.mock_all_auths_allowing_non_root_auth();

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
        env.mock_all_auths_allowing_non_root_auth();

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
        env.mock_all_auths_allowing_non_root_auth();

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
                    (
                        admin,
                        100_u32,
                        treasury,
                        DEFAULT_DISPUTE_WINDOW_SECONDS,
                        VERSION
                    )
                        .into_val(&env)
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
        env.mock_all_auths_allowing_non_root_auth();

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
        env.mock_all_auths_allowing_non_root_auth();

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
        env.mock_all_auths_allowing_non_root_auth();

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
                    (
                        admin,
                        100_u32,
                        treasury.clone(),
                        DEFAULT_DISPUTE_WINDOW_SECONDS,
                        VERSION
                    )
                        .into_val(&env)
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
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let asset = Address::generate(&env);
        let session_id = Bytes::from_array(&env, &[1u8, 2u8, 3u8]);
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
            expires_at: created_at + ESCROW_DURATION_SECONDS,
            payer_approved: false,
            payee_approved: false,
            approved_at: 0,
        };

        client.put_session(&s);

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
        client.update_session_status(&session_id, &SessionStatus::Completed, &new_updated_at);
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
        let sid1 = Bytes::from_array(&env, &[1u8]);
        let sid2 = Bytes::from_array(&env, &[2u8]);

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
            expires_at: 0,
            payer_approved: false,
            payee_approved: false,
            approved_at: 0,
        };

        let s2 = Session {
            session_id: sid2.clone(),
            ..s1.clone()
        };

        client.put_session(&s1);
        client.put_session(&s2);

        let g1 = client.get_session(&sid1).unwrap();
        let g2 = client.get_session(&sid2).unwrap();
        assert_eq!(g1.session_id, sid1);
        assert_eq!(g2.session_id, sid2);
    }

    #[test]
    fn test_session_migration_compatibility_old_version_decodes() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let addr = Address::generate(&env);
        let sid = Bytes::from_array(&env, &[9u8, 9u8]);

        // Simulate an older-version session (version 0) to verify forward-decode safety.
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
            expires_at: 0,
            payer_approved: false,
            payee_approved: false,
            approved_at: 0,
        };

        client.put_session(&old);
        let got = client.get_session(&sid).unwrap();
        assert_eq!(got.version, 0);
    }

    #[test]
    fn test_init_stores_correct_values_and_emits_event() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();
        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let platform_fee_bps = 250_u32;
        let dispute_window = 3600_u64;

        client.init(&admin, &platform_fee_bps, &treasury, &dispute_window);

        assert_eq!(client.get_dispute_window(), dispute_window);
        assert_eq!(client.get_treasury(), treasury);
        assert_eq!(client.get_platform_fee_bps(), platform_fee_bps);

        let events = env.events().all();
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
            expires_at: 0,
            payer_approved: false,
            payee_approved: false,
            approved_at: 0,
        };

        // store and ensure we can read back (decode) older versions
        client.put_session(&old).unwrap();
        let got = client.get_session(&sid).unwrap();
        assert_eq!(got.version, 0);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        let result = client.try_init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);
        assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
    }

    #[test]
    fn test_put_session_happy_path() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let addr = Address::generate(&env);
        let sid = Bytes::from_array(&env, &[42u8, 7u8, 13u8]);

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
            expires_at: 1000 + ESCROW_DURATION_SECONDS,
            payer_approved: false,
            payee_approved: false,
            approved_at: 0,
        };

        // First insertion should succeed
        client.put_session(&session);

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
        let sid = Bytes::from_array(&env, &[99u8, 88u8]);

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
            expires_at: 5000 + ESCROW_DURATION_SECONDS,
            payer_approved: false,
            payee_approved: false,
            approved_at: 0,
        };

        let mut session2 = session1.clone();
        session2.amount = 2_000_000; // Different amount, same ID

        // First insertion should succeed
        client.put_session(&session1);

        // Second insertion with same session_id should fail
        let result2 = client.try_put_session(&session2);
        assert_eq!(result2, Err(Ok(Error::DuplicateSessionId)));
    }

    #[test]
    fn test_put_session_allows_different_ids() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let addr = Address::generate(&env);
        let sid1 = Bytes::from_array(&env, &[1u8, 1u8]);
        let sid2 = Bytes::from_array(&env, &[2u8, 2u8]);
        let sid3 = Bytes::from_array(&env, &[3u8, 3u8]);

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
            expires_at: 0,
            payer_approved: false,
            payee_approved: false,
            approved_at: 0,
        };

        let session2 = Session {
            session_id: sid2.clone(),
            ..session1.clone()
        };
        let session3 = Session {
            session_id: sid3.clone(),
            ..session1.clone()
        };

        // All different session_ids should be accepted
        client.put_session(&session1);
        client.put_session(&session2);
        client.put_session(&session3);

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
        let sid = Bytes::from_array(&env, &[123u8, 45u8, 67u8]);

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
            expires_at: 0,
            payer_approved: false,
            payee_approved: false,
            approved_at: 0,
        };

        // First insertion succeeds
        client.put_session(&session);

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

        // Test with multiple single-byte session IDs (0-255 pattern)
        for i in 0u8..10u8 {
            let sid = Bytes::from_array(&env, &[i]);
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
                expires_at: (i as u64) + ESCROW_DURATION_SECONDS,
                payer_approved: false,
                payee_approved: false,
                approved_at: 0,
            };

            client.put_session(&session);

            let stored = client.get_session(&sid);
            assert!(stored.is_some());
            assert_eq!(stored.unwrap().amount, (i as i128) * 1000);
        }
    }

    #[test]
    fn test_put_session_randomized_ids_multi_byte() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let addr = Address::generate(&env);

        // Test with various multi-byte patterns simulating UUIDs or random IDs
        let id_patterns = [
            Bytes::from_array(&env, &[0u8, 1u8, 2u8, 3u8]),
            Bytes::from_array(&env, &[255u8, 254u8, 253u8]),
            Bytes::from_array(&env, &[0x12u8, 0x34u8, 0x56u8, 0x78u8, 0x9au8]),
            Bytes::from_array(&env, &[0xddu8, 0xeeu8, 0xffu8]),
            Bytes::from_array(&env, &[1u8, 1u8, 1u8, 1u8, 1u8]),
            Bytes::from_array(&env, &[0u8, 0u8, 0u8, 0u8]),
            Bytes::from_array(&env, &[128u8, 64u8, 32u8, 16u8, 8u8, 4u8, 2u8, 1u8]),
            Bytes::from_array(&env, &[7u8, 14u8, 21u8, 28u8, 35u8]),
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
                expires_at: (idx as u64) * 1000 + ESCROW_DURATION_SECONDS,
                payer_approved: false,
                payee_approved: false,
                approved_at: 0,
            };

            // Each unique pattern should be accepted
            client.put_session(&session);

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
                expires_at: 0,
                payer_approved: false,
                payee_approved: false,
                approved_at: 0,
            };

            let result = client.try_put_session(&session);
            assert_eq!(
                result,
                Err(Ok(Error::DuplicateSessionId)),
                "Expected DuplicateSessionId error for existing ID"
            );
        }
    }

    #[test]
    fn test_put_session_randomized_ids_large_ids() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let addr = Address::generate(&env);

        // Simulate large ID patterns (like SHA256 hashes or UUIDs)
        let large_ids = [
            Bytes::from_array(
                &env,
                &[
                    0x4du8, 0x6fu8, 0x9eu8, 0x8bu8, 0xcdu8, 0xf4u8, 0x2bu8, 0xa0u8, 0x45u8, 0xcfu8,
                    0x15u8, 0x11u8, 0x6au8, 0x7bu8, 0xd8u8, 0xe9u8,
                ],
            ),
            Bytes::from_array(
                &env,
                &[
                    0xffu8, 0xeeu8, 0xddu8, 0xccu8, 0xbbu8, 0xaau8, 0x99u8, 0x88u8, 0x77u8, 0x66u8,
                    0x55u8, 0x44u8, 0x33u8, 0x22u8, 0x11u8, 0x00u8,
                ],
            ),
            Bytes::from_array(
                &env,
                &[
                    0x00u8, 0x11u8, 0x22u8, 0x33u8, 0x44u8, 0x55u8, 0x66u8, 0x77u8, 0x88u8, 0x99u8,
                    0xaau8, 0xbbu8, 0xccu8, 0xddu8, 0xeeu8, 0xffu8,
                ],
            ),
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
                expires_at: 1_000_000 + ESCROW_DURATION_SECONDS,
                payer_approved: false,
                payee_approved: false,
                approved_at: 0,
            };

            client.put_session(&session);
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
                expires_at: 0,
                payer_approved: false,
                payee_approved: false,
                approved_at: 0,
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
        let sid_min = Bytes::from_array(&env, &[0u8]);
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
            expires_at: 0,
            payer_approved: false,
            payee_approved: false,
            approved_at: 0,
        };

        client.put_session(&session_min);
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
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract(payer.clone());
        let token_admin = token::StellarAssetClient::new(&env, &token_id);

        token_admin.mint(&payer, &(10_000_000_i128));

        let session_id = Bytes::from_array(&env, &[1u8, 2u8, 3u8]);
        let amount = 1_000_000_i128;
        let fee_bps = 250u32; // 2.5%

        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

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
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract(payer.clone());
        let session_id = Bytes::from_array(&env, &[5u8, 6u8]);

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
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract(payer.clone());
        let session_id = Bytes::from_array(&env, &[7u8, 8u8]);

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
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let payee2 = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract(payer.clone());
        let token_admin = token::StellarAssetClient::new(&env, &token_id);
        token_admin.mint(&payer, &(50_000_000_i128));

        let session_id = Bytes::from_array(&env, &[10u8, 11u8]);
        let amount = 1_000_000_i128;
        let fee_bps = 100u32;

        // First lock should succeed
        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        // Second lock with same session_id should fail
        let result2 =
            client.try_lock_funds(&session_id, &payer, &payee2, &token_id, &amount, &fee_bps);
        assert!(result2.is_err());
        assert_eq!(result2.unwrap_err(), Ok(Error::DuplicateSessionId));
    }

    #[test]
    fn test_lock_funds_sufficient_balance() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract(payer.clone());
        let token_admin = token::StellarAssetClient::new(&env, &token_id);
        let amount = 1_000_000_i128;
        let fee_bps = 250u32; // 2.5%
        let fee = (amount * fee_bps as i128) / 10000; // 25000
        let total = amount + fee;

        token_admin.mint(&payer, &total);

        let session_id = Bytes::from_array(&env, &[12u8, 13u8, 14u8]);
        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);
    }

    #[test]
    fn test_lock_funds_insufficient_balance() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract(payer.clone());
        let token_admin = token::StellarAssetClient::new(&env, &token_id);
        let amount = 1_000_000_i128;
        let fee_bps = 250u32;
        let fee = (amount * fee_bps as i128) / 10000;
        let total_needed = amount + fee;

        // Only mint 90% of needed amount
        token_admin.mint(&payer, &(total_needed * 9 / 10));

        let session_id = Bytes::from_array(&env, &[15u8, 16u8]);
        let result =
            client.try_lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Ok(Error::InsufficientBalance));
    }

    #[test]
    fn test_lock_funds_platform_fee_calculation() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract(payer.clone());
        let token_admin = token::StellarAssetClient::new(&env, &token_id);

        // Test various fee scenarios
        let test_cases: [(i128, u32, i128); 6] = [
            (1_000_000i128, 0u32, 0i128),          // 0% fee
            (1_000_000i128, 100u32, 10_000i128),   // 1% fee = 10,000
            (1_000_000i128, 250u32, 25_000i128),   // 2.5% fee = 25,000
            (1_000_000i128, 500u32, 50_000i128),   // 5% fee = 50,000
            (1_000_000i128, 1000u32, 100_000i128), // 10% fee = 100,000
            (10_000_000i128, 500u32, 500_000i128), // 5% of 10M = 500,000
        ];

        for (idx, &(amount, fee_bps, expected_fee)) in test_cases.iter().enumerate() {
            token_admin.mint(&payer, &(amount + expected_fee + 100_000)); // Add buffer

            let session_id = Bytes::from_array(&env, &[20u8 + idx as u8, 21u8]);
            client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

            // Verify stored session has correct amounts
            let session = client.get_session(&session_id).unwrap();
            assert_eq!(session.amount, amount);
            assert_eq!(session.fee_bps, fee_bps);
        }
    }

    #[test]
    fn test_lock_funds_creates_session_with_correct_timestamp() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract(payer.clone());
        let token_admin = token::StellarAssetClient::new(&env, &token_id);
        token_admin.mint(&payer, &(10_000_000_i128));

        // Set a specific ledger timestamp
        let timestamp = 1_000_000u64;
        env.ledger().with_mut(|l| l.timestamp = timestamp);

        let session_id = Bytes::from_array(&env, &[30u8, 31u8]);
        let amount = 1_000_000i128;

        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &100u32);

        let session = client.get_session(&session_id).unwrap();
        assert_eq!(session.created_at, timestamp);
        assert_eq!(session.updated_at, timestamp);
        assert_eq!(session.status, SessionStatus::Locked);
    }

    #[test]
    fn test_lock_funds_emits_event() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract(payer.clone());
        let token_admin = token::StellarAssetClient::new(&env, &token_id);
        token_admin.mint(&payer, &(10_000_000_i128));

        let session_id = Bytes::from_array(&env, &[40u8, 41u8, 42u8]);
        let amount = 1_000_000i128;
        let fee_bps = 250u32;
        let expected_fee = (amount * fee_bps as i128) / 10000;

        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        // The last event emitted by our contract should be FundsLocked.
        let all = env.events().all();
        let n = all.len();
        assert!(n > 0, "no events emitted");
        assert_eq!(
            all.slice(n - 1..),
            vec![
                &env,
                (
                    contract_id.clone(),
                    (Symbol::new(&env, "FundsLocked"),).into_val(&env),
                    (
                        session_id.clone(),
                        payer.clone(),
                        payee.clone(),
                        amount,
                        expected_fee
                    )
                        .into_val(&env)
                )
            ]
        );
    }

    #[test]
    fn test_lock_funds_multiple_sessions_different_parties() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let token_id = env.register_stellar_asset_contract(Address::generate(&env));
        let token_admin = token::StellarAssetClient::new(&env, &token_id);

        // Create multiple sessions with different parties
        let base_payer = Address::generate(&env);
        token_admin.mint(&base_payer, &(100_000_000_i128));

        for i in 0..5 {
            let payer = if i == 0 {
                base_payer.clone()
            } else {
                Address::generate(&env)
            };
            if i > 0 {
                token_admin.mint(&payer, &(10_000_000_i128));
            }

            let payee = Address::generate(&env);
            let session_id = Bytes::from_array(&env, &[50u8 + i as u8, 51u8]);
            let amount = 1_000_000i128 + (i as i128 * 100_000);

            client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &100u32);

            let session = client.get_session(&session_id).unwrap();
            assert_eq!(session.payer, payer);
            assert_eq!(session.payee, payee);
            assert_eq!(session.amount, amount);
        }
    }

    #[test]
    fn test_lock_funds_max_fee_calculation() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract(payer.clone());
        let token_admin = token::StellarAssetClient::new(&env, &token_id);

        // Test maximum fee (10000 bps = 100%)
        let amount = 1_000_000i128;
        let fee_bps = 10000u32; // 100% fee

        token_admin.mint(&payer, &(amount * 2 + 100_000)); // Need double for 100% fee

        let session_id = Bytes::from_array(&env, &[60u8, 61u8]);
        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        let session = client.get_session(&session_id).unwrap();
        assert_eq!(session.fee_bps, fee_bps);
    }

    // =========================================================================
    // platform fee tests
    // =========================================================================

    #[test]
    fn test_get_platform_fee_bps_returns_value_set_at_init() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);
        assert_eq!(client.get_platform_fee_bps(), 250);
    }

    #[test]
    fn test_set_platform_fee_bps_persists() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        client.set_platform_fee_bps(&500);
        assert_eq!(client.get_platform_fee_bps(), 500);
    }

    #[test]
    fn test_set_platform_fee_bps_at_zero_succeeds() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        assert!(client.try_set_platform_fee_bps(&0).is_ok());
        assert_eq!(client.get_platform_fee_bps(), 0);
    }

    #[test]
    fn test_set_platform_fee_bps_at_max_succeeds() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        assert!(client
            .try_set_platform_fee_bps(&PLATFORM_FEE_MAX_BPS)
            .is_ok());
        assert_eq!(client.get_platform_fee_bps(), PLATFORM_FEE_MAX_BPS);
    }

    #[test]
    fn test_set_platform_fee_bps_above_max_reverts() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        let result = client.try_set_platform_fee_bps(&(PLATFORM_FEE_MAX_BPS + 1));
        assert_eq!(result, Err(Ok(Error::InvalidFeeBps)));
    }

    #[test]
    fn test_init_with_fee_above_max_reverts() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        let result = client.try_init(
            &admin,
            &(PLATFORM_FEE_MAX_BPS + 1),
            &treasury,
            &DEFAULT_DISPUTE_WINDOW_SECONDS,
        );
        assert_eq!(result, Err(Ok(Error::InvalidFeeBps)));
    }

    #[test]
    fn test_set_platform_fee_bps_requires_admin_auth() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);
        client.set_platform_fee_bps(&200);

        let auths = env.auths();
        assert_eq!(auths.len(), 1);
        assert_eq!(auths[0].0, admin);
    }

    #[test]
    fn test_set_platform_fee_bps_emits_event_with_old_and_new() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        let old_fee = 100_u32;
        let new_fee = 300_u32;
        client.set_platform_fee_bps(&new_fee);

        assert_eq!(
            env.events().all(),
            vec![
                &env,
                (
                    contract_id.clone(),
                    (Symbol::new(&env, "Initialized"),).into_val(&env),
                    (
                        admin,
                        old_fee,
                        treasury,
                        DEFAULT_DISPUTE_WINDOW_SECONDS,
                        VERSION
                    )
                        .into_val(&env)
                ),
                (
                    contract_id.clone(),
                    (Symbol::new(&env, "PlatformFeeUpdated"),).into_val(&env),
                    (old_fee, new_fee).into_val(&env)
                )
            ]
        );
    }

    #[test]
    fn test_set_platform_fee_bps_event_reflects_successive_updates() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &0, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // First update: 0 → 500
        client.set_platform_fee_bps(&500);
        assert_eq!(client.get_platform_fee_bps(), 500);

        // Second update: 500 → 1000
        client.set_platform_fee_bps(&PLATFORM_FEE_MAX_BPS);
        assert_eq!(client.get_platform_fee_bps(), PLATFORM_FEE_MAX_BPS);

        // Third update: 1000 → 0
        client.set_platform_fee_bps(&0);
        assert_eq!(client.get_platform_fee_bps(), 0);

        // events[0] = Initialized, [1] = PlatformFeeUpdated(0→500),
        // [2] = PlatformFeeUpdated(500→1000), [3] = PlatformFeeUpdated(1000→0)
        assert_eq!(
            env.events().all(),
            vec![
                &env,
                (
                    contract_id.clone(),
                    (Symbol::new(&env, "Initialized"),).into_val(&env),
                    (
                        admin.clone(),
                        0_u32,
                        treasury.clone(),
                        DEFAULT_DISPUTE_WINDOW_SECONDS,
                        VERSION
                    )
                        .into_val(&env)
                ),
                (
                    contract_id.clone(),
                    (Symbol::new(&env, "PlatformFeeUpdated"),).into_val(&env),
                    (0_u32, 500_u32).into_val(&env)
                ),
                (
                    contract_id.clone(),
                    (Symbol::new(&env, "PlatformFeeUpdated"),).into_val(&env),
                    (500_u32, PLATFORM_FEE_MAX_BPS).into_val(&env)
                ),
                (
                    contract_id.clone(),
                    (Symbol::new(&env, "PlatformFeeUpdated"),).into_val(&env),
                    (PLATFORM_FEE_MAX_BPS, 0_u32).into_val(&env)
                ),
            ]
        );
    }

    // Tests for complete_session functionality
    // =========================================

    #[test]
    fn test_complete_session_happy_path() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Setup addresses and token
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);
        let token_client = token::Client::new(&env, &token_id);

        // Mint tokens to payer
        let amount = 1_000_000_i128;
        let fee_bps = 250u32; // 2.5%
        let fee = (amount * fee_bps as i128) / 10000;
        token_client.mint(&payer, &(amount + fee));

        // Lock funds
        let session_id = vec![&env, 100u8, 101u8];
        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        // Fast forward past dispute window
        let current_time = env.ledger().timestamp();
        env.ledger()
            .set_timestamp(current_time + DEFAULT_DISPUTE_WINDOW_SECONDS + 1);

        // Complete session
        let result = client.complete_session(&session_id, &payer);
        assert!(result.is_ok());

        // Verify session status updated
        let session = client.get_session(&session_id).unwrap();
        assert_eq!(session.status, SessionStatus::Completed);

        // Verify payee received funds
        let payee_balance = token_client.balance(&payee);
        assert_eq!(payee_balance, amount);

        // Verify treasury received fee
        let treasury_balance = token_client.balance(&treasury);
        assert_eq!(treasury_balance, fee);
    }

    #[test]
    fn test_complete_session_nonexistent_session() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let caller = Address::generate(&env);
        let session_id = vec![&env, 200u8, 201u8];

        let result = client.try_complete_session(&session_id, &caller);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Ok(Error::SessionNotFound));
    }

    #[test]
    fn test_complete_session_invalid_status_pending() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let addr = Address::generate(&env);
        let session_id = vec![&env, 202u8, 203u8];

        // Create a session with Pending status
        let session = Session {
            version: 1,
            session_id: session_id.clone(),
            payer: addr.clone(),
            payee: addr.clone(),
            asset: addr.clone(),
            amount: 1_000_000,
            fee_bps: 250,
            status: SessionStatus::Pending,
            created_at: 0,
            updated_at: 0,
            dispute_deadline: 0,
            expires_at: 0,
            payer_approved: false,
            payee_approved: false,
            approved_at: 0,
        };

        client.put_session(&session).unwrap();

        let result = client.try_complete_session(&session_id, &addr);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Ok(Error::InvalidSessionStatus));
    }

    #[test]
    fn test_complete_session_invalid_status_completed() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let addr = Address::generate(&env);
        let session_id = vec![&env, 204u8, 205u8];

        // Create a session with Completed status
        let session = Session {
            version: 1,
            session_id: session_id.clone(),
            payer: addr.clone(),
            payee: addr.clone(),
            asset: addr.clone(),
            amount: 1_000_000,
            fee_bps: 250,
            status: SessionStatus::Completed,
            created_at: 0,
            updated_at: 0,
            dispute_deadline: 0,
            expires_at: 0,
            payer_approved: false,
            payee_approved: false,
            approved_at: 0,
        };

        client.put_session(&session).unwrap();

        let result = client.try_complete_session(&session_id, &addr);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Ok(Error::InvalidSessionStatus));
    }

    #[test]
    fn test_complete_session_dispute_window_not_elapsed() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Setup addresses and token
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);
        let token_client = token::Client::new(&env, &token_id);

        // Mint tokens
        let amount = 1_000_000_i128;
        let fee_bps = 250u32;
        let fee = (amount * fee_bps as i128) / 10000;
        token_client.mint(&payer, &(amount + fee));

        // Lock funds
        let session_id = vec![&env, 206u8, 207u8];
        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        // Try to complete immediately (dispute window not elapsed)
        let result = client.try_complete_session(&session_id, &payer);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Ok(Error::DisputeWindowNotElapsed));
    }

    #[test]
    fn test_complete_session_exactly_at_deadline() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Setup addresses and token
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);
        let token_client = token::Client::new(&env, &token_id);

        // Mint tokens
        let amount = 1_000_000_i128;
        let fee_bps = 250u32;
        let fee = (amount * fee_bps as i128) / 10000;
        token_client.mint(&payer, &(amount + fee));

        // Lock funds
        let session_id = vec![&env, 208u8, 209u8];
        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        // Set time exactly at deadline (should still fail, needs to be after)
        let current_time = env.ledger().timestamp();
        env.ledger()
            .set_timestamp(current_time + DEFAULT_DISPUTE_WINDOW_SECONDS);

        let result = client.try_complete_session(&session_id, &payer);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Ok(Error::DisputeWindowNotElapsed));
    }

    #[test]
    fn test_complete_session_zero_fee() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &0, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Setup addresses and token
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);
        let token_client = token::Client::new(&env, &token_id);

        // Mint tokens (no fee)
        let amount = 1_000_000_i128;
        let fee_bps = 0u32;
        token_client.mint(&payer, &amount);

        // Lock funds
        let session_id = vec![&env, 210u8, 211u8];
        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        // Fast forward past dispute window
        let current_time = env.ledger().timestamp();
        env.ledger()
            .set_timestamp(current_time + DEFAULT_DISPUTE_WINDOW_SECONDS + 1);

        // Complete session
        let result = client.complete_session(&session_id, &payer);
        assert!(result.is_ok());

        // Verify payee received full amount
        let payee_balance = token_client.balance(&payee);
        assert_eq!(payee_balance, amount);

        // Verify treasury received nothing
        let treasury_balance = token_client.balance(&treasury);
        assert_eq!(treasury_balance, 0);
    }

    #[test]
    fn test_complete_session_updates_timestamp() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Setup addresses and token
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);
        let token_client = token::Client::new(&env, &token_id);

        // Mint tokens
        let amount = 1_000_000_i128;
        let fee_bps = 250u32;
        let fee = (amount * fee_bps as i128) / 10000;
        token_client.mint(&payer, &(amount + fee));

        // Lock funds
        let session_id = vec![&env, 212u8, 213u8];
        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        let created_at = client.get_session(&session_id).unwrap().created_at;

        // Fast forward past dispute window
        let current_time = env.ledger().timestamp();
        let completion_time = current_time + DEFAULT_DISPUTE_WINDOW_SECONDS + 100;
        env.ledger().set_timestamp(completion_time);

        // Complete session
        client.complete_session(&session_id, &payer);

        // Verify updated_at changed
        let session = client.get_session(&session_id).unwrap();
        assert_eq!(session.updated_at, completion_time);
        assert!(session.updated_at > created_at);
    }

    #[test]
    fn test_complete_session_emits_event() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Setup addresses and token
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);
        let token_client = token::Client::new(&env, &token_id);

        // Mint tokens
        let amount = 1_000_000_i128;
        let fee_bps = 250u32;
        let fee = (amount * fee_bps as i128) / 10000;
        token_client.mint(&payer, &(amount + fee));

        // Lock funds
        let session_id = vec![&env, 214u8, 215u8];
        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        // Fast forward past dispute window
        let current_time = env.ledger().timestamp();
        env.ledger()
            .set_timestamp(current_time + DEFAULT_DISPUTE_WINDOW_SECONDS + 1);

        // Complete session
        client.complete_session(&session_id, &payer);

        // Verify SessionCompleted event was emitted
        let events = env.events().all();
        let mut found_event = false;
        for event in events {
            if let Some(topics) = event.2.get(0) {
                if let Ok(symbol) = Symbol::try_from(topics) {
                    if symbol.to_string(&env) == Some("SessionCompleted".to_string()) {
                        found_event = true;
                        break;
                    }
                }
            }
        }
        assert!(found_event, "SessionCompleted event not found");
    }

    #[test]
    fn test_complete_session_multiple_sessions() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Setup token
        let payer = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);
        let token_client = token::Client::new(&env, &token_id);

        // Create and complete multiple sessions
        for i in 0..3 {
            let payee = Address::generate(&env);
            let amount = 1_000_000_i128 + (i as i128 * 100_000);
            let fee_bps = 250u32;
            let fee = (amount * fee_bps as i128) / 10000;

            token_client.mint(&payer, &(amount + fee));

            let session_id = vec![&env, 220u8 + (i as u8), 221u8];
            client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

            // Fast forward
            let current_time = env.ledger().timestamp();
            env.ledger()
                .set_timestamp(current_time + DEFAULT_DISPUTE_WINDOW_SECONDS + 1);

            // Complete
            let result = client.complete_session(&session_id, &payer);
            assert!(result.is_ok(), "Failed to complete session {}", i);

            // Verify
            let session = client.get_session(&session_id).unwrap();
            assert_eq!(session.status, SessionStatus::Completed);
            assert_eq!(token_client.balance(&payee), amount);
        }
    }

    #[test]
    fn test_complete_session_requires_auth() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Setup addresses and token
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let caller = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);
        let token_client = token::Client::new(&env, &token_id);

        // Mint tokens
        let amount = 1_000_000_i128;
        let fee_bps = 250u32;
        let fee = (amount * fee_bps as i128) / 10000;
        token_client.mint(&payer, &(amount + fee));

        // Lock funds
        let session_id = vec![&env, 230u8, 231u8];
        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        // Fast forward past dispute window
        let current_time = env.ledger().timestamp();
        env.ledger()
            .set_timestamp(current_time + DEFAULT_DISPUTE_WINDOW_SECONDS + 1);

        // Complete session with different caller
        client.complete_session(&session_id, &caller);

        // Verify caller was authenticated
        let auths = env.auths();
        let mut found_caller_auth = false;
        for auth in auths {
            if auth.0 == caller {
                found_caller_auth = true;
                break;
            }
        }
        assert!(found_caller_auth, "Caller authentication not found");
    }

    // Tests for approve_session functionality
    // ========================================

    #[test]
    fn test_approve_session_payer_approval() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Setup addresses and token
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);
        let token_client = token::Client::new(&env, &token_id);

        // Mint and lock funds
        let amount = 1_000_000_i128;
        let fee_bps = 250u32;
        let fee = (amount * fee_bps as i128) / 10000;
        token_client.mint(&payer, &(amount + fee));

        let session_id = vec![&env, 240u8, 241u8];
        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        // Payer approves
        let result = client.approve_session(&session_id, &payer);
        assert!(result.is_ok());

        // Verify approval recorded
        let session = client.get_session(&session_id).unwrap();
        assert!(session.payer_approved);
        assert!(!session.payee_approved);
        assert_eq!(session.approved_at, 0); // Not both approved yet
    }

    #[test]
    fn test_approve_session_payee_approval() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Setup addresses and token
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);
        let token_client = token::Client::new(&env, &token_id);

        // Mint and lock funds
        let amount = 1_000_000_i128;
        let fee_bps = 250u32;
        let fee = (amount * fee_bps as i128) / 10000;
        token_client.mint(&payer, &(amount + fee));

        let session_id = vec![&env, 242u8, 243u8];
        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        // Payee approves
        let result = client.approve_session(&session_id, &payee);
        assert!(result.is_ok());

        // Verify approval recorded
        let session = client.get_session(&session_id).unwrap();
        assert!(!session.payer_approved);
        assert!(session.payee_approved);
        assert_eq!(session.approved_at, 0); // Not both approved yet
    }

    #[test]
    fn test_approve_session_both_parties() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Setup addresses and token
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);
        let token_client = token::Client::new(&env, &token_id);

        // Mint and lock funds
        let amount = 1_000_000_i128;
        let fee_bps = 250u32;
        let fee = (amount * fee_bps as i128) / 10000;
        token_client.mint(&payer, &(amount + fee));

        let session_id = vec![&env, 244u8, 245u8];
        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        // Both parties approve
        client.approve_session(&session_id, &payer);
        client.approve_session(&session_id, &payee);

        // Verify both approvals recorded and approved_at set
        let session = client.get_session(&session_id).unwrap();
        assert!(session.payer_approved);
        assert!(session.payee_approved);
        assert!(session.approved_at > 0);
    }

    #[test]
    fn test_approve_session_duplicate_approval() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Setup addresses and token
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);
        let token_client = token::Client::new(&env, &token_id);

        // Mint and lock funds
        let amount = 1_000_000_i128;
        let fee_bps = 250u32;
        let fee = (amount * fee_bps as i128) / 10000;
        token_client.mint(&payer, &(amount + fee));

        let session_id = vec![&env, 246u8, 247u8];
        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        // First approval succeeds
        client.approve_session(&session_id, &payer);

        // Second approval by same party fails
        let result = client.try_approve_session(&session_id, &payer);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Ok(Error::AlreadyApproved));
    }

    #[test]
    fn test_approve_session_unauthorized_party() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Setup addresses and token
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let unauthorized = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);
        let token_client = token::Client::new(&env, &token_id);

        // Mint and lock funds
        let amount = 1_000_000_i128;
        let fee_bps = 250u32;
        let fee = (amount * fee_bps as i128) / 10000;
        token_client.mint(&payer, &(amount + fee));

        let session_id = vec![&env, 248u8, 249u8];
        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        // Unauthorized party tries to approve
        let result = client.try_approve_session(&session_id, &unauthorized);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Ok(Error::NotAuthorizedParty));
    }

    #[test]
    fn test_approve_session_nonexistent_session() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let approver = Address::generate(&env);
        let session_id = vec![&env, 250u8, 251u8];

        let result = client.try_approve_session(&session_id, &approver);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Ok(Error::SessionNotFound));
    }

    #[test]
    fn test_approve_session_invalid_status() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let addr = Address::generate(&env);
        let session_id = vec![&env, 252u8, 253u8];

        // Create a completed session
        let session = Session {
            version: 1,
            session_id: session_id.clone(),
            payer: addr.clone(),
            payee: addr.clone(),
            asset: addr.clone(),
            amount: 1_000_000,
            fee_bps: 250,
            status: SessionStatus::Completed,
            created_at: 0,
            updated_at: 0,
            dispute_deadline: 0,
            payer_approved: false,
            payee_approved: false,
            approved_at: 0,
        };

        client.put_session(&session).unwrap();

        let result = client.try_approve_session(&session_id, &addr);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Ok(Error::InvalidSessionStatus));
    }

    #[test]
    fn test_approve_session_emits_event() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Setup addresses and token
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);
        let token_client = token::Client::new(&env, &token_id);

        // Mint and lock funds
        let amount = 1_000_000_i128;
        let fee_bps = 250u32;
        let fee = (amount * fee_bps as i128) / 10000;
        token_client.mint(&payer, &(amount + fee));

        let session_id = vec![&env, 254u8, 255u8];
        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        // Approve
        client.approve_session(&session_id, &payer);

        // Verify SessionApproved event was emitted
        let events = env.events().all();
        let mut found_event = false;
        for event in events {
            if let Some(topics) = event.2.get(0) {
                if let Ok(symbol) = Symbol::try_from(topics) {
                    if symbol.to_string(&env) == Some("SessionApproved".to_string()) {
                        found_event = true;
                        break;
                    }
                }
            }
        }
        assert!(found_event, "SessionApproved event not found");
    }

    #[test]
    fn test_complete_session_with_both_approvals_early() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Setup addresses and token
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);
        let token_client = token::Client::new(&env, &token_id);

        // Mint and lock funds
        let amount = 1_000_000_i128;
        let fee_bps = 250u32;
        let fee = (amount * fee_bps as i128) / 10000;
        token_client.mint(&payer, &(amount + fee));

        let session_id = vec![&env, 1u8, 2u8, 3u8];
        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        // Both parties approve
        client.approve_session(&session_id, &payer);
        client.approve_session(&session_id, &payee);

        // Complete immediately (before dispute window) - should succeed
        let result = client.complete_session(&session_id, &payer);
        assert!(result.is_ok());

        // Verify completion
        let session = client.get_session(&session_id).unwrap();
        assert_eq!(session.status, SessionStatus::Completed);

        // Verify funds transferred
        assert_eq!(token_client.balance(&payee), amount);
        assert_eq!(token_client.balance(&treasury), fee);
    }

    #[test]
    fn test_complete_session_without_approvals_before_window() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Setup addresses and token
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);
        let token_client = token::Client::new(&env, &token_id);

        // Mint and lock funds
        let amount = 1_000_000_i128;
        let fee_bps = 250u32;
        let fee = (amount * fee_bps as i128) / 10000;
        token_client.mint(&payer, &(amount + fee));

        let session_id = vec![&env, 4u8, 5u8, 6u8];
        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        // Try to complete immediately without approvals - should fail
        let result = client.try_complete_session(&session_id, &payer);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Ok(Error::DisputeWindowNotElapsed));
    }

    #[test]
    fn test_complete_session_with_one_approval_before_window() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Setup addresses and token
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);
        let token_client = token::Client::new(&env, &token_id);

        // Mint and lock funds
        let amount = 1_000_000_i128;
        let fee_bps = 250u32;
        let fee = (amount * fee_bps as i128) / 10000;
        token_client.mint(&payer, &(amount + fee));

        let session_id = vec![&env, 7u8, 8u8, 9u8];
        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        // Only payer approves
        client.approve_session(&session_id, &payer);

        // Try to complete with only one approval - should fail
        let result = client.try_complete_session(&session_id, &payer);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Ok(Error::DisputeWindowNotElapsed));
    }

    // Tests for sweep_timeouts (timeout-based refunds)
    // =================================================

    #[test]
    fn test_sweep_timeouts_single_expired_session() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Setup addresses and token
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);
        let token_client = token::Client::new(&env, &token_id);

        // Mint tokens for payer
        let amount = 1_000_000_i128;
        let fee_bps = 250u32;
        let fee = (amount * fee_bps as i128) / 10000;
        let total = amount + fee;
        token_client.mint(&payer, &total);

        // Lock funds - this sets expires_at to created_at + 7 days
        let session_id = vec![&env, 100u8, 101u8];
        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        // Verify session was created with Locked status
        let session = client.get_session(&session_id).unwrap();
        assert_eq!(session.status, SessionStatus::Locked);
        assert!(session.expires_at > 0);

        // Advance ledger time past the expiry
        // expires_at = created_at + ESCROW_DURATION_SECONDS
        let expiry_time = session.expires_at;
        env.ledger().set_timestamp(expiry_time + 1);

        // Sweep timeouts
        let processed = client.sweep_timeouts(&10);
        assert_eq!(processed, 1);

        // Verify session is now Cancelled
        let updated_session = client.get_session(&session_id).unwrap();
        assert_eq!(updated_session.status, SessionStatus::Cancelled);

        // Verify funds were refunded to payer
        assert_eq!(token_client.balance(&payer), total);
        assert_eq!(token_client.balance(&payee), 0);
        assert_eq!(token_client.balance(&contract_id), 0);
    }

    #[test]
    fn test_sweep_timeouts_non_expired_session_not_refunded() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Setup addresses and token
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);
        let token_client = token::Client::new(&env, &token_id);

        // Mint tokens for payer
        let amount = 1_000_000_i128;
        let fee_bps = 250u32;
        let fee = (amount * fee_bps as i128) / 10000;
        let total = amount + fee;
        token_client.mint(&payer, &total);

        // Lock funds
        let session_id = vec![&env, 102u8, 103u8];
        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        // Get the expiry time
        let session = client.get_session(&session_id).unwrap();
        let expires_at = session.expires_at;

        // Set time to just before expiry
        env.ledger().set_timestamp(expires_at - 1);

        // Try to sweep - should not refund
        let processed = client.sweep_timeouts(&10);
        assert_eq!(processed, 0);

        // Verify session is still Locked
        let updated_session = client.get_session(&session_id).unwrap();
        assert_eq!(updated_session.status, SessionStatus::Locked);

        // Verify funds are still locked
        assert_eq!(token_client.balance(&payer), 0);
        assert_eq!(token_client.balance(&contract_id), total);
    }

    #[test]
    fn test_sweep_timeouts_multiple_expired_sessions() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Setup tokens
        let payer1 = Address::generate(&env);
        let payer2 = Address::generate(&env);
        let payee1 = Address::generate(&env);
        let payee2 = Address::generate(&env);

        let token_contract1 = env.register_stellar_asset_contract(payer1.clone());
        let token_id1 = Address::from_contract_id(&env, &token_contract1);
        let token_client1 = token::Client::new(&env, &token_id1);

        let token_contract2 = env.register_stellar_asset_contract(payer2.clone());
        let token_id2 = Address::from_contract_id(&env, &token_contract2);
        let token_client2 = token::Client::new(&env, &token_id2);

        // Mint tokens
        let amount = 500_000_i128;
        let fee_bps = 250u32;
        let fee = (amount * fee_bps as i128) / 10000;
        let total = amount + fee;

        token_client1.mint(&payer1, &total);
        token_client2.mint(&payer2, &total);

        // Lock funds for multiple sessions
        let session_id1 = vec![&env, 104u8, 105u8];
        let session_id2 = vec![&env, 106u8, 107u8];

        client.lock_funds(
            &session_id1,
            &payer1,
            &payee1,
            &token_id1,
            &amount,
            &fee_bps,
        );
        client.lock_funds(
            &session_id2,
            &payer2,
            &payee2,
            &token_id2,
            &amount,
            &fee_bps,
        );

        // Get expiry times and advance past them
        let session1 = client.get_session(&session_id1).unwrap();
        let session2 = client.get_session(&session_id2).unwrap();
        let max_expires = session1.expires_at.max(session2.expires_at);
        env.ledger().set_timestamp(max_expires + 1);

        // Sweep all
        let processed = client.sweep_timeouts(&10);
        assert_eq!(processed, 2);

        // Verify both sessions are cancelled
        let s1 = client.get_session(&session_id1).unwrap();
        let s2 = client.get_session(&session_id2).unwrap();
        assert_eq!(s1.status, SessionStatus::Cancelled);
        assert_eq!(s2.status, SessionStatus::Cancelled);

        // Verify funds refunded
        assert_eq!(token_client1.balance(&payer1), total);
        assert_eq!(token_client2.balance(&payer2), total);
    }

    #[test]
    fn test_sweep_timeouts_batch_limit() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Create multiple expired sessions
        let mut total = 0_i128;
        let amount = 100_000_i128;
        let fee_bps = 250u32;
        let fee = (amount * fee_bps as i128) / 10000;
        let session_total = amount + fee;

        let mut session_ids: Vec<Vec<u8>> = Vec::new(&env);

        // Create 5 sessions
        for i in 0..5u32 {
            let payer = Address::generate(&env);
            let payee = Address::generate(&env);
            let token_contract = env.register_stellar_asset_contract(payer.clone());
            let token_id = Address::from_contract_id(&env, &token_contract);
            let token_client = token::Client::new(&env, &token_id);

            token_client.mint(&payer, &session_total);

            let session_id = vec![&env, (110 + i) as u8, (111 + i) as u8];
            client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);
            session_ids.push_back(session_id);
            total += session_total;
        }

        // Get expiry and advance past it
        let first_session = client.get_session(session_ids.get(0).unwrap()).unwrap();
        env.ledger().set_timestamp(first_session.expires_at + 1);

        // Sweep with batch size of 2
        let processed1 = client.sweep_timeouts(&2);
        assert_eq!(processed1, 2);

        // Sweep remaining
        let processed2 = client.sweep_timeouts(&10);
        assert_eq!(processed2, 3);
    }

    #[test]
    fn test_sweep_timeouts_idempotency() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Setup addresses and token
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);
        let token_client = token::Client::new(&env, &token_id);

        // Mint tokens for payer
        let amount = 1_000_000_i128;
        let fee_bps = 250u32;
        let fee = (amount * fee_bps as i128) / 10000;
        let total = amount + fee;
        token_client.mint(&payer, &total);

        // Lock funds
        let session_id = vec![&env, 120u8, 121u8];
        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        // Advance past expiry
        let session = client.get_session(&session_id).unwrap();
        env.ledger().set_timestamp(session.expires_at + 1);

        // First sweep
        let processed1 = client.sweep_timeouts(&10);
        assert_eq!(processed1, 1);

        // Second sweep - should process 0 (idempotent)
        let processed2 = client.sweep_timeouts(&10);
        assert_eq!(processed2, 0);

        // Verify funds still with payer (not refunded again)
        assert_eq!(token_client.balance(&payer), total);
    }

    #[test]
    fn test_sweep_timeouts_completed_session_not_refunded() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Setup addresses and token
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);
        let token_client = token::Client::new(&env, &token_id);

        // Mint tokens for payer
        let amount = 1_000_000_i128;
        let fee_bps = 250u32;
        let fee = (amount * fee_bps as i128) / 10000;
        let total = amount + fee;
        token_client.mint(&payer, &(total * 2)); // Need extra for the second lock

        // Lock funds
        let session_id = vec![&env, 122u8, 123u8];
        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        // Complete the session normally
        client.approve_session(&session_id, &payer);
        client.approve_session(&session_id, &payee);
        client.complete_session(&session_id, &payer);

        // Advance past what would have been expiry
        let session = client.get_session(&session_id).unwrap();
        let original_expires = session.expires_at;
        env.ledger().set_timestamp(original_expires + 100);

        // Try to sweep - should not affect completed session
        let processed = client.sweep_timeouts(&10);
        assert_eq!(processed, 0);

        // Verify session is still Completed
        let updated_session = client.get_session(&session_id).unwrap();
        assert_eq!(updated_session.status, SessionStatus::Completed);
    }

    #[test]
    fn test_sweep_timeouts_emits_event() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Setup addresses and token
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract(payer.clone());
        let token_id = Address::from_contract_id(&env, &token_contract);

        // Mint tokens
        let amount = 1_000_000_i128;
        let fee_bps = 250u32;
        let fee = (amount * fee_bps as i128) / 10000;
        let total = amount + fee;
        let token_client = token::Client::new(&env, &token_id);
        token_client.mint(&payer, &total);

        // Lock funds
        let session_id = vec![&env, 124u8, 125u8];
        client.lock_funds(&session_id, &payer, &payee, &token_id, &amount, &fee_bps);

        // Advance past expiry
        let session = client.get_session(&session_id).unwrap();
        env.ledger().set_timestamp(session.expires_at + 1);

        // Sweep
        client.sweep_timeouts(&10);

        // Verify TimeoutRefunded event was emitted
        let events = env.events().all();
        let mut found_event = false;
        for event in events {
            if let Some(topics) = event.2.get(0) {
                if let Ok(symbol) = Symbol::try_from(topics) {
                    if symbol.to_string(&env) == Some("TimeoutRefunded".to_string()) {
                        found_event = true;
                        break;
                    }
                }
            }
        }
        assert!(found_event, "TimeoutRefunded event not found");
    }

    // ============================================================================
    // Upgradeability Tests
    // ============================================================================

    #[test]
    fn test_propose_upgrade_by_admin() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Propose upgrade with 1 hour timelock
        let wasm_hash = Bytes::from_array(&env, &[1u8; 32]);
        let timelock: u64 = 3600; // 1 hour
        client.propose_upgrade(&wasm_hash, &timelock);

        // Verify pending upgrade exists
        let pending = client.get_pending_upgrade().unwrap();
        assert_eq!(pending.new_wasm_hash, wasm_hash);
        assert!(pending.deadline > pending.proposed_at);
        assert_eq!(pending.deadline - pending.proposed_at, timelock);

        // Verify UpgradeProposed event was emitted
        let events = env.events().all();
        let mut found_event = false;
        for event in events {
            if let Some(topics) = event.2.get(0) {
                if let Ok(symbol) = Symbol::try_from(topics) {
                    if symbol.to_string(&env) == Some("UpgradeProposed".to_string()) {
                        found_event = true;
                        break;
                    }
                }
            }
        }
        assert!(found_event, "UpgradeProposed event not found");
    }

    #[test]
    fn test_propose_upgrade_default_timelock() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Propose upgrade with 0 timelock (should use default)
        let wasm_hash = Bytes::from_array(&env, &[1u8; 32]);
        client.propose_upgrade(&wasm_hash, &0);

        // Verify pending upgrade uses default timelock
        let pending = client.get_pending_upgrade().unwrap();
        assert_eq!(
            pending.deadline - pending.proposed_at,
            DEFAULT_UPGRADE_TIMELOCK_SECONDS
        );
    }

    #[test]
    #[should_panic(expected = "Unauthorized")]
    fn test_propose_upgrade_by_non_admin_blocked() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Try to propose upgrade as non-admin (should panic)
        let non_admin = Address::generate(&env);
        env.set_auths(&[soroban_sdk::testutils::AuthorizedInvocation {
            function: soroban_sdk::testutils::AuthorizedFunction::Contract((
                contract_id.clone(),
                Symbol::new(&env, "propose_upgrade"),
                (Bytes::from_array(&env, &[1u8; 32]), 3600u64).into_val(&env),
            )),
            sub_invocations: Vec::new(&env),
        }]);

        // This should panic with Unauthorized
        client.propose_upgrade(&Bytes::from_array(&env, &[1u8; 32]), &3600);
    }

    #[test]
    fn test_apply_upgrade_success() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Get initial version
        let initial_version = client.get_version();
        assert_eq!(initial_version, VERSION);

        // Propose upgrade with 1 hour timelock
        let wasm_hash = Bytes::from_array(&env, &[1u8; 32]);
        let timelock: u64 = 3600;
        client.propose_upgrade(&wasm_hash, &timelock);

        // Advance time past the deadline
        env.ledger()
            .set_timestamp(env.ledger().timestamp() + timelock + 1);

        // Apply upgrade
        client.apply_upgrade();

        // Verify version incremented
        let new_version = client.get_version();
        assert_eq!(new_version, initial_version + 1);

        // Verify pending upgrade cleared
        assert!(client.get_pending_upgrade().is_none());

        // Verify Upgraded event was emitted
        let events = env.events().all();
        let mut found_event = false;
        for event in events {
            if let Some(topics) = event.2.get(0) {
                if let Ok(symbol) = Symbol::try_from(topics) {
                    if symbol.to_string(&env) == Some("Upgraded".to_string()) {
                        found_event = true;
                        break;
                    }
                }
            }
        }
        assert!(found_event, "Upgraded event not found");
    }

    #[test]
    #[should_panic(expected = "UpgradeNotReady")]
    fn test_apply_upgrade_before_deadline_blocked() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Propose upgrade with 1 hour timelock
        let wasm_hash = Bytes::from_array(&env, &[1u8; 32]);
        let timelock: u64 = 3600;
        client.propose_upgrade(&wasm_hash, &timelock);

        // Try to apply upgrade before deadline (should panic)
        client.apply_upgrade();
    }

    #[test]
    #[should_panic(expected = "UpgradeNotProposed")]
    fn test_apply_upgrade_without_proposal_blocked() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Try to apply upgrade without proposing (should panic)
        client.apply_upgrade();
    }

    #[test]
    #[should_panic(expected = "Unauthorized")]
    fn test_apply_upgrade_by_non_admin_blocked() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Propose upgrade as admin
        let wasm_hash = Bytes::from_array(&env, &[1u8; 32]);
        let timelock: u64 = 3600;
        client.propose_upgrade(&wasm_hash, &timelock);

        // Advance time past the deadline
        env.ledger()
            .set_timestamp(env.ledger().timestamp() + timelock + 1);

        // Try to apply upgrade as non-admin (should panic)
        let non_admin = Address::generate(&env);
        env.set_auths(&[soroban_sdk::testutils::AuthorizedInvocation {
            function: soroban_sdk::testutils::AuthorizedFunction::Contract((
                contract_id.clone(),
                Symbol::new(&env, "apply_upgrade"),
                ().into_val(&env),
            )),
            sub_invocations: Vec::new(&env),
        }]);

        client.apply_upgrade();
    }

    #[test]
    fn test_cancel_upgrade_by_admin() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Propose upgrade
        let wasm_hash = Bytes::from_array(&env, &[1u8; 32]);
        client.propose_upgrade(&wasm_hash, &3600);

        // Verify pending upgrade exists
        assert!(client.get_pending_upgrade().is_some());

        // Cancel upgrade
        client.cancel_upgrade();

        // Verify pending upgrade cleared
        assert!(client.get_pending_upgrade().is_none());

        // Verify UpgradeCancelled event was emitted
        let events = env.events().all();
        let mut found_event = false;
        for event in events {
            if let Some(topics) = event.2.get(0) {
                if let Ok(symbol) = Symbol::try_from(topics) {
                    if symbol.to_string(&env) == Some("UpgradeCancelled".to_string()) {
                        found_event = true;
                        break;
                    }
                }
            }
        }
        assert!(found_event, "UpgradeCancelled event not found");
    }

    #[test]
    #[should_panic(expected = "Unauthorized")]
    fn test_cancel_upgrade_by_non_admin_blocked() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Propose upgrade
        let wasm_hash = Bytes::from_array(&env, &[1u8; 32]);
        client.propose_upgrade(&wasm_hash, &3600);

        // Try to cancel upgrade as non-admin (should panic)
        env.set_auths(&[soroban_sdk::testutils::AuthorizedInvocation {
            function: soroban_sdk::testutils::AuthorizedFunction::Contract((
                contract_id.clone(),
                Symbol::new(&env, "cancel_upgrade"),
                ().into_val(&env),
            )),
            sub_invocations: Vec::new(&env),
        }]);

        client.cancel_upgrade();
    }

    #[test]
    fn test_version_increments_on_multiple_upgrades() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        let initial_version = client.get_version();

        // Perform multiple upgrades
        for i in 0..3 {
            let wasm_hash = Bytes::from_array(&env, &[(i + 1) as u8; 32]);
            let timelock: u64 = 60; // Short timelock for testing

            client.propose_upgrade(&wasm_hash, &timelock);

            // Advance time past deadline
            let current_time = env.ledger().timestamp();
            env.ledger().set_timestamp(current_time + timelock + 1);

            client.apply_upgrade();
        }

        // Verify version incremented 3 times
        assert_eq!(client.get_version(), initial_version + 3);
    }

    #[test]
    fn test_get_version_returns_default_when_not_set() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Version should be set during init
        assert_eq!(client.get_version(), VERSION);
    }

    #[test]
    #[should_panic(expected = "InvalidTimelock")]
    fn test_propose_upgrade_with_invalid_timelock_blocked() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Try to propose upgrade with timelock below minimum (should panic)
        let wasm_hash = Bytes::from_array(&env, &[1u8; 32]);
        client.propose_upgrade(&wasm_hash, &30); // Less than MIN_UPGRADE_TIMELOCK_SECONDS (60)
    }

    #[test]
    fn test_propose_upgrade_overwrites_existing() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        // Initialize contract
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.init(&admin, &250, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Propose first upgrade
        let wasm_hash1 = Bytes::from_array(&env, &[1u8; 32]);
        client.propose_upgrade(&wasm_hash1, &3600);

        let pending1 = client.get_pending_upgrade().unwrap();

        // Propose second upgrade (should overwrite)
        let wasm_hash2 = Bytes::from_array(&env, &[2u8; 32]);
        env.ledger().set_timestamp(env.ledger().timestamp() + 100);
        client.propose_upgrade(&wasm_hash2, &7200);

        let pending2 = client.get_pending_upgrade().unwrap();

        // Verify second upgrade replaced first
        assert_eq!(pending2.new_wasm_hash, wasm_hash2);
        assert_ne!(pending2.proposed_at, pending1.proposed_at);
        assert_eq!(pending2.deadline - pending2.proposed_at, 7200);
    }
}
