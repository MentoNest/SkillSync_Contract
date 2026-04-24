use soroban_sdk::{contractimpl, Env, Address, BytesN, Symbol};

pub struct MultiTokenSession;

/// Contract supporting Soroban-compliant tokens
#[contractimpl]
impl MultiTokenSession {
    /// Lock funds for a session using a specific token
    pub fn lock_funds(env: Env, session_id: BytesN<32>, token_address: Address, from: Address, amount: i128) {
        // Ensure session has no mixed tokens
        let existing_token: Option<Address> = env.storage().get(&(session_id.clone(), Symbol::short("token")));
        if let Some(t) = existing_token {
            if t != token_address {
                panic!("Mixed token sessions not allowed");
            }
        } else {
            env.storage().set(&(session_id.clone(), Symbol::short("token")), &token_address);
        }

        // Call token contract’s transfer_from
        env.invoke_contract(
            &token_address,
            &Symbol::short("transfer_from"),
            (from.clone(), env.current_contract_address(), amount),
        );

        // Store locked amount
        let current: i128 = env.storage().get(&(session_id.clone(), Symbol::short("locked"))).unwrap_or(0);
        env.storage().set(&(session_id.clone(), Symbol::short("locked")), &(current + amount));
    }

    /// Approve payout for a session
    pub fn approve_session(env: Env, session_id: BytesN<32>, to: Address, amount: i128) {
        let token: Address = env.storage().get(&(session_id.clone(), Symbol::short("token"))).unwrap();

        // Call token contract’s transfer
        env.invoke_contract(
            &token,
            &Symbol::short("transfer"),
            (env.current_contract_address(), to.clone(), amount),
        );

        // Deduct locked amount
        let current: i128 = env.storage().get(&(session_id.clone(), Symbol::short("locked"))).unwrap_or(0);
        env.storage().set(&(session_id.clone(), Symbol::short("locked")), &(current - amount));
    }

    /// Admin sets fee in native or stablecoin
    pub fn set_fee(env: Env, token_address: Address, amount: i128) {
        env.storage().set(&Symbol::short("fee_token"), &token_address);
        env.storage().set(&Symbol::short("fee_amount"), &amount);
    }

    /// Get session token
    pub fn get_session_token(env: Env, session_id: BytesN<32>) -> Option<Address> {
        env.storage().get(&(session_id, Symbol::short("token")))
    }
}
