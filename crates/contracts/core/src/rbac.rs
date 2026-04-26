use soroban_sdk::{contracttype, Address, Bytes, Env};

/// Predefined role identifiers.
pub mod roles {
    use soroban_sdk::{Bytes, Env};

    pub fn default_admin(env: &Env) -> Bytes {
        Bytes::from_slice(env, b"DEFAULT_ADMIN_ROLE")
    }
    pub fn fee_manager(env: &Env) -> Bytes {
        Bytes::from_slice(env, b"FEE_MANAGER_ROLE")
    }
    pub fn dispute_resolver(env: &Env) -> Bytes {
        Bytes::from_slice(env, b"DISPUTE_RESOLVER_ROLE")
    }
    pub fn upgrader(env: &Env) -> Bytes {
        Bytes::from_slice(env, b"UPGRADER_ROLE")
    }
}

#[contracttype]
#[derive(Clone)]
pub enum RbacKey {
    /// Stores whether (role, account) pair is active.
    HasRole(Bytes, Address),
}

/// Grants `role` to `account`. Caller must already hold DEFAULT_ADMIN_ROLE.
pub fn grant_role(env: &Env, role: Bytes, account: Address) {
    env.storage()
        .persistent()
        .set(&RbacKey::HasRole(role, account), &true);
}

/// Revokes `role` from `account`.
pub fn revoke_role(env: &Env, role: Bytes, account: Address) {
    env.storage()
        .persistent()
        .remove(&RbacKey::HasRole(role, account));
}

/// Returns `true` when `account` holds `role`.
pub fn has_role(env: &Env, role: Bytes, account: Address) -> bool {
    env.storage()
        .persistent()
        .get::<RbacKey, bool>(&RbacKey::HasRole(role, account))
        .unwrap_or(false)
}

/// Panics if `account` does not hold `role`.
pub fn only_role(env: &Env, role: Bytes, account: Address) {
    if !has_role(env, role, account) {
        panic!("unauthorized: missing required role");
    }
}
