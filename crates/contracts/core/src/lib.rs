#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, Address, Env, Symbol,
};

pub const DISPUTE_WINDOW_MIN_SECONDS: u64 = 60;
pub const DISPUTE_WINDOW_MAX_SECONDS: u64 = 30 * 24 * 60 * 60;
pub const DEFAULT_DISPUTE_WINDOW_SECONDS: u64 = 24 * 60 * 60;

#[contract]
pub struct SkillSyncContract;

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    DisputeWindow,
    Treasury,
    PlatformFee,
    Version,
}

const VERSION: u32 = 1;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidDisputeWindow = 3,
    Unauthorized = 4,
    InvalidTreasuryAddress = 5,
}

#[contractimpl]
impl SkillSyncContract {
    pub fn init(
        env: Env,
        admin: Address,
        platform_fee_bps: u32,
        treasury_address: Address,
        dispute_window_secs: u64,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::PlatformFee, &platform_fee_bps);
        env.storage()
            .instance()
            .set(&DataKey::Treasury, &treasury_address);
        env.storage()
            .instance()
            .set(&DataKey::DisputeWindow, &dispute_window_secs);
        env.storage().instance().set(&DataKey::Version, &VERSION);

        env.events().publish(
            (Symbol::new(&env, "Initialized"),),
            (
                admin,
                platform_fee_bps,
                treasury_address,
                dispute_window_secs,
                VERSION,
            ),
        );

        Ok(())
    }

    pub fn ping(_env: Env) -> u32 {
        1
    }

    pub fn get_dispute_window(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::DisputeWindow)
            .unwrap_or(DEFAULT_DISPUTE_WINDOW_SECONDS)
    }

    pub fn get_treasury(env: Env) -> Address {
        // Return stored treasury address if present, otherwise fall back to admin.
        match env.storage().instance().get(&DataKey::Treasury) {
            Some(addr) => addr,
            None => match read_admin(&env) {
                Ok(admin) => admin,
                Err(_) => panic_with_error!(&env, Error::NotInitialized),
            },
        }
    }

    pub fn set_dispute_window(env: Env, seconds: u64) -> Result<(), Error> {
        let admin = read_admin(&env)?;
        admin.require_auth();

        validate_dispute_window(seconds)?;

        let old = Self::get_dispute_window(env.clone());
        env.storage()
            .instance()
            .set(&DataKey::DisputeWindow, &seconds);
        env.events()
            .publish((Symbol::new(&env, "DisputeWindowUpdated"),), (old, seconds));
        Ok(())
    }

    pub fn set_treasury(env: Env, new_addr: Address) -> Result<(), Error> {
        let admin = read_admin(&env)?;
        admin.require_auth();

        let old = match env.storage().instance().get(&DataKey::Treasury) {
            Some(addr) => addr,
            None => read_admin(&env)?,
        };

        env.storage().instance().set(&DataKey::Treasury, &new_addr);
        env.events()
            .publish((Symbol::new(&env, "TreasuryUpdated"),), (old, new_addr));
        Ok(())
    }
}

fn read_admin(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotInitialized)
}

