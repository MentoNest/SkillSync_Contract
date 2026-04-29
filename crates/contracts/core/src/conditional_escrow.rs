/// Conditional escrow module — issue #213
///
/// Escrow release is gated on the state of an external Soroban contract.
/// Anyone may call `release_if_condition_met` once the external contract
/// returns `true` for the configured selector.  If the condition is not met
/// within `condition_timeout_ledgers`, the buyer may reclaim via
/// `refund_conditional_failed`.
use soroban_sdk::{contracttype, symbol_short, token, Address, Bytes, Env};

use crate::{DataKey, Error, Session, SessionStatus, SkillSyncContract};

// ── Storage key ───────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug)]
pub struct ConditionalConfig {
    /// External contract whose state gates the release.
    pub condition_contract: Address,
    /// Function selector (symbol name) to call on the external contract.
    pub condition_selector: soroban_sdk::Symbol,
    /// Ledger number after which the condition is considered failed.
    pub timeout_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug)]
pub enum ConditionalKey {
    Config(Bytes),
}

// ── Events ────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug)]
pub struct ConditionMetEvent {
    pub session_id: Bytes,
    pub released_to: Address,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct ConditionFailedRefundEvent {
    pub session_id: Bytes,
    pub buyer: Address,
    pub amount: i128,
}

// ── Implementation ────────────────────────────────────────────────────────────

impl SkillSyncContract {
    /// Lock funds with a condition.  The session is created in `Locked` state;
    /// release only happens when `check_condition` returns `true`.
    pub fn lock_funds_conditional(
        env: Env,
        session_id: Bytes,
        payer: Address,
        payee: Address,
        asset: Address,
        amount: i128,
        condition_contract: Address,
        condition_selector: soroban_sdk::Symbol,
        condition_timeout_ledgers: u32,
    ) -> Result<(), Error> {
        Self::require_not_paused(&env)?;
        crate::acquire_lock(&env)?;

        crate::validate_session_id(&session_id)?;
        crate::validate_amount(amount)?;
        crate::validate_different_addresses(&payer, &payee)?;

        let fee_bps = Self::get_platform_fee(env.clone());
        let now = env.ledger().timestamp();
        let dispute_window = Self::get_dispute_window(env.clone());

        let platform_fee = amount
            .checked_mul(fee_bps as i128)
            .ok_or(Error::FeeCalculationOverflow)?
            .checked_div(10_000)
            .ok_or(Error::FeeCalculationOverflow)?;

        let total = amount
            .checked_add(platform_fee)
            .ok_or(Error::FeeCalculationOverflow)?;

        let token_client = token::Client::new(&env, &asset);
        if token_client.balance(&payer) < total {
            crate::release_lock(&env);
            return Err(Error::InsufficientBalance);
        }

        let timeout_ledger = env
            .ledger()
            .sequence()
            .checked_add(condition_timeout_ledgers)
            .ok_or(Error::FeeCalculationOverflow)?;

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

        let config = ConditionalConfig {
            condition_contract,
            condition_selector,
            timeout_ledger,
        };
        env.storage()
            .persistent()
            .set(&ConditionalKey::Config(session_id.clone()), &config);

        env.events().publish(
            (symbol_short!("cond_lock"),),
            (session_id, payer, payee, amount),
        );

        crate::release_lock(&env);
        Ok(())
    }

    /// Check whether the external condition is met for a session.
    ///
    /// Calls `condition_selector()` on the external contract and expects a
    /// `bool` return value.
    pub fn check_condition(env: Env, session_id: Bytes) -> Result<bool, Error> {
        let config: ConditionalConfig = env
            .storage()
            .persistent()
            .get(&ConditionalKey::Config(session_id.clone()))
            .ok_or(Error::SessionNotFound)?;

        let result: bool = env.invoke_contract(
            &config.condition_contract,
            &config.condition_selector,
            soroban_sdk::vec![&env],
        );
        Ok(result)
    }

