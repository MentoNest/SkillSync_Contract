use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, BytesN, Env, IntoVal, Val,
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
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Treasury,
    FeeBps,
    NextSessionId,
    Session(BytesN<32>),
}

#[derive(Clone)]
#[contracttype]
pub struct SessionApprovedEvent {
    pub session_id: BytesN<32>,
    pub buyer: Address,
    pub seller: Address,
    pub token: Address,
    pub payout: i128,
    pub fee: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct SessionCompletedEvent {
    pub session_id: BytesN<32>,
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
    }

    pub fn create_session(
        env: Env,
        buyer: Address,
        seller: Address,
        token: Address,
        amount: i128,
    ) -> BytesN<32> {
        buyer.require_auth();

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
        };

        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&buyer, &env.current_contract_address(), &amount);
        Self::save_session(&env, session_id.clone(), &session);
        env.storage()
            .instance()
            .set(&DataKey::NextSessionId, &(next_id + 1));

        session_id
    }

    pub fn complete_session(env: Env, session_id: BytesN<32>) {
        let mut session = Self::get_session(env.clone(), session_id.clone());
        session.seller.require_auth();

        if !matches!(session.status, SessionStatus::Locked) {
            panic!("session must be locked to be completed");
        }
        session.status = SessionStatus::Completed;
        session.completed_at = Some(env.ledger().timestamp());
        Self::save_session(&env, &session_id, &session);

        let topics = (symbol_short!("completed"), session_id.clone());
        let data = SessionCompletedEvent { session_id: session_id.clone() };
        env.events().publish(topics, data);
    }

    pub fn approve_session(env: Env, session_id: BytesN<32>) {
        let mut session = Self::get_session(env.clone(), session_id.clone());
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
        Self::save_session(&env, &session_id, &session);

        let topics = (symbol_short!("approved"), session_id.clone());
        let data: Val = SessionApprovedEvent {
            session_id: session_id.clone(),
            buyer: session.buyer,
            seller: session.seller,
            token: session.token,
            payout,
            fee,
        }
        .into_val(&env);
        env.events().publish(topics, data);
    }

    pub fn get_session(env: Env, session_id: BytesN<32>) -> Session {
        env.storage()
            .persistent()
            .get(&DataKey::Session(session_id))
            .unwrap_or_else(|| panic!("Session not found"))
    }

    fn save_session(env: &Env, id: &BytesN<32>, session: &Session) {
        env.storage().persistent().set(&DataKey::Session(id.clone()), session);
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

    fn next_session_id_val(env: &Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::NextSessionId)
            .unwrap_or_else(|| panic!("contract not initialized"))
    }
}