fn validate_dispute_window(seconds: u64) -> Result<(), Error> {
    if !(DISPUTE_WINDOW_MIN_SECONDS..=DISPUTE_WINDOW_MAX_SECONDS).contains(&seconds) {
        return Err(Error::InvalidDisputeWindow);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events},
        vec, Address, Env, IntoVal,
    };

    #[test]
    fn test_ping() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        assert_eq!(client.ping(), 1);
    }

    #[test]
    fn test_get_and_set_dispute_window_persists() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);
        assert_eq!(client.get_dispute_window(), DEFAULT_DISPUTE_WINDOW_SECONDS);

        let updated = 120_u64;
        client.set_dispute_window(&updated);
        assert_eq!(client.get_dispute_window(), updated);
    }

    #[test]
    fn test_set_dispute_window_below_min_reverts() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);
        let result = client.try_set_dispute_window(&(DISPUTE_WINDOW_MIN_SECONDS - 1));
        assert_eq!(result, Err(Ok(Error::InvalidDisputeWindow)));
    }

    #[test]
    fn test_set_dispute_window_above_max_reverts() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);
        let result = client.try_set_dispute_window(&(DISPUTE_WINDOW_MAX_SECONDS + 1));
        assert_eq!(result, Err(Ok(Error::InvalidDisputeWindow)));
    }

    #[test]
    fn test_set_dispute_window_requires_admin_auth() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);
        client.set_dispute_window(&120_u64);

        let auths = env.auths();
        assert_eq!(auths.len(), 1);
        assert_eq!(auths[0].0, admin);
    }

    #[test]
    fn test_set_dispute_window_emits_event_with_old_and_new() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        let old = DEFAULT_DISPUTE_WINDOW_SECONDS;
        let new = 600_u64;
        client.set_dispute_window(&new);

        assert_eq!(
            env.events().all(),
            vec![
                &env,
                (
                    contract_id.clone(),
                    (Symbol::new(&env, "Initialized"),).into_val(&env),
                    (admin, 100_u32, treasury, DEFAULT_DISPUTE_WINDOW_SECONDS, VERSION).into_val(&env)
                ),
                (
                    contract_id.clone(),
                    (Symbol::new(&env, "DisputeWindowUpdated"),).into_val(&env),
                    (old, new).into_val(&env)
                )
            ]
        );
    }

    #[test]
    fn test_get_and_set_treasury_persists() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);
        // Initially treasury should default to stored treasury
        assert_eq!(client.get_treasury(), treasury);

        let new_treasury = Address::generate(&env);
        client.set_treasury(&new_treasury);
        assert_eq!(client.get_treasury(), new_treasury);
    }

    #[test]
    fn test_set_treasury_requires_admin_auth() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);
        let new_treasury = Address::generate(&env);
        client.set_treasury(&new_treasury);

        let auths = env.auths();
        assert_eq!(auths.len(), 1);
        assert_eq!(auths[0].0, admin);
    }

    #[test]
    fn test_set_treasury_emits_event_with_old_and_new() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        let old = treasury.clone();
        let new = Address::generate(&env);
        client.set_treasury(&new);

        assert_eq!(
            env.events().all(),
            vec![
                &env,
                (
                    contract_id.clone(),
                    (Symbol::new(&env, "Initialized"),).into_val(&env),
                    (admin, 100_u32, treasury.clone(), DEFAULT_DISPUTE_WINDOW_SECONDS, VERSION).into_val(&env)
                ),
                (
                    contract_id.clone(),
                    (Symbol::new(&env, "TreasuryUpdated"),).into_val(&env),
                    (old, new).into_val(&env)
                )
            ]
        );
    }

    #[test]
    fn test_init_stores_correct_values_and_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let platform_fee_bps = 250_u32;
        let dispute_window = 3600_u64;

        client.init(&admin, &platform_fee_bps, &treasury, &dispute_window);

        // Verify stored values. For admin/fee/version, there are no getters yet,
        // but getting dispute_window and treasury verifies they are stored correctly.
        assert_eq!(client.get_dispute_window(), dispute_window);
        assert_eq!(client.get_treasury(), treasury);

        // Verify event emitted
        let events = env.events().all();
        // Event should be the Initialized event
        assert_eq!(
            events,
            vec![
                &env,
                (
                    contract_id,
                    (Symbol::new(&env, "Initialized"),).into_val(&env),
                    (admin, platform_fee_bps, treasury, dispute_window, VERSION).into_val(&env)
                )
            ]
        );
    }

    #[test]
    fn test_init_twice_fails() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SkillSyncContract);
        let client = SkillSyncContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);

        // Second init should revert
        let result = client.try_init(&admin, &100, &treasury, &DEFAULT_DISPUTE_WINDOW_SECONDS);
        assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
    }
}
