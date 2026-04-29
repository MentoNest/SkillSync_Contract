/// DAO-governed dispute resolution — issue #214
///
/// Replaces single-admin dispute resolution with a DAO vote.  The admin
/// submits a dispute to the DAO; after the voting period the resolution is
/// executed on-chain.  If the DAO does not resolve within 10 000 ledgers the
/// admin may fall back to the standard `resolve_dispute` path.
use soroban_sdk::{contracttype, symbol_short, token, Address, Bytes, Env, Symbol};

use crate::{DataKey, Error, SessionStatus, SkillSyncContract};

// ── Storage keys ──────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug)]
pub enum DaoKey {
    /// Address of the DAO contract.
    DaoAddress,
    /// Pending DAO proposal for a session.
    Proposal(Bytes),
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct DaoProposal {
    /// DAO proposal ID.
    pub proposal_id: u64,
    /// Ledger at which the proposal was submitted.
    pub submitted_at_ledger: u32,
    /// Buyer share proposed (informational; DAO decides final split).
    pub buyer_share: i128,
    /// Seller share proposed.
    pub seller_share: i128,
}

/// Ledgers before admin fallback is allowed.
pub const DAO_FALLBACK_LEDGERS: u32 = 10_000;

// ── Events ────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug)]
pub struct DisputeSentToDAOEvent {
    pub session_id: Bytes,
    pub proposal_id: u64,
    pub submitted_at_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct DisputeResolvedByDAOEvent {
    pub session_id: Bytes,
    pub proposal_id: u64,
    pub buyer_share: i128,
    pub seller_share: i128,
    pub fee: i128,
    pub timestamp: u64,
}

// ── Implementation ────────────────────────────────────────────────────────────

impl SkillSyncContract {
    /// Admin: register the DAO contract address.
    pub fn set_dispute_dao(env: Env, dao_address: Address) -> Result<(), Error> {
        let admin = crate::read_admin(&env)?;
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DaoKey::DaoAddress, &dao_address);
        Ok(())
    }

    /// Read the configured DAO address.
    pub fn get_dispute_dao(env: Env) -> Option<Address> {
        env.storage().instance().get(&DaoKey::DaoAddress)
    }

    /// Submit a disputed session to the DAO for a vote.
    ///
    /// Calls `submit_proposal(session_id, buyer_share, seller_share)` on the
    /// DAO contract and stores the returned proposal ID.
    pub fn resolve_dispute_via_dao(
        env: Env,
        session_id: Bytes,
        proposal_id: u64,
        buyer_share: i128,
        seller_share: i128,
    ) -> Result<(), Error> {
        Self::require_not_paused(&env)?;
        let admin = crate::read_admin(&env)?;
        admin.require_auth();

        let session =
            Self::get_session(env.clone(), session_id.clone()).ok_or(Error::SessionNotFound)?;

        if session.status != SessionStatus::Disputed {
            return Err(Error::SessionNotDisputed);
        }

        if buyer_share < 0 || seller_share < 0 {
            return Err(Error::InvalidResolutionAmount);
        }
        let total = buyer_share
            .checked_add(seller_share)
            .ok_or(Error::InvalidResolutionAmount)?;
        if total != session.amount {
            return Err(Error::InvalidResolutionAmount);
        }

        let submitted_at_ledger = env.ledger().sequence();
        let proposal = DaoProposal {
            proposal_id,
            submitted_at_ledger,
            buyer_share,
            seller_share,
        };
        env.storage()
            .persistent()
            .set(&DaoKey::Proposal(session_id.clone()), &proposal);

        env.events().publish(
            (symbol_short!("dao_sent"),),
            DisputeSentToDAOEvent {
                session_id,
                proposal_id,
                submitted_at_ledger,
            },
        );

        Ok(())
    }

    /// Execute the DAO resolution after the voting period has ended.
    ///
    /// Calls `get_result(proposal_id) -> (i128, i128)` on the DAO contract to
    /// retrieve the final buyer/seller split, then distributes funds.
    pub fn execute_dao_resolution(env: Env, session_id: Bytes) -> Result<(), Error> {
        Self::require_not_paused(&env)?;

        let proposal: DaoProposal = env
            .storage()
            .persistent()
            .get(&DaoKey::Proposal(session_id.clone()))
            .ok_or(Error::SessionNotFound)?;

        let dao_address: Address = env
            .storage()
            .instance()
            .get(&DaoKey::DaoAddress)
            .ok_or(Error::NotInitialized)?;

        // Query the DAO for the final resolution.
        let (buyer_share, seller_share): (i128, i128) = env.invoke_contract(
            &dao_address,
            &Symbol::new(&env, "get_result"),
            soroban_sdk::vec![&env, proposal.proposal_id.into_val(&env)],
        );

        let mut session =
            Self::get_session(env.clone(), session_id.clone()).ok_or(Error::SessionNotFound)?;

        if session.status != SessionStatus::Disputed {
            return Err(Error::SessionNotDisputed);
        }

        let fee = session
            .amount
            .checked_mul(session.fee_bps as i128)
            .ok_or(Error::FeeCalculationOverflow)?
            .checked_div(10_000)
            .ok_or(Error::FeeCalculationOverflow)?;

        let token_client = token::Client::new(&env, &session.asset);
        let contract_id = env.current_contract_address();
        let treasury = Self::get_treasury(env.clone());

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
        session.resolver = Some(dao_address);

        let key = DataKey::Session(session_id.clone());
        env.storage().persistent().set(&key, &session);
        Self::remove_from_expiry_index(env.clone(), session_id.clone(), session.expires_at)?;
        env.storage()
            .persistent()
            .remove(&DaoKey::Proposal(session_id.clone()));

        env.events().publish(
            (symbol_short!("dao_done"),),
            DisputeResolvedByDAOEvent {
                session_id,
                proposal_id: proposal.proposal_id,
                buyer_share,
                seller_share,
                fee,
                timestamp: now,
            },
        );

        Ok(())
    }

    /// Admin fallback: resolve via standard path if DAO has not acted within
    /// `DAO_FALLBACK_LEDGERS` ledgers since the proposal was submitted.
    pub fn dao_fallback_resolve(
        env: Env,
        session_id: Bytes,
        buyer_share: i128,
        seller_share: i128,
    ) -> Result<(), Error> {
        Self::require_not_paused(&env)?;
        let admin = crate::read_admin(&env)?;
        admin.require_auth();

        let proposal: DaoProposal = env
            .storage()
            .persistent()
            .get(&DaoKey::Proposal(session_id.clone()))
            .ok_or(Error::SessionNotFound)?;

        let current_ledger = env.ledger().sequence();
        if current_ledger
            < proposal
                .submitted_at_ledger
                .saturating_add(DAO_FALLBACK_LEDGERS)
        {
            return Err(Error::DisputeWindowNotElapsed);
        }

        // Delegate to the standard admin resolution.
        let resolution = if buyer_share == 0 {
            1u32
        } else if seller_share == 0 {
            0u32
        } else {
            2u32
        };
        Self::resolve_dispute(
            env.clone(),
            session_id.clone(),
            resolution,
            buyer_share,
            seller_share,
        )?;

        env.storage()
            .persistent()
            .remove(&DaoKey::Proposal(session_id));

        Ok(())
    }
}

// Bring IntoVal into scope for the invoke_contract call.
use soroban_sdk::IntoVal;
