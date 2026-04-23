#![cfg(test)]

use soroban_sdk::{symbol_short, vec, Env, Address, BytesN, Symbol, testutils::Address as TestAddress};
use crate::{CoreContract, Session, SessionStatus, DataKey};

#[test]
fn test_hello() {
    let env = Env::default();
    let contract_id = env.register_contract(None, CoreContract);
    let result: Vec<Symbol> = env.invoke_contract(
        &contract_id,
        &symbol_short!("hello"),
        vec![&env, symbol_short!("World")],
    );
    assert_eq!(result, vec![&env, symbol_short!("Hello"), symbol_short!("World")]);
}

#[test]
fn test_complete_session_success() {
    let env = Env::default();
    let contract_id = env.register_contract(None, CoreContract);
    
    // Create test addresses
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let session_id = BytesN::from_array(&env, &[1u8; 32]);
    
    // Create a session in Locked status
    env.invoke_contract(
        &contract_id,
        &symbol_short!("create_session"),
        vec![
            &env,
            session_id.clone(),
            seller.clone(),
            buyer.clone(),
            1000i128,
        ],
    );
    
    // Verify session is created and locked
    let session: Session = env.invoke_contract(
        &contract_id,
        &symbol_short!("get_session"),
        vec![&env, session_id.clone()],
    );
    assert_eq!(session.status, SessionStatus::Locked);
    assert_eq!(session.seller, seller);
    assert_eq!(session.buyer, buyer);
    
    // Complete the session as seller
    env.invoke_contract(
        &contract_id,
        &symbol_short!("complete_session"),
        vec![&env, session_id.clone()],
    );
    
    // Verify session is completed
    let completed_session: Session = env.invoke_contract(
        &contract_id,
        &symbol_short!("get_session"),
        vec![&env, session_id.clone()],
    );
    assert_eq!(completed_session.status, SessionStatus::Completed);
    assert!(completed_session.completed_at.is_some());
    
    // Verify event was emitted
    let events = env.events().all();
    assert_eq!(events.len(), 1);
    let event = &events[0];
    assert_eq!(event.topics[0], symbol_short!("SessionCompleted"));
    assert_eq!(event.topics[1], session_id);
}

#[test]
fn test_complete_session_unauthorized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, CoreContract);
    
    // Create test addresses
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let unauthorized_user = Address::generate(&env);
    let session_id = BytesN::from_array(&env, &[2u8; 32]);
    
    // Create a session in Locked status
    env.invoke_contract(
        &contract_id,
        &symbol_short!("create_session"),
        vec![
            &env,
            session_id.clone(),
            seller.clone(),
            buyer.clone(),
            1000i128,
        ],
    );
    
    // Try to complete session as unauthorized user
    let result = env.try_invoke_contract(
        &contract_id,
        &symbol_short!("complete_session"),
        vec![&env, session_id.clone()],
    );
    
    // Should fail with authorization error
    assert!(result.is_err());
}

#[test]
fn test_complete_session_invalid_status() {
    let env = Env::default();
    let contract_id = env.register_contract(None, CoreContract);
    
    // Create test addresses
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let session_id = BytesN::from_array(&env, &[3u8; 32]);
    
    // Create a session in Completed status directly (bypass normal flow)
    let session = Session {
        id: session_id.clone(),
        seller: seller.clone(),
        buyer: buyer.clone(),
        status: SessionStatus::Created, // Not Locked
        created_at: env.ledger().timestamp(),
        locked_at: None,
        completed_at: None,
        amount: 1000i128,
    };
    
    // Manually store the session
    let session_key = DataKey::Session(session_id.clone());
    env.storage().persistent().set(&session_key, &session);
    
    // Try to complete session
    let result = env.try_invoke_contract(
        &contract_id,
        &symbol_short!("complete_session"),
        vec![&env, session_id.clone()],
    );
    
    // Should fail with invalid status error
    assert!(result.is_err());
}

#[test]
fn test_complete_session_not_found() {
    let env = Env::default();
    let contract_id = env.register_contract(None, CoreContract);
    
    let session_id = BytesN::from_array(&env, &[4u8; 32]);
    
    // Try to complete non-existent session
    let result = env.try_invoke_contract(
        &contract_id,
        &symbol_short!("complete_session"),
        vec![&env, session_id.clone()],
    );
    
    // Should fail with session not found error
    assert!(result.is_err());
}