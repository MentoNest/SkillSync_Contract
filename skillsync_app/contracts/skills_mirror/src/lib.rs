#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, Symbol, String, Vec,
};

#[contracttype]
pub enum DataKey {
    Skills(Symbol),      // Maps slug -> name
    SkillIndex(u64),     // Index -> slug, for listing
    Count,               // Total number of skills
    Admin,               // Admin address
}

#[contract]
pub struct SkillsTaxonomy;

#[contractimpl]
impl SkillsTaxonomy {
    /// Initialize the contract with an admin
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Count, &0u64);
    }

    /// Add a skill (admin only)
    pub fn add_skill(env: Env, slug: Symbol, name: String) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        if env.storage().persistent().has(&DataKey::Skills(slug.clone())) {
            panic!("Skill already exists");
        }

        // Store skill
        env.storage().persistent().set(&DataKey::Skills(slug.clone()), &name);

        // Update index
        let mut count: u64 = env.storage().instance().get(&DataKey::Count).unwrap();
        env.storage()
            .persistent()
            .set(&DataKey::SkillIndex(count), &slug);
        count += 1;
        env.storage().instance().set(&DataKey::Count, &count);

        // Emit event
        env.events().publish(
            (symbol_short!("SkilAdded"), slug.clone()),
            name.clone(),
        );
    }

    /// Remove a skill (admin only)
    pub fn remove_skill(env: Env, slug: Symbol) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        if !env.storage().persistent().has(&DataKey::Skills(slug.clone())) {
            panic!("Skill does not exist");
        }

        // Remove skill
        env.storage().persistent().remove(&DataKey::Skills(slug.clone()));

        // Remove from index
        let mut count: u64 = env.storage().instance().get(&DataKey::Count).unwrap();
        let last_index = count - 1;
        let last_slug: Symbol = env
            .storage()
            .persistent()
            .get(&DataKey::SkillIndex(last_index))
            .unwrap();

        // Find the index of the slug to remove
        let mut removed_index: Option<u64> = None;
        for i in 0..count {
            let s: Symbol = env.storage().persistent().get(&DataKey::SkillIndex(i)).unwrap();
            if s == slug {
                removed_index = Some(i);
                break;
            }
        }

        if let Some(idx) = removed_index {
            // Swap last with removed
            if idx != last_index {
                env.storage()
                    .persistent()
                    .set(&DataKey::SkillIndex(idx), &last_slug);
            }
            // Remove last index
            env.storage().persistent().remove(&DataKey::SkillIndex(last_index));
            count -= 1;
            env.storage().instance().set(&DataKey::Count, &count);
        }

        // Emit event
        env.events().publish((symbol_short!("Renamed"), slug.clone()), ());
    }

    /// Rename a skill (admin only)
    pub fn rename_skill(env: Env, slug: Symbol, new_name: String) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        if !env.storage().persistent().has(&DataKey::Skills(slug.clone())) {
            panic!("Skill does not exist");
        }

        env.storage()
            .persistent()
            .set(&DataKey::Skills(slug.clone()), &new_name);

        // Emit event
        env.events()
            .publish((symbol_short!("Renamed"), slug.clone()), new_name);
    }

    /// Get the name of a skill by slug
    pub fn get_skill(env: Env, slug: Symbol) -> Option<String> {
        env.storage().persistent().get(&DataKey::Skills(slug))
    }

    /// List skills with pagination
    pub fn list(env: Env, page: u64, limit: u64) -> Vec<(Symbol, String)> {
        let count: u64 = env.storage().instance().get(&DataKey::Count).unwrap_or(0);
        let mut result: Vec<(Symbol, String)> = Vec::new(&env);

        if count == 0 {
            return result;
        }

        let start = page * limit;
        if start >= count {
            return result;
        }

        let end = core::cmp::min(start + limit, count);

        for i in start..end {
            let slug: Symbol = env.storage().persistent().get(&DataKey::SkillIndex(i)).unwrap();
            let name: String = env.storage().persistent().get(&DataKey::Skills(slug.clone())).unwrap();
            result.push_back((slug, name));
        }

        result
    }
}

mod test;