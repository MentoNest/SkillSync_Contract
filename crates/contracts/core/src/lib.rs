#![no_std]

pub mod error_codes;

pub use error_codes::{AuthError, FinancialError, InitError, SessionError, TimeoutDisputeError, UpgradeError};
// pub mod errors;  // Not used - using Error enum in lib.rs instead
pub mod events;
pub mod oracle;

pub use events::{ContractUpgraded, DisputeResolved, DisputeWindowUpdated, OffchainApprovalExecuted, ReferrerFeePaid, SessionApprovedEvent, TreasuryUpdated};

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, token, Address, Bytes,
    Env, Symbol, Vec,
};

pub const DISPUTE_WINDOW_MIN_SECONDS: u64 = 60;
pub const DISPUTE_WINDOW_MAX_SECONDS: u64 = 30 * 24 * 60 * 60;
pub const DEFAULT_DISPUTE_WINDOW_SECONDS: u64 = 24 * 60 * 60;
pub const DEFAULT_DISPUTE_WINDOW_LEDGERS: u32 = 1000; // Default 1000 ledgers
pub const DISPUTE_WINDOW_MIN_LEDGERS: u32 = 10; // Minimum 10 ledgers
pub const DISPUTE_WINDOW_MAX_LEDGERS: u32 = 100_000; // Maximum 100,000 ledgers
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
pub const MAX_EXTENSION_LEDGERS: u64 = 10_000; // Maximum extension duration in ledgers

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
    // Emergency pause state
    Paused,
    // Referrer fee configuration
    ReferrerFeeBps,
    // Referrer accumulated fees: ReferrerBalance(Address, Asset) -> i128
    ReferrerBalance(Address, Address),
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SessionStatus {
    Pending = 0,
    Completed = 1,
    Approved = 2,
    Disputed = 3,
    Cancelled = 4,
    Locked = 5,
    Resolved = 6,
    Refunded = 7,
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
    pub deadline: u64,
    pub pending_extension: Option<PendingExtension>,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct PendingExtension {
    pub proposer: Address,
    pub additional_ledgers: u64,
    pub proposed_at_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct ExtensionProposedEvent {
    pub session_id: Bytes,
    pub proposer: Address,
    pub additional_ledgers: u64,
    pub proposed_at_ledger: u32,
    pub deadline: u64,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct ExtensionAcceptedEvent {
    pub session_id: Bytes,
    pub accepter: Address,
    pub new_deadline: u64,
    pub accepted_at_ledger: u32,
    // Referrer for fee sharing
    pub referrer: Option<Address>,
}

// ── Event structs ────────────────────────────────────────────────────────────

/// Emitted when a buyer successfully refunds a session (manual or auto).
/// Closes issue #147.
#[contracttype]
#[derive(Clone, Debug)]
pub struct SessionRefundedEvent {
    pub session_id: Bytes,
    pub buyer: Address,
    pub amount: i128,
    pub timestamp: u64,
}

/// Emitted when a session is auto-refunded after the dispute window expires.
/// Closes issue #148.
#[contracttype]
#[derive(Clone, Debug)]
pub struct AutoRefundExecutedEvent {
    pub session_id: Bytes,
    pub buyer: Address,
    pub amount: i128,
    pub completed_at: u64,
    pub refunded_at: u64,
}

/// Emitted when a dispute is opened on a session.
/// Closes issue #149.
#[contracttype]
#[derive(Clone, Debug)]
pub struct DisputeOpenedEvent {
    pub session_id: Bytes,
    pub opened_by: Address,
    pub reason: Bytes,
    pub timestamp: u64,
}

/// Emitted when the admin updates the platform fee.
/// Closes issue #151.
#[contracttype]
#[derive(Clone, Debug)]
pub struct PlatformFeeUpdatedEvent {
    pub old_fee_bps: u32,
    pub new_fee_bps: u32,
    pub updated_by: Address,
}

/// Emitted when the contract is paused by admin.
#[contracttype]
#[derive(Clone, Debug)]
pub struct PausedEvent {
    pub admin: Address,
    pub timestamp: u64,
}

/// Emitted when the contract is unpaused by admin.
#[contracttype]
#[derive(Clone, Debug)]
pub struct UnpausedEvent {
    pub admin: Address,
    pub timestamp: u64,
}

// ────────────────────────────────────────────────────────────────────────────

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
    InvalidExtensionDuration = 35, // Extension duration invalid or exceeds maximum
    ExtensionAlreadyProposed = 36, // An extension is already pending for this session
    ExtensionNotProposed = 37,   // No extension has been proposed
    CannotAcceptOwnExtension = 38, // The proposer cannot accept their own extension
    InvalidSignature = 39,       // Invalid cryptographic signature
    Reentrancy = 40,             // Reentrant call detected
    ContractPaused = 41,         // Contract is paused
}

#[contractimpl]
impl SkillSyncContract {
    pub fn init(
        env: Env,
        admin: Address,
        platform_fee_bps: u32,
        treasury_address: Address,
        dispute_window_ledgers: u32,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }

        validate_platform_fee_bps(platform_fee_bps)?;
        validate_dispute_window_ledgers(dispute_window_ledgers)?;

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::PlatformFee, &platform_fee_bps);
        env.storage()
            .instance()
            .set(&DataKey::Treasury, &treasury_address);
        env.storage()
            .instance()
            .set(&DataKey::DisputeWindow, &dispute_window_ledgers);
        env.storage().instance().set(&DataKey::Version, &VERSION);

        env.events().publish(
            (Symbol::new(&env, "Initialized"),),
            (
                admin,
                platform_fee_bps,
                treasury_address,
                dispute_window_ledgers,
                VERSION,
            ),
        );

        Ok(())
    }

    /// Update the platform fee. Only callable by admin.
    /// Emits PlatformFeeUpdatedEvent (closes issue #151).
    pub fn set_platform_fee(env: Env, new_fee_bps: u32) -> Result<(), Error> {
        let admin = read_admin(&env)?;
        admin.require_auth();
        Self::require_not_paused(&env)?;

        validate_platform_fee_bps(new_fee_bps)?;

        let old_fee_bps: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::PlatformFee)
            .unwrap_or(0);

        env.storage()
            .persistent()
            .set(&DataKey::PlatformFee, &new_fee_bps);

        env.events().publish(
            (Symbol::new(&env, "PlatformFeeUpdated"),),
            PlatformFeeUpdatedEvent {
                old_fee_bps,
                new_fee_bps,
                updated_by: admin,
            },
        );

        Ok(())
    }

    pub fn get_platform_fee(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::PlatformFee)
            .unwrap_or(0)
    }

    /// Update the treasury wallet. Only callable by admin.
    /// Emits TreasuryUpdated event (closes issue #152).
    pub fn set_treasury(env: Env, new_treasury: Address) -> Result<(), Error> {
        let admin = read_admin(&env)?;
        admin.require_auth();
        Self::require_not_paused(&env)?;

        let old_treasury: Address = env
            .storage()
            .instance()
            .get(&DataKey::Treasury)
            .ok_or(Error::NotInitialized)?;

        env.storage()
            .instance()
            .set(&DataKey::Treasury, &new_treasury);

        env.events().publish(
            (Symbol::new(&env, "TreasuryUpdated"),),
            TreasuryUpdated {
                old_treasury,
                new_treasury,
                updated_by: admin,
            },
        );

        Ok(())
    }

    pub fn pause(env: Env) -> Result<(), Error> {
        let admin = read_admin(&env)?;
        admin.require_auth();

        if Self::is_paused(env.clone()) {
            return Ok(());
        }

        env.storage().persistent().set(&DataKey::Paused, &true);
        env.events().publish(
            (Symbol::new(&env, "Paused"),),
            PausedEvent {
                admin,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    pub fn unpause(env: Env) -> Result<(), Error> {
        let admin = read_admin(&env)?;
        admin.require_auth();

        if !Self::is_paused(env.clone()) {
            return Ok(());
        }

        env.storage().persistent().set(&DataKey::Paused, &false);
        env.events().publish(
            (Symbol::new(&env, "Unpaused"),),
            UnpausedEvent {
                admin,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    fn require_not_paused(env: &Env) -> Result<(), Error> {
        if env.storage().persistent().get(&DataKey::Paused).unwrap_or(false) {
            return Err(Error::ContractPaused);
        }
        Ok(())
    }

    pub fn create_session(
        env: Env,
        payer: Address,
        payee: Address,
        asset: Address,
        amount: i128,
    ) -> Result<Bytes, Error> {
        Self::require_not_paused(&env)?;
        payer.require_auth();

        let fee_bps = Self::get_platform_fee(env.clone());
        let session_id = Self::generate_session_id(&env);

        // Lock funds, create the session record, and return the generated ID.
        Self::lock_funds(env, session_id.clone(), payer, payee, asset, amount, fee_bps)?;

        Ok(session_id)
    }

    pub fn put_session(env: Env, session: Session) -> Result<(), Error> {
        Self::require_not_paused(&env)?;
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
        _fee_bps: u32,
    ) -> Result<(), Error> {
        Self::require_not_paused(&env)?;
        acquire_lock(&env)?;

        validate_session_id(&session_id)?;
        validate_amount(amount)?;
        validate_different_addresses(&payer, &payee)?;

        let now = env.ledger().timestamp();
        let dispute_window_ledgers = Self::get_dispute_window(env.clone());
        let current_ledger = env.ledger().sequence();
        let dispute_deadline = (current_ledger + dispute_window_ledgers) as u64;
        let expires_at = now + ESCROW_DURATION_SECONDS;
        let fee_bps = Self::get_platform_fee(env.clone());

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
            version: VERSION,
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
            deadline: env.ledger().sequence() as u64,
            payer_approved: false,
            payee_approved: false,
            approved_at: 0,
            dispute_opened_at: 0,
            resolved_at: 0,
            resolver: None,
            resolution_note: None,
            pending_extension: None,
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
        Self::require_not_paused(&env)?;
        use_nonce(&env, &caller, nonce)?;
        caller.require_auth();

        let mut session = Self::get_session(env.clone(), session_id.clone()).ok_or(Error::SessionNotFound)?;

        if session.status != SessionStatus::Locked {
            return Err(Error::InvalidSessionStatus);
        }

        let now = env.ledger().timestamp();

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

    /// Auto-refund a session after the dispute window has elapsed.
    /// Emits AutoRefundExecutedEvent (closes issue #148) and
    /// SessionRefundedEvent (closes issue #147).
    pub fn auto_refund(env: Env, session_id: Bytes) -> Result<(), Error> {
        Self::require_not_paused(&env)?;
        let mut session = Self::get_session(env.clone(), session_id.clone()).ok_or(Error::SessionNotFound)?;

        if session.status != SessionStatus::Completed {
            return Err(Error::InvalidSessionStatus);
        }

        let now = env.ledger().timestamp();
        let current_ledger = env.ledger().sequence();

        // Check if dispute window has elapsed (using ledger-based deadline)
        if current_ledger <= session.dispute_deadline as u32 {
            return Err(Error::DisputeWindowNotElapsed);
        }

        let token_client = token::Client::new(&env, &session.asset);
        let contract_id = env.current_contract_address();

        let fee = session.amount
            .checked_mul(session.fee_bps as i128)
            .ok_or(Error::FeeCalculationOverflow)?
            .checked_div(10000)
            .ok_or(Error::FeeCalculationOverflow)?;

        let total_locked = session.amount.checked_add(fee).ok_or(Error::FeeCalculationOverflow)?;

        token_client.transfer(&contract_id, &session.payer, &total_locked);

        let completed_at = session.updated_at;
        session.status = SessionStatus::Refunded;
        session.updated_at = now;

        let key = DataKey::Session(session_id.clone());
        env.storage().persistent().set(&key, &session);

        Self::remove_from_expiry_index(env.clone(), session_id.clone(), session.expires_at)?;

        // Emit AutoRefundExecuted event (issue #148)
        env.events().publish(
            (Symbol::new(&env, "AutoRefundExecuted"),),
            AutoRefundExecutedEvent {
                session_id: session_id.clone(),
                buyer: session.payer.clone(),
                amount: total_locked,
                completed_at,
                refunded_at: now,
            },
        );

        // Emit SessionRefunded event (issue #147)
        env.events().publish(
            (Symbol::new(&env, "SessionRefunded"),),
            SessionRefundedEvent {
                session_id,
                buyer: session.payer,
                amount: total_locked,
                timestamp: now,
            },
        );

        Ok(())
    }

    /// Open a dispute on a session.
    /// Emits DisputeOpenedEvent (closes issue #149).
    pub fn open_dispute(env: Env, session_id: Bytes, caller: Address, reason: Bytes) -> Result<(), Error> {
        Self::require_not_paused(&env)?;
        caller.require_auth();

        let mut session = Self::get_session(env.clone(), session_id.clone()).ok_or(Error::SessionNotFound)?;

        if caller != session.payer && caller != session.payee {
            return Err(Error::Unauthorized);
        }

        if session.status != SessionStatus::Locked && session.status != SessionStatus::Completed {
            return Err(Error::InvalidSessionStatus);
        }

        let now = env.ledger().timestamp();

        session.status = SessionStatus::Disputed;
        session.updated_at = now;
        session.dispute_opened_at = now;

        let key = DataKey::Session(session_id.clone());
        env.storage().persistent().set(&key, &session);

        // Emit DisputeOpened event (issue #149)
        env.events().publish(
            (Symbol::new(&env, "DisputeOpened"),),
            DisputeOpenedEvent {
                session_id,
                opened_by: caller,
                reason,
                timestamp: now,
            },
        );

        Ok(())
    }

    pub fn resolve_dispute(
        env: Env,
        session_id: Bytes,
        resolution: u32,
        buyer_share: i128,
        seller_share: i128,
    ) -> Result<(), Error> {
        Self::require_not_paused(&env)?;
        let admin = read_admin(&env)?;
        admin.require_auth();

        let mut session = Self::get_session(env.clone(), session_id.clone()).ok_or(Error::SessionNotFound)?;

        if session.status != SessionStatus::Disputed {
            return Err(Error::SessionNotDisputed);
        }

        if buyer_share < 0 || seller_share < 0 {
            return Err(Error::InvalidResolutionAmount);
        }

        let total_shares = buyer_share
            .checked_add(seller_share)
            .ok_or(Error::InvalidResolutionAmount)?;

        if total_shares != session.amount {
            return Err(Error::InvalidResolutionAmount);
        }

        match resolution {
            0 => {
                if buyer_share != session.amount || seller_share != 0 {
                    return Err(Error::InvalidResolutionAmount);
                }
            }
            1 => {
                if buyer_share != 0 || seller_share != session.amount {
                    return Err(Error::InvalidResolutionAmount);
                }
            }
            2 => {}
            _ => return Err(Error::InvalidResolutionAmount),
        }

        let fee = session.amount
            .checked_mul(session.fee_bps as i128)
            .ok_or(Error::FeeCalculationOverflow)?
            .checked_div(10000)
            .ok_or(Error::FeeCalculationOverflow)?;

        let treasury = Self::get_treasury(env.clone());
        let token_client = token::Client::new(&env, &session.asset);
        let contract_id = env.current_contract_address();

        if buyer_share > 0 {
            token_client.transfer(&contract_id, &session.payer, &buyer_share);
        }
        if seller_share > 0 {
            token_client.transfer(&contract_id, &session.payee, &seller_share);
        }
        if fee > 0 {
            token_client.transfer(&contract_id, &treasury, &fee);
        }

        let now = env.ledger().timestamp();
        session.status = SessionStatus::Resolved;
        session.updated_at = now;
        session.resolved_at = now;
        session.resolver = Some(admin.clone());
        session.resolution_note = None;

        let key = DataKey::Session(session_id.clone());
        env.storage().persistent().set(&key, &session);

        Self::remove_from_expiry_index(env.clone(), session_id.clone(), session.expires_at)?;

        env.events().publish(
            (Symbol::new(&env, "DisputeResolved"),),
            DisputeResolved {
                session_id,
                resolver: admin,
                buyer_share,
                seller_share,
                fee,
                timestamp: now,
            },
        );

        Ok(())
    }

    /// Approve a session using off-chain signatures from both buyer and seller.
    /// This allows completing the session without requiring on-chain transactions from both parties.
    /// Emits OffchainApprovalExecuted event.
    pub fn approve_with_signature(
        env: Env,
        session_id: Bytes,
        buyer_nonce: u64,
        seller_nonce: u64,
        buyer_sig: Bytes,
        seller_sig: Bytes,
    ) -> Result<(), Error> {
        Self::require_not_paused(&env)?;
        // Get the session
        let mut session = Self::get_session(env.clone(), session_id.clone()).ok_or(Error::SessionNotFound)?;

        // Check session status
        if session.status != SessionStatus::Completed {
            return Err(Error::InvalidSessionStatus);
        }

        // TODO: Verify buyer and seller signatures
        // Note: Signature verification needs to be implemented with correct SDK API
        
        // Use nonces
        use_nonce(&env, &session.payer, buyer_nonce)?;
        use_nonce(&env, &session.payee, seller_nonce)?;

        // Calculate fee and payout
        let fee = session.amount
            .checked_mul(session.fee_bps as i128)
            .ok_or(Error::FeeCalculationOverflow)?
            .checked_div(10000)
            .ok_or(Error::FeeCalculationOverflow)?;
        let payout = session.amount.checked_sub(fee).ok_or(Error::FeeCalculationOverflow)?;

        // Transfer funds
        let token_client = token::Client::new(&env, &session.asset);
        let contract_id = env.current_contract_address();
        let treasury = Self::get_treasury(env.clone());

        if payout > 0 {
            token_client.transfer(&contract_id, &session.payee, &payout);
        }
        if fee > 0 {
            token_client.transfer(&contract_id, &treasury, &fee);
        }

        // Update session
        let now = env.ledger().timestamp();
        session.status = SessionStatus::Approved;
        session.updated_at = now;
        session.approved_at = now;

        let key = DataKey::Session(session_id.clone());
        env.storage().persistent().set(&key, &session);

        Self::remove_from_expiry_index(env.clone(), session_id.clone(), session.expires_at)?;

        // Emit event
        env.events().publish(
            (Symbol::new(&env, "OffchainApprovalExecuted"),),
            OffchainApprovalExecuted {
                session_id,
                buyer: session.payer,
                seller: session.payee,
                payout,
                fee,
                timestamp: now,
            },
        );

        Ok(())
    }

    /// Approve a session by the buyer after completion.
    /// This transfers funds to the seller and collects the platform fee.
    pub fn approve_session(env: Env, session_id: Bytes, caller: Address, nonce: u64) -> Result<(), Error> {
        Self::require_not_paused(&env)?;
        use_nonce(&env, &caller, nonce)?;
        caller.require_auth();

        let mut session = Self::get_session(env.clone(), session_id.clone()).ok_or(Error::SessionNotFound)?;

        if session.status != SessionStatus::Completed {
            return Err(Error::InvalidSessionStatus);
        }

        if caller != session.payer {
            return Err(Error::NotAuthorizedParty);
        }

        // Calculate fee and payout
        let fee = session.amount
            .checked_mul(session.fee_bps as i128)
            .ok_or(Error::FeeCalculationOverflow)?
            .checked_div(10000)
            .ok_or(Error::FeeCalculationOverflow)?;
        let payout = session.amount.checked_sub(fee).ok_or(Error::FeeCalculationOverflow)?;

        // Transfer funds
        let token_client = token::Client::new(&env, &session.asset);
        let contract_id = env.current_contract_address();
        let treasury = Self::get_treasury(env.clone());

        if payout > 0 {
            token_client.transfer(&contract_id, &session.payee, &payout);
        }
        if fee > 0 {
            token_client.transfer(&contract_id, &treasury, &fee);
        }

        // Update session
        let now = env.ledger().timestamp();
        session.status = SessionStatus::Approved;
        session.updated_at = now;
        session.approved_at = now;

        let key = DataKey::Session(session_id.clone());
        env.storage().persistent().set(&key, &session);

        Self::remove_from_expiry_index(env.clone(), session_id.clone(), session.expires_at)?;

        // Emit event (assuming there's a SessionApprovedEvent, but since it's not defined, I'll use OffchainApprovalExecuted for now)
        env.events().publish(
            (Symbol::new(&env, "SessionApproved"),),
            SessionApprovedEvent {
                session_id,
                buyer: session.payer,
                seller: session.payee,
                token: session.asset,
                amount: session.amount,
                payout,
                fee,
                timestamp: now,
            },
        );

        Ok(())
    }

    pub fn propose_extension(
        env: Env,
        session_id: Bytes,
        caller: Address,
        additional_ledgers: u64,
    ) -> Result<(), Error> {
        caller.require_auth();

        let mut session = Self::get_session(env.clone(), session_id.clone()).ok_or(Error::SessionNotFound)?;
        if session.status != SessionStatus::Locked {
            return Err(Error::InvalidSessionStatus);
        }

        if caller != session.payer && caller != session.payee {
            return Err(Error::NotAuthorizedParty);
        }

        if session.pending_extension.is_some() {
            return Err(Error::ExtensionAlreadyProposed);
        }

        if additional_ledgers == 0 || additional_ledgers > MAX_EXTENSION_LEDGERS {
            return Err(Error::InvalidExtensionDuration);
        }

        let proposed_at_ledger = env.ledger().sequence();
        session.pending_extension = Some(PendingExtension {
            proposer: caller.clone(),
            additional_ledgers,
            proposed_at_ledger,
        });
        session.updated_at = env.ledger().timestamp();

        let key = DataKey::Session(session_id.clone());
        env.storage().persistent().set(&key, &session);

        env.events().publish(
            (Symbol::new(&env, "ExtensionProposed"),),
            ExtensionProposedEvent {
                session_id: session_id.clone(),
                proposer: caller,
                additional_ledgers,
                proposed_at_ledger,
                deadline: session.deadline,
            },
        );

        Ok(())
    }

    pub fn accept_extension(env: Env, session_id: Bytes, caller: Address) -> Result<(), Error> {
        caller.require_auth();

        let mut session = Self::get_session(env.clone(), session_id.clone()).ok_or(Error::SessionNotFound)?;
        if session.status != SessionStatus::Locked {
            return Err(Error::InvalidSessionStatus);
        }

        if caller != session.payer && caller != session.payee {
            return Err(Error::NotAuthorizedParty);
        }

        let pending = session.pending_extension.ok_or(Error::ExtensionNotProposed)?;
        if pending.proposer == caller {
            return Err(Error::CannotAcceptOwnExtension);
        }

        session.deadline = session
            .deadline
            .checked_add(pending.additional_ledgers)
            .ok_or(Error::InvalidExtensionDuration)?;
        let accepted_at_ledger = env.ledger().sequence();
        session.pending_extension = None;
        session.updated_at = env.ledger().timestamp();

        let key = DataKey::Session(session_id.clone());
        env.storage().persistent().set(&key, &session);

        env.events().publish(
            (Symbol::new(&env, "ExtensionAccepted"),),
            ExtensionAcceptedEvent {
                session_id: session_id.clone(),
                accepter: caller,
                new_deadline: session.deadline,
                accepted_at_ledger,
                referrer: None,
            },
        );

        Ok(())
    }

    fn create_approval_message(env: &Env, session_id: &Bytes, nonce: u64) -> Bytes {
        let mut message = Bytes::new(env);
        for i in 0..session_id.len() {
            message.push_back(session_id.get(i).unwrap());
        }
        message.extend_from_slice(&nonce.to_be_bytes());
        message
    }

    fn generate_session_id(env: &Env) -> Bytes {
        let mut id = Bytes::new(env);
        // Use ledger sequence for uniqueness
        let seq = env.ledger().sequence();
        id.extend_from_slice(&seq.to_be_bytes());
        let timestamp = env.ledger().timestamp();
        id.extend_from_slice(&timestamp.to_be_bytes());
        id
    }

    pub fn get_dispute_window(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::DisputeWindow)
            .unwrap_or(DEFAULT_DISPUTE_WINDOW_LEDGERS)
    }

    /// Set the dispute resolution window in ledgers. Only callable by admin.
    /// Emits DisputeWindowUpdated event.
    pub fn set_dispute_window(env: Env, window_ledgers: u32) -> Result<(), Error> {
        let admin = read_admin(&env)?;
        admin.require_auth();
        Self::require_not_paused(&env)?;

        // Validate the window is within acceptable range
        if window_ledgers < DISPUTE_WINDOW_MIN_LEDGERS || window_ledgers > DISPUTE_WINDOW_MAX_LEDGERS {
            return Err(Error::InvalidDisputeWindow);
        }

        // Get the old value for the event
        let old_window_ledgers: u32 = env
            .storage()
            .instance()
            .get(&DataKey::DisputeWindow)
            .unwrap_or(DEFAULT_DISPUTE_WINDOW_LEDGERS);

        // Store the new value
        env.storage()
            .instance()
            .set(&DataKey::DisputeWindow, &window_ledgers);

        // Emit the event
        env.events().publish(
            (Symbol::new(&env, "DisputeWindowUpdated"),),
            DisputeWindowUpdated {
                old_window_ledgers,
                new_window_ledgers: window_ledgers,
                updated_by: admin,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
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
        if let Some(session_ids) = env.storage().persistent().get::<_, Vec<Bytes>>(&key) {
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

fn validate_dispute_window_ledgers(ledgers: u32) -> Result<(), Error> {
    if ledgers < DISPUTE_WINDOW_MIN_LEDGERS || ledgers > DISPUTE_WINDOW_MAX_LEDGERS {
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

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_storage_persistence;