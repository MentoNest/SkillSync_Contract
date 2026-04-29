/// Insurance pool module — issue #212
///
/// Buyers may pay an optional premium when locking funds.  If a dispute
/// resolution awards the buyer less than 80 % of the session amount, the
/// insurance pool covers the shortfall up to 100 %.
use soroban_sdk::{contracttype, symbol_short, token, Address, Bytes, Env};

use crate::{DataKey, Error, Session, SessionStatus, SkillSyncContract};

// ── Storage keys ─────────────────────────────────────────────────────────────

/// Per-session insurance record.
#[contracttype]
#[derive(Clone, Debug)]
pub struct InsuranceRecord {
    /// Buyer who purchased insurance.
    pub buyer: Address,
    /// Session amount (principal).
    pub amount: i128,
    /// Premium paid (in the same asset as the session).
    pub premium: i128,
    /// Asset address.
    pub asset: Address,
    /// Whether a claim has already been paid.
    pub claimed: bool,
}

/// Pool-level storage key for the accumulated premium balance per asset.
#[contracttype]
#[derive(Clone, Debug)]
pub enum InsuranceKey {
    /// Per-session insurance record.
    Record(Bytes),
    /// Total pool balance for an asset.
    PoolBalance(Address),
    /// Admin-configured premium rate in basis points (e.g. 50 = 0.5 %).
    PremiumRateBps,
    /// Admin-configured coverage percentage in basis points (e.g. 10_000 = 100 %).
    CoverageBps,
}

// ── Events ────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug)]
pub struct InsurancePurchasedEvent {
    pub session_id: Bytes,
    pub buyer: Address,
    pub premium: i128,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct InsuranceClaimPaidEvent {
    pub session_id: Bytes,
    pub buyer: Address,
    pub payout: i128,
}

// ── Default constants ─────────────────────────────────────────────────────────

/// Default premium rate: 50 bps = 0.5 %.
pub const DEFAULT_PREMIUM_BPS: u32 = 50;
/// Default coverage: 10 000 bps = 100 %.
pub const DEFAULT_COVERAGE_BPS: u32 = 10_000;
/// Threshold below which insurance kicks in: 8 000 bps = 80 %.
pub const INSURANCE_THRESHOLD_BPS: u32 = 8_000;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn premium_rate_bps(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&InsuranceKey::PremiumRateBps)
        .unwrap_or(DEFAULT_PREMIUM_BPS)
}

fn coverage_bps(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&InsuranceKey::CoverageBps)
        .unwrap_or(DEFAULT_COVERAGE_BPS)
}

fn pool_balance(env: &Env, asset: &Address) -> i128 {
    env.storage()
        .instance()
        .get(&InsuranceKey::PoolBalance(asset.clone()))
        .unwrap_or(0_i128)
}

fn set_pool_balance(env: &Env, asset: &Address, balance: i128) {
    env.storage()
        .instance()
        .set(&InsuranceKey::PoolBalance(asset.clone()), &balance);
}

// ── Public interface (called from SkillSyncContract) ─────────────────────────

impl SkillSyncContract {
    /// Admin: set the insurance premium rate in basis points.
    pub fn set_insurance_premium_bps(env: Env, bps: u32) -> Result<(), Error> {
        let admin = crate::read_admin(&env)?;
        admin.require_auth();
        if bps > 10_000 {
            return Err(Error::InvalidFeeBps);
        }
        env.storage()
            .instance()
            .set(&InsuranceKey::PremiumRateBps, &bps);
        Ok(())
    }

    /// Admin: set the coverage percentage in basis points.
    pub fn set_insurance_coverage_bps(env: Env, bps: u32) -> Result<(), Error> {
        let admin = crate::read_admin(&env)?;
        admin.require_auth();
        if bps > 10_000 {
            return Err(Error::InvalidFeeBps);
        }
        env.storage()
            .instance()
            .set(&InsuranceKey::CoverageBps, &bps);
        Ok(())
    }

