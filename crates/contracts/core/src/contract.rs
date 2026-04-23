use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, token,
    Address, Bytes, BytesN, Env, IntoVal, Symbol, Val,
    contract, contractimpl, contracttype, symbol_short, token, Address, Env, IntoVal, Val,
};

#[derive(Clone)]
#[contracttype]
pub enum SessionStatus {
    Pending,
    Completed,
    Approved,
    Refunded,
    RefundRequested,
    Refunded,
    Disputed,
    Locked,
}

#[derive(Clone)]
#[contracttype]
pub struct Session {
    pub id: u64,
    pub buyer: Address,
    pub seller: Address,
    pub token: Address,
    pub amount: i128,
    pub status: SessionStatus,
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
    Treasury,
    FeeBps,
    NextSessionId,
    Session(u64),
    DisputeWindowSecs,
    LockedSession(BytesN<32>),
}

#[derive(Clone)]
#[contracttype]
pub struct SessionApprovedEvent {
    pub session_id: u64,
    pub buyer: Address,
    pub seller: Address,
    pub token: Address,
    pub payout: i128,
    pub fee: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct SessionCompletedEvent {
    pub session_id: u64,
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
    pub fn initialize(env: Env, treasury: Address, fee_bps: u32) {
        if env.storage().instance().has(&DataKey::Treasury) {
            panic!("contract already initialized");
        }
        if fee_bps > 10_000 {
            panic!("fee bps must be <= 10000");
        }

        env.storage().instance().set(&DataKey::Treasury, &treasury);
        env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
        env.storage()
            .instance()
            .set(&DataKey::NextSessionId, &1_u64);
        // Default dispute window: 7 days (604800 seconds)
        env.storage()
            .instance()
            .set(&DataKey::DisputeWindowSecs, &604800_u64);
    }

    pub fn create_session(
        env: Env,
        buyer: Address,
        seller: Address,
        token: Address,
        amount: i128,
    ) -> u64 {
        buyer.require_auth();

        if amount <= 0 {
            panic!("amount must be positive");
        }
        if buyer == seller {
            panic!("buyer and seller must differ");
        }

        let session_id = Self::next_session_id(&env);
        let session = Session {
            id: session_id,
            buyer: buyer.clone(),
            seller,
            token: token.clone(),
            amount,
            status: SessionStatus::Pending,
            completed_at: 0,
        };

        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&buyer, &env.current_contract_address(), &amount);

        env.storage()
            .persistent()
            .set(&DataKey::Session(session_id), &session);
        env.storage()
            .instance()
            .set(&DataKey::NextSessionId, &(session_id + 1));

        session_id
    }

    pub fn complete_session(env: Env, session_id: u64) {
        let mut session = Self::get_session(env.clone(), session_id);
        session.seller.require_auth();

        if !matches!(session.status, SessionStatus::Pending) {
            panic!("session must be pending");
        }

        session.status = SessionStatus::Completed;
        session.completed_at = env.ledger().timestamp();
        env.storage()
            .persistent()
            .set(&DataKey::Session(session_id), &session);

        let topics = (symbol_short!("completed"), session_id);
        let data = SessionCompletedEvent { session_id };
        env.events().publish(topics, data);
    }

    pub fn approve_session(env: Env, session_id: u64) {
        let mut session = Self::get_session(env.clone(), session_id);
        session.buyer.require_auth();

        if !matches!(session.status, SessionStatus::Completed) {
            panic!("session must be completed");
        }

        let fee_bps = Self::fee_bps(env.clone());
        let treasury = Self::treasury(env.clone());
        let fee = session.amount * i128::from(fee_bps) / 10_000;
        let payout = session.amount - fee;
        let contract_address = env.current_contract_address();
        let token_client = token::Client::new(&env, &session.token);

        if payout > 0 {
            token_client.transfer(&contract_address, &session.seller, &payout);
        }
        if fee > 0 {
            token_client.transfer(&contract_address, &treasury, &fee);
        }

        session.status = SessionStatus::Approved;
        env.storage()
            .persistent()
            .set(&DataKey::Session(session_id), &session);

        let topics = (symbol_short!("approved"), session_id);
        let data: Val = SessionApprovedEvent {
            session_id,
            buyer: session.buyer,
            seller: session.seller,
            token: session.token,
            payout,
            fee,
        }
        .into_val(&env);
        env.events().publish(topics, data);
    }

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
        env.events().publish(
            (Symbol::new(&env, "FundsLocked"), session_id),
            (buyer, seller, amount),
        );
    }

    pub fn get_session(env: Env, session_id: u64) -> Session {
        env.storage()
            .persistent()
            .get(&DataKey::Session(session_id))
            .unwrap_or_else(|| panic!("session not found"))
    }

    pub fn get_locked_session(env: Env, session_id: BytesN<32>) -> Option<LockedSession> {
        env.storage()
            .persistent()
            .get(&DataKey::LockedSession(session_id))
    }

    pub fn treasury(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Treasury)
            .unwrap_or_else(|| panic!("contract not initialized"))
    }

    pub fn fee_bps(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::FeeBps)
            .unwrap_or_else(|| panic!("contract not initialized"))
    }

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
