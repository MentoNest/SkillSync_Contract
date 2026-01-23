#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Symbol, Vec,
};

mod test;

const ADMIN_KEY: Symbol = symbol_short!("ADMIN");
const REGISTRY_KEYS: Symbol = symbol_short!("RKEYS");

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Registry(Symbol),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryUpdatedEvent {
    pub name: Symbol,
    pub addr: Address,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum RegistryError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    NotFound = 3,
}

#[contract]
pub struct RegistryContract;

#[contractimpl]
impl RegistryContract {
    /// Initialize the registry with an admin address.
    pub fn init(env: Env, admin: Address) -> Result<(), RegistryError> {
        if env.storage().instance().has(&ADMIN_KEY) {
            return Err(RegistryError::AlreadyInitialized);
        }

        env.storage().instance().set(&ADMIN_KEY, &admin);
        Ok(())
    }

    /// Set or update a registry pointer (admin-only).
    pub fn set(env: Env, name: Symbol, addr: Address) -> Result<(), RegistryError> {
        let admin = read_admin(&env)?;
        admin.require_auth();

        let key = DataKey::Registry(name.clone());
        let is_new = !env.storage().persistent().has(&key);

        env.storage().persistent().set(&key, &addr);

        if is_new {
            let mut keys = read_registry_keys(&env);
            keys.push_back(name.clone());
            env.storage().instance().set(&REGISTRY_KEYS, &keys);
        }

        env.events().publish(
            (Symbol::new(&env, "RegistryUpdated"),),
            RegistryUpdatedEvent { name, addr },
        );

        Ok(())
    }

    /// Get the address for a registry name.
    pub fn get(env: Env, name: Symbol) -> Result<Address, RegistryError> {
        let key = DataKey::Registry(name);
        env.storage()
            .persistent()
            .get(&key)
            .ok_or(RegistryError::NotFound)
    }

    /// Return all registry entries in insertion order.
    pub fn all(env: Env) -> Vec<(Symbol, Address)> {
        let keys = read_registry_keys(&env);
        let mut entries = Vec::new(&env);

        for name in keys.iter() {
            let key = DataKey::Registry(name.clone());
            if let Some(addr) = env.storage().persistent().get(&key) {
                entries.push_back((name, addr));
            }
        }

        entries
    }

    /// Get the current admin address.
    pub fn get_admin(env: Env) -> Result<Address, RegistryError> {
        read_admin(&env)
    }
}

fn read_admin(env: &Env) -> Result<Address, RegistryError> {
    env.storage()
        .instance()
        .get(&ADMIN_KEY)
        .ok_or(RegistryError::NotInitialized)
}

fn read_registry_keys(env: &Env) -> Vec<Symbol> {
    env.storage()
        .instance()
        .get(&REGISTRY_KEYS)
        .unwrap_or(Vec::new(env))
}