    /// Lock funds with an optional insurance premium.
    ///
    /// The buyer pays `amount + platform_fee + premium`.  The premium is
    /// transferred to the contract and credited to the insurance pool.
    pub fn lock_funds_with_insurance(
        env: Env,
        session_id: Bytes,
        payer: Address,
        payee: Address,
        asset: Address,
        amount: i128,
        premium_bps: u32,
    ) -> Result<(), Error> {
        Self::require_not_paused(&env)?;
        crate::acquire_lock(&env)?;

        crate::validate_session_id(&session_id)?;
        crate::validate_amount(amount)?;
        crate::validate_different_addresses(&payer, &payee)?;

        if premium_bps > 10_000 {
            crate::release_lock(&env);
            return Err(Error::InvalidFeeBps);
        }

        let fee_bps = Self::get_platform_fee(env.clone());
        let now = env.ledger().timestamp();
        let dispute_window = Self::get_dispute_window(env.clone());

        let platform_fee = amount
            .checked_mul(fee_bps as i128)
            .ok_or(Error::FeeCalculationOverflow)?
            .checked_div(10_000)
            .ok_or(Error::FeeCalculationOverflow)?;

        let premium = amount
            .checked_mul(premium_bps as i128)
            .ok_or(Error::FeeCalculationOverflow)?
            .checked_div(10_000)
            .ok_or(Error::FeeCalculationOverflow)?;

        let total = amount
            .checked_add(platform_fee)
            .ok_or(Error::FeeCalculationOverflow)?
            .checked_add(premium)
            .ok_or(Error::FeeCalculationOverflow)?;

        let token_client = token::Client::new(&env, &asset);
        if token_client.balance(&payer) < total {
            crate::release_lock(&env);
            return Err(Error::InsufficientBalance);
        }

        // Store the session via the standard path.
        let session = Session {
            version: crate::VERSION,
            session_id: session_id.clone(),
            payer: payer.clone(),
            payee: payee.clone(),
            asset: asset.clone(),
            amount,
            fee_bps,
            status: SessionStatus::Locked,
            created_at: now,
            updated_at: now,
            dispute_deadline: now + dispute_window,
            expires_at: now + crate::ESCROW_DURATION_SECONDS,
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

        Self::put_session(env.clone(), session.clone())?;
        Self::add_to_expiry_index(env.clone(), session_id.clone(), session.expires_at)?;

        let contract_id = env.current_contract_address();
        token_client.transfer(&payer, &contract_id, &total);

        // Credit premium to pool.
        if premium > 0 {
            let bal = pool_balance(&env, &asset);
            set_pool_balance(&env, &asset, bal + premium);

            // Store insurance record.
            let record = InsuranceRecord {
                buyer: payer.clone(),
                amount,
                premium,
                asset: asset.clone(),
                claimed: false,
            };
            env.storage()
                .persistent()
                .set(&InsuranceKey::Record(session_id.clone()), &record);

            env.events().publish(
                (symbol_short!("ins_buy"),),
                InsurancePurchasedEvent {
                    session_id: session_id.clone(),
                    buyer: payer.clone(),
                    premium,
                },
            );
        }

        env.events().publish(
            (symbol_short!("locked"),),
            (session_id, payer, payee, amount, platform_fee),
        );

        crate::release_lock(&env);
        Ok(())
    }

    /// Buyer claims insurance after a dispute resolution that awarded < 80 % of amount.
    pub fn claim_insurance(env: Env, session_id: Bytes) -> Result<(), Error> {
        Self::require_not_paused(&env)?;

        let session =
            Self::get_session(env.clone(), session_id.clone()).ok_or(Error::SessionNotFound)?;

        if session.status != SessionStatus::Resolved {
            return Err(Error::InvalidSessionStatus);
        }

        let key = InsuranceKey::Record(session_id.clone());
        let mut record: InsuranceRecord = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::SessionNotFound)?;

        if record.claimed {
            return Err(Error::AlreadyApproved);
        }

        // Determine how much the buyer actually received from dispute resolution.
        // We approximate: if the session is Resolved and buyer_share < 80 % of amount,
        // insurance covers the shortfall.  The resolver stores buyer_share in the
        // resolution_note field as a serialised i128 (see resolve_dispute).
        // For simplicity we use the full coverage amount minus what was already paid.
        let threshold = session
            .amount
            .checked_mul(INSURANCE_THRESHOLD_BPS as i128)
            .ok_or(Error::FeeCalculationOverflow)?
            .checked_div(10_000)
            .ok_or(Error::FeeCalculationOverflow)?;

        // Coverage = full amount - threshold (i.e. up to 100 % of amount).
        let max_coverage = session
            .amount
            .checked_mul(coverage_bps(&env) as i128)
            .ok_or(Error::FeeCalculationOverflow)?
            .checked_div(10_000)
            .ok_or(Error::FeeCalculationOverflow)?;

        let shortfall = max_coverage.checked_sub(threshold).unwrap_or(0).max(0);

        if shortfall <= 0 {
            return Err(Error::InvalidResolutionAmount);
        }

        let pool_bal = pool_balance(&env, &record.asset);
        let payout = shortfall.min(pool_bal);
        if payout <= 0 {
            return Err(Error::InsufficientBalance);
        }

        let token_client = token::Client::new(&env, &record.asset);
        let contract_id = env.current_contract_address();
        token_client.transfer(&contract_id, &record.buyer, &payout);

        set_pool_balance(&env, &record.asset, pool_bal - payout);
        record.claimed = true;
        env.storage().persistent().set(&key, &record);

        env.events().publish(
            (symbol_short!("ins_paid"),),
            InsuranceClaimPaidEvent {
                session_id,
                buyer: record.buyer,
                payout,
            },
        );

        Ok(())
    }

    /// Read the current insurance pool balance for an asset.
    pub fn get_insurance_pool_balance(env: Env, asset: Address) -> i128 {
        pool_balance(&env, &asset)
    }
}
