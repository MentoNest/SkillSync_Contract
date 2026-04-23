#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, vec, Env, Symbol, Vec, BytesN, Address, 
    Map, Status
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionStatus {
    Created = 0,
    Locked = 1,
    Completed = 2,
    Cancelled = 3,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Session {
    pub id: BytesN<32>,
    pub seller: Address,
    pub buyer: Address,
    pub status: SessionStatus,
    pub created_at: u64,
    pub locked_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub amount: i128,
}

#[contracttype]
pub enum DataKey {
    Session(BytesN<32>),
}

#[contracttype]
pub struct SessionCompleted {
    pub session_id: BytesN<32>,
    pub seller: Address,
    pub buyer: Address,
    pub completed_at: u64,
}

#[contract]
pub struct CoreContract;

#[contractimpl]
impl CoreContract {
    pub fn hello(env: Env, to: Symbol) -> Vec<Symbol> {
        vec![&env, symbol_short!("Hello"), to]
    }

    pub fn complete_session(env: Env, session_id: BytesN<32>) {
        // Get the caller (must be seller)
        let caller = env.invoker();
        
        // Get the session from storage
        let session_key = DataKey::Session(session_id.clone());
        let mut session: Session = env.storage().persistent().get(&session_key)
            .unwrap_or_else(|| panic_with_error!(env, Status::from_error(1001))); // Session not found

        // Verify caller is the seller
        if session.seller != caller {
            panic_with_error!(env, Status::from_error(1002)); // Not authorized
        }

        // Verify session status is Locked
        if session.status != SessionStatus::Locked {
            panic_with_error!(env, Status::from_error(1003)); // Invalid status
        }

        // Update session status and timestamp
        session.status = SessionStatus::Completed;
        session.completed_at = Some(env.ledger().timestamp());

        // Save updated session
        env.storage().persistent().set(&session_key, &session);

        // Emit SessionCompleted event
        env.events().publish(
            (symbol_short!("SessionCompleted"), session_id.clone()),
            SessionCompleted {
                session_id: session_id.clone(),
                seller: session.seller,
                buyer: session.buyer,
                completed_at: env.ledger().timestamp(),
            },
        );
    }

    // Helper function for testing - create a session
    pub fn create_session(env: Env, session_id: BytesN<32>, seller: Address, buyer: Address, amount: i128) {
        let session = Session {
            id: session_id.clone(),
            seller,
            buyer,
            status: SessionStatus::Locked,
            created_at: env.ledger().timestamp(),
            locked_at: Some(env.ledger().timestamp()),
            completed_at: None,
            amount,
        };
        
        let session_key = DataKey::Session(session_id);
        env.storage().persistent().set(&session_key, &session);
    }

    // Helper function for testing - get session
    pub fn get_session(env: Env, session_id: BytesN<32>) -> Session {
        let session_key = DataKey::Session(session_id);
        env.storage().persistent().get(&session_key)
            .unwrap_or_else(|| panic_with_error!(env, Status::from_error(1001)))
    }
}