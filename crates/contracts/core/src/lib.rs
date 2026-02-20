#![no_std]

use soroban_sdk::{contract, contractimpl, Env, Symbol};

#[contract]
pub struct SkillSyncContract;

#[contractimpl]
impl SkillSyncContract {
    /// Initialize the contract
    pub fn initialize(env: Env, admin: soroban_sdk::Address) {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "admin"), &admin);
    }

    /// Get the admin address
    pub fn get_admin(env: Env) -> Option<soroban_sdk::Address> {
        env.storage().instance().get(&Symbol::new(&env, "admin"))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as TestAddress, Address};

    #[test]
    fn test_initialize() {
        let env = soroban_sdk::Env::default();
        let admin = Address::generate(&env);

        SkillSyncContract::initialize(env.clone(), admin.clone());

        assert_eq!(SkillSyncContract::get_admin(env), Some(admin));
    }
}
