use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, token, Address,
    Bytes, BytesN, Env, Symbol,
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionStatus {
    Locked,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowSession {
    pub buyer: Address,
    pub seller: Address,
    pub amount: i128,
    pub status: SessionStatus,
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Session(BytesN<32>),
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
    pub fn lock_funds(env: Env, session_id: BytesN<32>, seller: Address, amount: i128) {
        if amount <= 0 {
            panic_with_error!(&env, ContractError::InvalidAmount);
        }

        let key = DataKey::Session(session_id.clone());
        if env.storage().persistent().has(&key) {
            panic_with_error!(&env, ContractError::SessionExists);
        }

        let buyer = env.invoker();
        buyer.require_auth();

        let native_token = native_token_client(&env);
        native_token.transfer(&buyer, &env.current_contract_address(), &amount);

        let session = EscrowSession {
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

    pub fn get_session(env: Env, session_id: BytesN<32>) -> Option<EscrowSession> {
        env.storage()
            .persistent()
            .get(&DataKey::Session(session_id))
    }
}

fn native_token_client(env: &Env) -> token::TokenClient {
    let native_token = native_token_address(env);
    token::TokenClient::new(env, &native_token)
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
