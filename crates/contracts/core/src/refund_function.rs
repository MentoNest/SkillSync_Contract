#![cfg()]

use soroban_sdk::{symbol_short, vec, Env};


fn refund() {
    let env = Env::default();
    let contract_id = env.register_contract(None, CoreContract);
    let result: Vec<Symbol> = env.invoke_contract(
        &contract_id,
        &symbol_short!("hello"),
        vec![&env, symbol_short!("World")],
    );
    assert_eq!(result, vec![&env, symbol_short!("Hello"), symbol_short!("World")]);

    
#[contractimpl]
impl CoreContract {
    pub fn hello(env: Env, to: Symbol) -> Vec<Symbol> {
        vec![&env, symbol_short!("Refund"), to]
    }
}
}