    /// Anyone may call this to release funds once the condition is met.
    pub fn release_if_condition_met(env: Env, session_id: Bytes) -> Result<(), Error> {
        Self::require_not_paused(&env)?;

        let config: ConditionalConfig = env
            .storage()
            .persistent()
            .get(&ConditionalKey::Config(session_id.clone()))
            .ok_or(Error::SessionNotFound)?;

        // Condition must be met before timeout.
        if env.ledger().sequence() > config.timeout_ledger {
            return Err(Error::SessionNotExpired);
        }

        let met: bool = env.invoke_contract(
            &config.condition_contract,
            &config.condition_selector,
            soroban_sdk::vec![&env],
        );
        if !met {
            return Err(Error::InvalidSessionStatus);
        }

        let mut session =
            Self::get_session(env.clone(), session_id.clone()).ok_or(Error::SessionNotFound)?;

        if session.status != SessionStatus::Locked {
            return Err(Error::InvalidSessionStatus);
        }

        let fee = session
            .amount
            .checked_mul(session.fee_bps as i128)
            .ok_or(Error::FeeCalculationOverflow)?
            .checked_div(10_000)
            .ok_or(Error::FeeCalculationOverflow)?;
        let payout = session
            .amount
            .checked_sub(fee)
            .ok_or(Error::FeeCalculationOverflow)?;

        let token_client = token::Client::new(&env, &session.asset);
        let contract_id = env.current_contract_address();
        let treasury = Self::get_treasury(env.clone());

        if payout > 0 {
            token_client.transfer(&contract_id, &session.payee, &payout);
        }
        if fee > 0 {
            token_client.transfer(&contract_id, &treasury, &fee);
        }

        let now = env.ledger().timestamp();
        session.status = SessionStatus::Approved;
        session.updated_at = now;
        session.approved_at = now;

        let key = DataKey::Session(session_id.clone());
        env.storage().persistent().set(&key, &session);
        Self::remove_from_expiry_index(env.clone(), session_id.clone(), session.expires_at)?;
        env.storage()
            .persistent()
            .remove(&ConditionalKey::Config(session_id.clone()));

        env.events().publish(
            (symbol_short!("cond_met"),),
            ConditionMetEvent {
                session_id,
                released_to: session.payee,
                amount: payout,
            },
        );

        Ok(())
    }

    /// Buyer reclaims funds when the condition was not met within the timeout.
    pub fn refund_conditional_failed(env: Env, session_id: Bytes) -> Result<(), Error> {
        Self::require_not_paused(&env)?;

        let config: ConditionalConfig = env
            .storage()
            .persistent()
            .get(&ConditionalKey::Config(session_id.clone()))
            .ok_or(Error::SessionNotFound)?;

        if env.ledger().sequence() <= config.timeout_ledger {
            return Err(Error::DisputeWindowNotElapsed);
        }

        let mut session =
            Self::get_session(env.clone(), session_id.clone()).ok_or(Error::SessionNotFound)?;

        if session.status != SessionStatus::Locked {
            return Err(Error::InvalidSessionStatus);
        }

        let fee = session
            .amount
            .checked_mul(session.fee_bps as i128)
            .ok_or(Error::FeeCalculationOverflow)?
            .checked_div(10_000)
            .ok_or(Error::FeeCalculationOverflow)?;
        let total_locked = session
            .amount
            .checked_add(fee)
            .ok_or(Error::FeeCalculationOverflow)?;

        let token_client = token::Client::new(&env, &session.asset);
        let contract_id = env.current_contract_address();
        token_client.transfer(&contract_id, &session.payer, &total_locked);

        let now = env.ledger().timestamp();
        session.status = SessionStatus::Refunded;
        session.updated_at = now;

        let key = DataKey::Session(session_id.clone());
        env.storage().persistent().set(&key, &session);
        Self::remove_from_expiry_index(env.clone(), session_id.clone(), session.expires_at)?;
        env.storage()
            .persistent()
            .remove(&ConditionalKey::Config(session_id.clone()));

        env.events().publish(
            (symbol_short!("cond_fail"),),
            ConditionFailedRefundEvent {
                session_id,
                buyer: session.payer,
                amount: total_locked,
            },
        );

        Ok(())
    }
}
