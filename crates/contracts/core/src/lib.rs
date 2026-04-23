#![no_std]

mod contract;

pub use contract::{
    CoreContract, CoreContractClient, Session, SessionApprovedEvent, SessionCompletedEvent,
    SessionRefundedEvent, SessionStatus,
};

#[cfg(test)]
mod test;

#![no_std]

mod contract;

pub use contract::{
    CoreContract, CoreContractClient, Session, SessionApprovedEvent, SessionCompletedEvent,
    SessionRefundedEvent, SessionStatus,
};

#[cfg(test)]
mod test;


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



fn setup() -> (
    Env,
    CoreContractClient<'static>,
    TokenClient<'static>,
    StellarAssetClient<'static>,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token_address = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_address);
    let asset_client = StellarAssetClient::new(&env, &token_address);

    asset_client.mint(&buyer, &1_000);

    let contract_id = env.register_contract(None, CoreContract);
    let contract = CoreContractClient::new(&env, &contract_id);
    contract.initialize(&treasury, &500);

    (
        env,
        contract,
        token_client,
        asset_client,
        buyer,
        seller,
        treasury,
        contract_id,
    )
}



