#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, BytesN, Env, Symbol, Vec};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    WasmHash,
}
mod contract;

pub use contract::{
<<<<<<< main
    ContractError, CoreContract, CoreContractClient, LockedSession, Session,
    SessionApprovedEvent, SessionCompletedEvent, SessionStatus,
=======
    CoreContract, CoreContractClient, Session, SessionApprovedEvent, SessionCompletedEvent,
    SessionStatus,
>>>>>>> main
};

#[contractimpl]
impl CoreContract {
    pub fn init(env: Env, admin: Address) {
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
    }

    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        let old_wasm_hash: BytesN<32> = env
            .storage()
            .instance()
            .get(&DataKey::WasmHash)
            .unwrap_or(BytesN::from_array(&env, &[0; 32]));

        env.deployer().update_current_contract_wasm(new_wasm_hash.clone());
        env.storage().instance().set(&DataKey::WasmHash, &new_wasm_hash);
        
        env.events().publish(
            (Symbol::new(&env, "ContractUpgraded"),),
            (old_wasm_hash, new_wasm_hash),
        );
    }

    pub fn hello(env: Env, to: Symbol) -> Vec<Symbol> {
        let mut vec = Vec::new(&env);
        vec.push_back(symbol_short!("Hello"));
        vec.push_back(to);
        vec
    }
}
#[cfg(test)]
mod test;
