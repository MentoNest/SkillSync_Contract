#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, token, Address, Bytes,
    Env, Map, Symbol, Vec,
};

pub const DISPUTE_WINDOW_MIN_SECONDS: u64 = 60;
pub const DISPUTE_WINDOW_MAX_SECONDS: u64 = 30 * 24 * 60 * 60;
pub const DEFAULT_DISPUTE_WINDOW_SECONDS: u64 = 24 * 60 * 60;
pub const PLATFORM_FEE_MAX_BPS: u32 = 1000; // 10%
pub const MAX_FEE_BPS: u32 = 10_000; // 100% - absolute maximum
pub const ESCROW_DURATION_SECONDS: u64 = 7 * 24 * 60 * 60; // Default 7 days
pub const SECONDS_PER_DAY: u64 = 24 * 60 * 60;
pub const MIN_UPGRADE_TIMELOCK_SECONDS: u64 = 60; // Minimum 1 minute timelock
pub const DEFAULT_UPGRADE_TIMELOCK_SECONDS: u64 = 24 * 60 * 60; // Default 1 day timelock

// Input validation limits
pub const MAX_SESSION_ID_LEN: u32 = 64;      // Max session ID length
pub const MAX_NOTE_LEN: u32 = 256;           // Max resolution note length
pub const MAX_AMOUNT: i128 = 1_000_000_000_000_000; // 100 trillion units max

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
    // Fee configuration
    FeeOnRefunds,
    // Reputation system
    MentorReputation(Address),
    // Reentrancy guard
    ReentrancyLock,
    // Nonce for replay protection
    Nonce(Address),
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SessionStatus {
    Pending = 0,
    Completed = 1,
    Disputed = 2,
    Cancelled = 3,
    Locked = 4,
    Resolved = 5,
    Refunded = 6,
}

#[contracttype]
#[derive(Clone, Debug, Default)]
pub struct MentorReputation {
    pub total_sessions: u32,
    pub total_rating_sum: u32,
    pub rating_count: u32,
    pub reliability_score: u32,
    pub updated_at: u64,
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
    pub dispute_opened_at: u64,
    // Resolution fields for dispute resolution
    pub resolved_at: u64,
    pub resolver: Option<Address>,
    pub resolution_note: Option<Bytes>,
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
    InvalidResolutionAmount = 23, // Resolution amounts don't sum to available amount
    SessionNotDisputed = 24,     // Session is not in Disputed status
    ResolutionFeeError = 25,     // Error calculating resolution fees
    FeeCalculationOverflow = 26, // Fee calculation overflow/underflow
    NonceAlreadyUsed = 27,       // Nonce already used for replay protection
    InvalidRating = 28,          // Rating value is invalid (must be 1-5)
    ReputationOverflow = 29,     // Reputation calculation overflow
    InvalidDisputeState = 30,    // Session is not in a valid state for dispute
    InvalidAddress = 31,         // Invalid or empty address
    InvalidSessionId = 32,       // Session ID empty or too long
    InvalidNote = 33,            // Note too long
    AmountTooLarge = 34,         // Amount exceeds maximum allowed
    Reentrancy = 35,             // Reentrant call detected
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

    pub fn put_session(env: Env, session: Session) -> Result<(), Error> {
        let key = DataKey::Session(session.session_id.clone());
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

    pub fn lock_funds(
        env: Env,
        session_id: Bytes,
        payer: Address,
        payee: Address,
        asset: Address,
        amount: i128,
        fee_bps: u32,
    ) -> Result<(), Error> {
        acquire_lock(&env)?;

        validate_session_id(&session_id)?;
        validate_amount(amount)?;
        validate_different_addresses(&payer, &payee)?;

        let now = env.ledger().timestamp();
        let dispute_window = Self::get_dispute_window(env.clone());
        let dispute_deadline = now + dispute_window;
        let expires_at = now + ESCROW_DURATION_SECONDS;

        let fee = amount
            .checked_mul(fee_bps as i128)
            .ok_or(Error::TransferError)?
            .checked_div(10000)
            .ok_or(Error::TransferError)?;

        let total_amount = amount.checked_add(fee).ok_or(Error::TransferError)?;
        let token_client = token::Client::new(&env, &asset);

        if token_client.balance(&payer) < total_amount {
            release_lock(&env);
            return Err(Error::InsufficientBalance);
        }

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
            dispute_opened_at: 0,
            resolved_at: 0,
            resolver: None,
            resolution_note: None,
        };

        Self::put_session(env.clone(), session)?;
        Self::add_to_expiry_index(env.clone(), session_id.clone(), expires_at)?;

        let contract_id = env.current_contract_address();
        token_client.transfer(&payer, &contract_id, &total_amount);

        env.events().publish(
            (Symbol::new(&env, "FundsLocked"),),
            (session_id, payer, payee, amount, fee),
        );

        release_lock(&env);
        Ok(())
    }

