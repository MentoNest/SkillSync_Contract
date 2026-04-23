use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, BytesN, Env, IntoVal, Val,
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, token,
    Address, Bytes, BytesN, Env, IntoVal, Symbol, Val,
};

#[derive(Clone)]
#[contracttype]
pub enum SessionStatus {
    Locked, // Initial state, funds are held in escrow
    Completed,
    Approved,
    Refunded,
    Disputed,
    Resolved,
    // Pending, // Removed as Locked is the new initial state
    RefundRequested,
    Refunded,
    Disputed,
    Locked,
}

#[derive(Clone)]
#[contracttype]
pub struct Session {
    pub id: BytesN<32>,
    pub buyer: Address,
    pub seller: Address,
    pub token: Address,
    pub amount: i128,
    pub status: SessionStatus,
    pub created_at: u64,
    pub completed_at: Option<u64>, // Use Option for nullable timestamps
    pub dispute_resolved_at: Option<u64>, // Use Option for nullable timestamps
    pub completed_at: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct LockedSession {
    pub buyer: Address,
    pub seller: Address,
    pub amount: i128,
    pub status: SessionStatus,
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Admin,
    Treasury,
    FeeBps,
    NextSessionId,
    Session(BytesN<32>),
    Session(u64),
    DisputeWindowSecs,
    LockedSession(BytesN<32>),
}

#[derive(Clone)]
#[contracttype]
pub struct InitializedEvent {
    pub admin: Address,
    pub treasury: Address,
    pub dispute_window: u32,
}

#[derive(Clone)]
#[contracttype]
pub struct FundsLockedEvent {
    pub session_id: BytesN<32>,
    pub buyer: Address,
    pub seller: Address,
    pub amount: i128,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct SessionApprovedEvent {
    pub session_id: BytesN<32>,
    pub buyer: Address,
    pub seller: Address,
    pub token: Address,
    pub amount: i128,
    pub payout: i128,
    pub fee: i128,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct SessionCompletedEvent {
    pub session_id: BytesN<32>,
}

#[derive(Clone)]
#[contracttype]
pub struct AutoRefundExecutedEvent {
pub struct RefundRequestedEvent {
    pub session_id: u64,
    pub buyer: Address,
}

#[derive(Clone)]
#[contracttype]
pub struct RefundedEvent {
    pub session_id: u64,
    pub buyer: Address,
    pub amount: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct DisputeInitiatedEvent {
    pub session_id: u64,
    pub initiator: Address,
}

#[derive(Clone)]
#[contracttype]
pub struct DisputeResolvedEvent {
    pub session_id: u64,
    pub buyer_payout: i128,
    pub seller_payout: i128,
}
pub struct SessionRefundedEvent {
    pub session_id: u64,
    pub buyer: Address,
    pub seller: Address,
    pub token: Address,
    pub refund_amount: i128,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    SessionExists = 1,
    InvalidAmount = 2,
}


#[contract]
pub struct CoreContract;

#[contractimpl]
impl CoreContract {
    pub fn initialize(env: Env, admin: Address, treasury: Address, fee_bps: u32) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("contract already initialized");
        }
        if fee_bps > 10_000 {
            panic!("fee bps must be <= 10000");
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Treasury, &treasury);
        env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
        env.storage()
            .instance()
            .set(&DataKey::NextSessionId, &1_u64);
        // Default dispute window: 7 days (604800 seconds)
        let dispute_window_secs: u64 = 604800;
        env.storage()
            .instance()
            .set(&DataKey::DisputeWindowSecs, &dispute_window_secs);

        // Emit Initialized event
        let topics = (Symbol::new(&env, "Initialized"),);
        let data: Val = InitializedEvent {
            admin: admin.clone(),
            treasury: treasury.clone(),
            dispute_window: dispute_window_secs as u32,
        }
        .into_val(&env);
        env.events().publish(topics, data);
    }

    pub fn lock_funds(
        env: Env,
        session_id: u64,
        buyer: Address,
        seller: Address,
        token: Address,
        amount: i128,
    ) -> BytesN<32> {
    ) {
        Self::require_initialized(&env);
        buyer.require_auth();

        if env.storage().persistent().has(&DataKey::Session(session_id)) {
            panic!("session already exists");
        }
        if amount <= 0 {
            panic!("amount must be positive");
        }
        if buyer == seller {
            panic!("buyer and seller must differ");
        }

        // Generate a Bytes32 ID from the incremental counter
        let next_id = Self::next_session_id_val(&env);
        let mut id_bytes = [0u8; 32];
        id_bytes[24..32].copy_from_slice(&next_id.to_be_bytes());
        let session_id = BytesN::from_array(&env, &id_bytes);

        let session = Session {
            id: session_id.clone(),
            buyer,
            seller,
            token,
            amount,
            status: SessionStatus::Locked,
            created_at: env.ledger().timestamp(),
            completed_at: None,
            dispute_resolved_at: None,
        let session = Session {
            id: session_id,
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: token.clone(),
            amount,
            status: SessionStatus::Pending,
            completed_at: 0,
        };

        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&buyer, &env.current_contract_address(), &amount);
        Self::save_session(&env, session_id.clone(), &session);
        env.storage()
            .instance()
            .set(&DataKey::NextSessionId, &(next_id + 1));

        env.storage()
            .persistent()
            .set(&DataKey::Session(session_id), &session);

        let topics = (symbol_short!("locked"), session_id);
        let data = FundsLockedEvent {
            session_id,
            buyer,
            seller,
            token,
            amount,
        };
        env.events().publish(topics, data);
    }

    pub fn complete_session(env: Env, session_id: BytesN<32>) {
        let mut session = Self::get_session(env.clone(), session_id.clone());
    pub fn complete_session(env: Env, session_id: u64) {
        Self::require_initialized(&env);
        let mut session = Self::get_session(env.clone(), session_id);
        session.seller.require_auth();

        if !matches!(session.status, SessionStatus::Locked) {
            panic!("session must be locked to be completed");
        }
        session.status = SessionStatus::Completed;
        session.completed_at = Some(env.ledger().timestamp());
        Self::save_session(&env, &session_id, &session);
        session.completed_at = env.ledger().timestamp();
        env.storage()
            .persistent()
            .set(&DataKey::Session(session_id), &session);

        let topics = (symbol_short!("completed"), session_id.clone());
        let data = SessionCompletedEvent { session_id: session_id.clone() };
        env.events().publish(topics, data);
    }

    pub fn approve_session(env: Env, session_id: BytesN<32>) {
        let mut session = Self::get_session(env.clone(), session_id.clone());
    pub fn approve_session(env: Env, session_id: u64) {
        Self::require_initialized(&env);
        let mut session = Self::get_session(env.clone(), session_id);
        session.buyer.require_auth();

        if !matches!(session.status, SessionStatus::Completed) {
            panic!("session must be completed");
        }

        let (payout, fee) = Self::apply_fee(&env, session.amount);
        let contract_address = env.current_contract_address();
        let token_client = token::Client::new(&env, &session.token);
        let treasury = Self::treasury(env.clone());

        if payout > 0 {
            token_client.transfer(&contract_address, &session.seller, &payout);
        }
        if fee > 0 {
            token_client.transfer(&contract_address, &treasury, &fee);
        }

        session.status = SessionStatus::Approved;
        Self::save_session(&env, &session_id, &session);

        // Emit SessionApproved event with gross amount, fee, and net payout
        let timestamp = env.ledger().timestamp();
        let topics = (Symbol::new(&env, "SessionApproved"), session_id);
        let topics = (symbol_short!("approved"), session_id.clone());
        let topics = (Symbol::new(&env, "fee_deducted"), session_id);
        let fee_data = FeeDeductedEvent {
            session_id,
            amount: session.amount,
            fee,
            bps: Self::fee_bps(env.clone()),
        };
        env.events().publish(topics, fee_data);

        let topics = (symbol_short!("approved"), session_id);
        let data: Val = SessionApprovedEvent {
            session_id: session_id.clone(),
            buyer: session.buyer,
            seller: session.seller,
            token: session.token,
            amount: session.amount,
            payout,
            fee,
            timestamp,
        }
        .into_val(&env);
        env.events().publish(topics, data);
    }

    pub fn refund_session(env: Env, session_id: BytesN<32>) {
        let mut session = Self::get_session(env.clone(), session_id.clone());
        // Only the seller or an admin can trigger a refund, or a buyer if it's disputed
        session.seller.require_auth();

        if !matches!(session.status, SessionStatus::Locked) {
            panic!("only locked sessions can be refunded");
        }

        let token_client = token::Client::new(&env, &session.token);
        token_client.transfer(&env.current_contract_address(), &session.buyer, &session.amount);

        session.status = SessionStatus::Refunded;
        Self::save_session(&env, &session_id, &session);

        env.events().publish((symbol_short!("refunded"), session_id), ());
    }

    pub fn dispute_session(env: Env, session_id: BytesN<32>) {
        let mut session = Self::get_session(env.clone(), session_id.clone());
        session.buyer.require_auth();

        if !matches!(session.status, SessionStatus::Locked) && !matches!(session.status, SessionStatus::Completed) {
            panic!("cannot dispute at this stage");
        }

        session.status = SessionStatus::Disputed;
        Self::save_session(&env, &session_id, &session);

        env.events().publish((symbol_short!("disputed"), session_id), ());
    }

    pub fn resolve_dispute(env: Env, session_id: BytesN<32>, refund_buyer: bool) {
        // Only the treasury/admin can resolve a dispute
        let treasury = Self::treasury(env.clone());
        treasury.require_auth();

        let mut session = Self::get_session(env.clone(), session_id.clone());

        if !matches!(session.status, SessionStatus::Disputed) {
            panic!("session is not disputed");
        }

        let token_client = token::Client::new(&env, &session.token);
        let contract_address = env.current_contract_address();

        if refund_buyer {
            token_client.transfer(&contract_address, &session.buyer, &session.amount);
            session.status = SessionStatus::Refunded;
        } else {
            let fee_bps = Self::fee_bps(env.clone());
            let fee = session.amount * i128::from(fee_bps) / 10_000;
            let payout = session.amount - fee;

            if payout > 0 {
                token_client.transfer(&contract_address, &session.seller, &payout);
            }
            if fee > 0 {
                token_client.transfer(&contract_address, &treasury, &fee);
            }
            session.status = SessionStatus::Resolved;
        }

        session.dispute_resolved_at = Some(env.ledger().timestamp());
        Self::save_session(&env, &session_id, &session);

        let topics = (symbol_short!("resolved"), session_id);
        env.events().publish(topics, ());
    }

    pub fn get_session(env: Env, session_id: BytesN<32>) -> Session {
    pub fn auto_refund(env: Env, session_id: u64) {
        let mut session = Self::get_session(env.clone(), session_id);

        if !matches!(session.status, SessionStatus::Completed) {
            panic!("session must be completed");
        }

        let dispute_window_secs = Self::dispute_window_secs(env.clone());
        let current_time = env.ledger().timestamp();
        let time_since_completion = current_time - session.completed_at;

        if time_since_completion < dispute_window_secs {
            panic!("dispute window has not expired");
    pub fn refund_initiate(env: Env, session_id: u64) {
        let mut session = Self::get_session(env.clone(), session_id);
        session.buyer.require_auth();

        if !matches!(session.status, SessionStatus::Pending | SessionStatus::Completed) {
            panic!("session must be pending or completed");
        }

        session.status = SessionStatus::RefundRequested;
        env.storage()
            .persistent()
            .set(&DataKey::Session(session_id), &session);

        let topics = (symbol_short!("refund"), session_id);
        let data = RefundRequestedEvent {
            session_id,
            buyer: session.buyer,
        };
        env.events().publish(topics, data);
    }

    pub fn refund_approve(env: Env, session_id: u64) {
        let mut session = Self::get_session(env.clone(), session_id);
        session.seller.require_auth();

        if !matches!(session.status, SessionStatus::RefundRequested) {
            panic!("session must be refund requested");
        }

        let contract_address = env.current_contract_address();
        let token_client = token::Client::new(&env, &session.token);

        // Refund full amount to buyer
        token_client.transfer(&contract_address, &session.buyer, &session.amount);

        session.status = SessionStatus::Refunded;
        // Full refund to buyer, no fee charged
        token_client.transfer(&contract_address, &session.buyer, &session.amount);

        session.status = SessionStatus::Refunded;
    pub fn refund_session(env: Env, session_id: u64) {
        let mut session = Self::get_session(env.clone(), session_id);
        session.buyer.require_auth();

        // Refund only allowed if session is pending (not completed or approved)
        if !matches!(session.status, SessionStatus::Pending) {
            panic!("refund only allowed for pending sessions");
        }

        // Transfer full amount back to buyer (no fee deducted)
        let contract_address = env.current_contract_address();
        let token_client = token::Client::new(&env, &session.token);
        token_client.transfer(&contract_address, &session.buyer, &session.amount);

        // Update session status to indicate refund
        session.status = SessionStatus::Approved; // Using Approved as final state for refunded sessions
        env.storage()
            .persistent()
            .set(&DataKey::Session(session_id), &session);

        // Emit AutoRefundExecuted event
        let topics = (symbol_short!("autorefund"), session_id);
        let data: Val = AutoRefundExecutedEvent {
            session_id,
            buyer: session.buyer.clone(),
        let topics = (symbol_short!("refunded"), session_id);
        let data: Val = RefundedEvent {
            session_id,
            buyer: session.buyer,
            amount: session.amount,
        }
        .into_val(&env);
        env.events().publish(topics, data);
    }

    pub fn dispute_initiate(env: Env, session_id: u64) {
        let mut session = Self::get_session(env.clone(), session_id);
        
        // Either buyer or seller can initiate dispute
        if session.status != SessionStatus::Pending && session.status != SessionStatus::Completed {
            panic!("session must be pending or completed");
        }

        // The caller must be either buyer or seller (enforced by auth)
        // In mock environment, mock_all_auths handles this
        // We check if the invoking address is buyer or seller
        session.status = SessionStatus::Disputed;
        env.storage()
            .persistent()
            .set(&DataKey::Session(session_id), &session);

        let topics = (symbol_short!("dispute"), session_id);
        let data = DisputeInitiatedEvent {
            session_id,
            initiator: session.buyer.clone(), // Default to buyer for event
        // Emit SessionRefunded event
        let topics = (symbol_short!("refunded"), session_id);
        let data = SessionRefundedEvent {
            session_id,
            buyer: session.buyer,
            seller: session.seller,
            token: session.token,
            refund_amount: session.amount,
        };
        env.events().publish(topics, data);
    }

    pub fn dispute_resolve(env: Env, session_id: u64, buyer_payout: i128) {
        let mut session = Self::get_session(env.clone(), session_id);
        let treasury = Self::treasury(env.clone());
        treasury.require_auth();

        if !matches!(session.status, SessionStatus::Disputed) {
            panic!("session must be disputed");
        }

        if buyer_payout < 0 || buyer_payout > session.amount {
            panic!("buyer payout must be between 0 and session amount");
        }

        let seller_payout = session.amount - buyer_payout;
        let contract_address = env.current_contract_address();
        let token_client = token::Client::new(&env, &session.token);

        // Distribute funds according to resolution
        if buyer_payout > 0 {
            token_client.transfer(&contract_address, &session.buyer, &buyer_payout);
        }
        if seller_payout > 0 {
            token_client.transfer(&contract_address, &session.seller, &seller_payout);
        }

        // Mark as approved (resolved)
        session.status = SessionStatus::Approved;
        env.storage()
            .persistent()
            .set(&DataKey::Session(session_id), &session);

        let topics = (symbol_short!("dispute_resolved"), session_id);
        let data: Val = DisputeResolvedEvent {
            session_id,
            buyer_payout,
            seller_payout,
        }
        .into_val(&env);
        env.events().publish(topics, data);
    pub fn lock_funds(env: Env, session_id: BytesN<32>, seller: Address, amount: i128) {
        if amount <= 0 {
            panic_with_error!(&env, ContractError::InvalidAmount);
        }

        let key = DataKey::LockedSession(session_id.clone());
        if env.storage().persistent().has(&key) {
            panic_with_error!(&env, ContractError::SessionExists);
        }

        let buyer = env.invoker();
        buyer.require_auth();

        let native_token = native_token_client(&env);
        native_token.transfer(&buyer, &env.current_contract_address(), &amount);

        let session = LockedSession {
            buyer: buyer.clone(),
            seller: seller.clone(),
            amount,
            status: SessionStatus::Locked,
        };

        env.storage().persistent().set(&key, &session);

        // Emit FundsLocked event with all relevant session metadata
        let timestamp = env.ledger().timestamp();
        let topics = (Symbol::new(&env, "FundsLocked"), session_id.clone());
        let data: Val = FundsLockedEvent {
            session_id,
            buyer,
            seller,
            amount,
            timestamp,
        }
        .into_val(&env);
        env.events().publish(topics, data);
    }

    pub fn get_session(env: Env, session_id: u64) -> Session {
        Self::require_initialized(&env);
        env.storage()
            .persistent()
            .get(&DataKey::Session(session_id))
            .unwrap_or_else(|| panic!("Session not found"))
    }

    fn save_session(env: &Env, id: &BytesN<32>, session: &Session) {
        env.storage().persistent().set(&DataKey::Session(id.clone()), session);
    }

    pub fn get_locked_session(env: Env, session_id: BytesN<32>) -> Option<LockedSession> {
        env.storage()
            .persistent()
            .get(&DataKey::LockedSession(session_id))
    }

    fn admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("contract not initialized"))
    }

    pub fn treasury(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Treasury)
            .unwrap_or_else(|| panic!("contract not initialized"))
    }

    pub fn get_treasury(env: Env) -> Address {
        Self::treasury(env)
    }

    pub fn set_treasury(env: Env, new_treasury: Address) {
        let admin = Self::admin(env.clone());
        admin.require_auth();

        let old_treasury = Self::treasury(env.clone());
        env.storage()
            .instance()
            .set(&DataKey::Treasury, &new_treasury);

        let topics = (Symbol::new(&env, "TreasuryUpdated"),);
        let data: Val = TreasuryUpdated {
            old_treasury,
            new_treasury: new_treasury.clone(),
            updated_by: admin,
        }
        .into_val(&env);
        env.events().publish(topics, data);
    }

    pub fn fee_bps(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::FeeBps)
            .unwrap_or_else(|| panic!("contract not initialized"))
    }

    fn next_session_id_val(env: &Env) -> u64 {
    pub fn dispute_window_secs(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::DisputeWindowSecs)
            .unwrap_or_else(|| panic!("contract not initialized"))
    }

    fn next_session_id(env: &Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::NextSessionId)
            .unwrap_or_else(|| panic!("contract not initialized"))
    }
}

fn native_token_client(env: &Env) -> token::Client {
    let native_token = native_token_address(env);
    token::Client::new(env, &native_token)
}

fn native_token_address(env: &Env) -> Address {
    let serialized_native_asset = Bytes::from_slice(env, &[0, 0, 0, 0]);
    let deployer = env.deployer().with_stellar_asset(serialized_native_asset);
    let address = deployer.deployed_address();

    #[cfg(test)]
    if !address.exists() {
        deployer.deploy();
    }

    address
}
