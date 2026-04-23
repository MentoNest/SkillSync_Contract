use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, Env, IntoVal, Symbol, Val,
};

#[derive(Clone)]
#[contracttype]
pub enum SessionStatus {
    Pending,
    Completed,
    Approved,
    Refunded,
    Resolved,
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
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Admin,
    Treasury,
    FeeBps,
    NextSessionId,
    Session(u64),
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
pub struct FeeDeductedEvent {
    pub session_id: u64,
    pub amount: i128,
    pub fee: i128,
    pub bps: u32,
}

#[derive(Clone)]
#[contracttype]
pub struct RefundEvent {
    pub session_id: u64,
    pub buyer: Address,
    pub amount: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct DisputeResolvedEvent {
    pub session_id: u64,
    pub buyer_refund: i128,
    pub seller_payout: i128,
    pub fee: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct InitializedEvent {
    pub admin: Address,
    pub treasury: Address,
    pub fee_bps: u32,
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

        let topics = (symbol_short!("init"),);
        let data = InitializedEvent {
            admin: admin.clone(),
            treasury: treasury.clone(),
            fee_bps,
        };
        env.events().publish(topics, data);
    }

    pub fn create_session(
        env: Env,
        buyer: Address,
        seller: Address,
        token: Address,
        amount: i128,
    ) -> u64 {
        Self::require_initialized(&env);
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
        Self::require_initialized(&env);
        let mut session = Self::get_session(env.clone(), session_id);
        session.seller.require_auth();

        if !matches!(session.status, SessionStatus::Pending) {
            panic!("session must be pending");
        }

        session.status = SessionStatus::Completed;
        env.storage()
            .persistent()
            .set(&DataKey::Session(session_id), &session);

        let topics = (symbol_short!("completed"), session_id);
        let data = SessionCompletedEvent { session_id };
        env.events().publish(topics, data);
    }

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
        env.storage()
            .persistent()
            .set(&DataKey::Session(session_id), &session);

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

    pub fn refund_session(env: Env, session_id: u64) {
        Self::require_initialized(&env);
        let mut session = Self::get_session(env.clone(), session_id);
        session.buyer.require_auth();

        let (refund_amount, fee) = match session.status {
            SessionStatus::Pending => {
                // Early refund: no fee deducted
                (session.amount, 0)
            }
            SessionStatus::Completed => {
                // Late refund: fee applies
                Self::apply_fee(&env, session.amount)
            }
            _ => panic!("invalid session status for refund"),
        };

        let contract_address = env.current_contract_address();
        let token_client = token::Client::new(&env, &session.token);

        token_client.transfer(&contract_address, &session.buyer, &refund_amount);
        if fee > 0 {
            let treasury = Self::treasury(env.clone());
            token_client.transfer(&contract_address, &treasury, &fee);
        }

        session.status = SessionStatus::Refunded;
        env.storage()
            .persistent()
            .set(&DataKey::Session(session_id), &session);

        if fee > 0 {
            let topics = (Symbol::new(&env, "fee_deducted"), session_id);
            let fee_data = FeeDeductedEvent {
                session_id,
                amount: session.amount,
                fee,
                bps: Self::fee_bps(env.clone()),
            };
            env.events().publish(topics, fee_data);
        }

        let topics = (symbol_short!("refunded"), session_id);
        let data = RefundEvent {
            session_id,
            buyer: session.buyer,
            amount: refund_amount,
        };
        env.events().publish(topics, data);
    }

    pub fn resolve_dispute(env: Env, session_id: u64, buyer_refund: i128) {
        Self::require_initialized(&env);
        let mut session = Self::get_session(env.clone(), session_id);
        Self::admin(env.clone()).require_auth();

        if !matches!(session.status, SessionStatus::Completed) {
            panic!("session must be completed");
        }
        if buyer_refund < 0 || buyer_refund > session.amount {
            panic!("invalid refund amount");
        }

        let (total_after_fee, total_fee) = Self::apply_fee(&env, session.amount);
        if buyer_refund > total_after_fee {
            panic!("refund exceeds amount after fee");
        }
        let seller_payout = total_after_fee - buyer_refund;

        let contract_address = env.current_contract_address();
        let token_client = token::Client::new(&env, &session.token);
        let treasury = Self::treasury(env.clone());

        if buyer_refund > 0 {
            token_client.transfer(&contract_address, &session.buyer, &buyer_refund);
        }
        if seller_payout > 0 {
            token_client.transfer(&contract_address, &session.seller, &seller_payout);
        }
        if total_fee > 0 {
            token_client.transfer(&contract_address, &treasury, &total_fee);
        }

        session.status = SessionStatus::Resolved;
        env.storage()
            .persistent()
            .set(&DataKey::Session(session_id), &session);

        let topics = (Symbol::new(&env, "fee_deducted"), session_id);
        let fee_data = FeeDeductedEvent {
            session_id,
            amount: session.amount,
            fee: total_fee,
            bps: Self::fee_bps(env.clone()),
        };
        env.events().publish(topics, fee_data);

        let topics = (symbol_short!("resolved"), session_id);
        let data = DisputeResolvedEvent {
            session_id,
            buyer_refund,
            seller_payout,
            fee: total_fee,
        };
        env.events().publish(topics, data);
    }

    pub fn get_session(env: Env, session_id: u64) -> Session {
        Self::require_initialized(&env);
        env.storage()
            .persistent()
            .get(&DataKey::Session(session_id))
            .unwrap_or_else(|| panic!("session not found"))
    }

    pub fn admin(env: Env) -> Address {
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

    pub fn fee_bps(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::FeeBps)
            .unwrap_or_else(|| panic!("contract not initialized"))
    }

    pub fn set_admin(env: Env, new_admin: Address) {
        let current = Self::admin(env.clone());
        current.require_auth();
        env.storage().instance().set(&DataKey::Admin, &new_admin);
    }

    pub fn set_fee_bps(env: Env, fee_bps: u32) {
        Self::admin(env.clone()).require_auth();
        if fee_bps > 10_000 {
            panic!("fee bps must be <= 10000");
        }
        env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
    }

    pub fn set_treasury(env: Env, treasury: Address) {
        Self::admin(env.clone()).require_auth();
        env.storage().instance().set(&DataKey::Treasury, &treasury);
    }

    fn require_initialized(env: &Env) {
        if !env.storage().instance().has(&DataKey::Admin) {
            panic!("contract not initialized");
        }
    }

    fn apply_fee(env: &Env, amount: i128) -> (i128, i128) {
        let fee_bps = Self::fee_bps(env.clone());
        let fee = amount
            .checked_mul(i128::from(fee_bps))
            .unwrap_or_else(|| panic!("fee calculation overflow"))
            / 10_000;
        let after_fee = amount - fee;
        (after_fee, fee)
    }

    fn next_session_id(env: &Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::NextSessionId)
            .unwrap_or_else(|| panic!("contract not initialized"))
    }
}