    pub fn complete_session(env: Env, session_id: Bytes, caller: Address, nonce: u64) -> Result<(), Error> {
        use_nonce(&env, &caller, nonce)?;
        caller.require_auth();

        let mut session = Self::get_session(env.clone(), session_id.clone()).ok_or(Error::SessionNotFound)?;

        if session.status != SessionStatus::Locked {
            return Err(Error::InvalidSessionStatus);
        }

        let now = env.ledger().timestamp();
        
        // In the new implementation, complete_session just marks it as Completed.
        // Finalization (releasing funds) should happen after the dispute window.
        session.status = SessionStatus::Completed;
        session.updated_at = now;

        let key = DataKey::Session(session_id.clone());
        env.storage().persistent().set(&key, &session);

        env.events().publish(
            (Symbol::new(&env, "SessionCompleted"),),
            (session_id, session.payee.clone(), session.amount),
        );

        Ok(())
    }

    pub fn auto_refund(env: Env, session_id: Bytes) -> Result<(), Error> {
        let mut session = Self::get_session(env.clone(), session_id.clone()).ok_or(Error::SessionNotFound)?;

        if session.status != SessionStatus::Completed {
            return Err(Error::InvalidSessionStatus);
        }

        let now = env.ledger().timestamp();
        let dispute_window = Self::get_dispute_window(env.clone());

        if now <= session.updated_at + dispute_window {
            return Err(Error::DisputeWindowNotElapsed);
        }

        // Refund full amount to buyer (payer) - No fee deducted
        let token_client = token::Client::new(&env, &session.asset);
        let contract_id = env.current_contract_address();
        
        // The total amount locked was amount + fee
        let fee = session.amount
            .checked_mul(session.fee_bps as i128)
            .ok_or(Error::FeeCalculationOverflow)?
            .checked_div(10000)
            .ok_or(Error::FeeCalculationOverflow)?;
        
        let total_locked = session.amount.checked_add(fee).ok_or(Error::FeeCalculationOverflow)?;

        token_client.transfer(&contract_id, &session.payer, &total_locked);

        session.status = SessionStatus::Refunded;
        session.updated_at = now;

        let key = DataKey::Session(session_id.clone());
        env.storage().persistent().set(&key, &session);

        // Remove from expiry index
        Self::remove_from_expiry_index(env.clone(), session_id.clone(), session.expires_at)?;

        env.events().publish(
            (Symbol::new(&env, "AutoRefundExecuted"),),
            (session_id, session.payer, total_locked),
        );

        Ok(())
    }

    pub fn open_dispute(env: Env, session_id: Bytes, caller: Address, reason: Bytes) -> Result<(), Error> {
        caller.require_auth();

        let mut session = Self::get_session(env.clone(), session_id.clone()).ok_or(Error::SessionNotFound)?;

        // Authorization check: only payer or payee can open a dispute
        if caller != session.payer && caller != session.payee {
            return Err(Error::Unauthorized);
        }

        // Status check: must be Locked or Completed
        if session.status != SessionStatus::Locked && session.status != SessionStatus::Completed {
            return Err(Error::InvalidSessionStatus);
        }

        let now = env.ledger().timestamp();
        
        // Update session
        session.status = SessionStatus::Disputed;
        session.updated_at = now;
        session.dispute_opened_at = now;

        let key = DataKey::Session(session_id.clone());
        env.storage().persistent().set(&key, &session);

        // Emit DisputeOpened event
        env.events().publish(
            (Symbol::new(&env, "DisputeOpened"),),
            (session_id, caller, reason),
        );

        Ok(())
    }

    pub fn resolve_dispute(
        env: Env,
        session_id: Bytes,
        to_payer: i128,
        to_payee: i128,
        note: Option<Bytes>,
    ) -> Result<(), Error> {
        // Validate note length
        validate_note(&note)?;

        // Require admin authorization
        let admin = read_admin(&env)?;
        admin.require_auth();

        let mut session = Self::get_session(env.clone(), session_id.clone()).ok_or(Error::SessionNotFound)?;

        // Validate session status is Disputed
        if session.status != SessionStatus::Disputed {
            return Err(Error::SessionNotDisputed);
        }

        // Calculate total locked amount (amount + fee)
        let fee = session.amount
            .checked_mul(session.fee_bps as i128)
            .ok_or(Error::FeeCalculationOverflow)?
            .checked_div(10000)
            .ok_or(Error::FeeCalculationOverflow)?;
        
        let total_available = session.amount.checked_add(fee).ok_or(Error::FeeCalculationOverflow)?;

        // The resolution split (to_payer + to_payee) should sum to the original 'amount'.
        // The 'fee' is transferred to the treasury as compensation for platform services.
        if to_payer + to_payee != session.amount {
            return Err(Error::InvalidResolutionAmount);
        }

        if to_payer < 0 || to_payee < 0 {
            return Err(Error::InvalidResolutionAmount);
        }

        let treasury = Self::get_treasury(env.clone());
        let token_client = token::Client::new(&env, &session.asset);
        let contract_id = env.current_contract_address();

        // Perform transfers
        if to_payer > 0 {
            token_client.transfer(&contract_id, &session.payer, &to_payer);
        }
        if to_payee > 0 {
            token_client.transfer(&contract_id, &session.payee, &to_payee);
        }
        if fee > 0 {
            token_client.transfer(&contract_id, &treasury, &fee);
        }

        let now = env.ledger().timestamp();
        session.status = SessionStatus::Resolved;
        session.updated_at = now;
        session.resolved_at = now;
        session.resolver = Some(admin);
        session.resolution_note = note;

        let key = DataKey::Session(session_id.clone());
        env.storage().persistent().set(&key, &session);

        // Remove from expiry index
        Self::remove_from_expiry_index(env.clone(), session_id.clone(), session.expires_at)?;

        // Emit DisputeResolved event
        env.events().publish(
            (Symbol::new(&env, "DisputeResolved"),),
            (session_id, to_payer, to_payee, fee),
        );

        Ok(())
    }

    pub fn get_dispute_window(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::DisputeWindow)
            .unwrap_or(DEFAULT_DISPUTE_WINDOW_SECONDS)
    }

    pub fn get_treasury(env: Env) -> Address {
        match env.storage().instance().get(&DataKey::Treasury) {
            Some(addr) => addr,
            None => read_admin(&env).unwrap_or_else(|_| panic_with_error!(&env, Error::NotInitialized)),
        }
    }

    fn add_to_expiry_index(env: Env, session_id: Bytes, expires_at: u64) -> Result<(), Error> {
        let day_bucket = expires_at / SECONDS_PER_DAY;
        let key = DataKey::ExpiryIndex(day_bucket);
        let mut session_ids: Vec<Bytes> = env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(&env));
        if !session_ids.contains(&session_id) {
            session_ids.push_back(session_id);
            env.storage().persistent().set(&key, &session_ids);
        }
        Ok(())
    }

    fn remove_from_expiry_index(env: Env, session_id: Bytes, expires_at: u64) -> Result<(), Error> {
        let day_bucket = expires_at / SECONDS_PER_DAY;
        let key = DataKey::ExpiryIndex(day_bucket);
        if let Some(mut session_ids) = env.storage().persistent().get::<_, Vec<Bytes>>(&key) {
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
}

fn read_admin(env: &Env) -> Result<Address, Error> {
    env.storage().instance().get(&DataKey::Admin).ok_or(Error::NotInitialized)
}

fn acquire_lock(env: &Env) -> Result<(), Error> {
    if env.storage().instance().get(&DataKey::ReentrancyLock).unwrap_or(false) {
        return Err(Error::Reentrancy);
    }
    env.storage().instance().set(&DataKey::ReentrancyLock, &true);
    Ok(())
}

fn release_lock(env: &Env) {
    env.storage().instance().set(&DataKey::ReentrancyLock, &false);
}

fn use_nonce(env: &Env, addr: &Address, nonce: u64) -> Result<(), Error> {
    let key = DataKey::Nonce(addr.clone());
    let current = env.storage().persistent().get(&key).unwrap_or(0);
    if nonce <= current {
        return Err(Error::NonceAlreadyUsed);
    }
    env.storage().persistent().set(&key, &nonce);
    Ok(())
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

fn validate_session_id(session_id: &Bytes) -> Result<(), Error> {
    if session_id.len() == 0 || session_id.len() > MAX_SESSION_ID_LEN {
        return Err(Error::InvalidSessionId);
    }
    Ok(())
}

fn validate_amount(amount: i128) -> Result<(), Error> {
    if amount <= 0 || amount > MAX_AMOUNT {
        return Err(Error::InvalidAmount);
    }
    Ok(())
}

fn validate_different_addresses(addr1: &Address, addr2: &Address) -> Result<(), Error> {
    if addr1 == addr2 {
        return Err(Error::InvalidAddress);
    }
    Ok(())
}

fn validate_note(note: &Option<Bytes>) -> Result<(), Error> {
    if let Some(n) = note {
        if n.len() > MAX_NOTE_LEN {
            return Err(Error::InvalidNote);
        }
    }
    Ok(())
}
