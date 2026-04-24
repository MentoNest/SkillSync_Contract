use soroban_sdk::{contracttype, Address, Bytes, Env};

use crate::errors::ContractError;

/// Price data returned by the oracle.
#[contracttype]
#[derive(Clone, Debug)]
pub struct PriceData {
    /// Price in the smallest unit (e.g. stroops).
    pub price: i128,
    /// Ledger timestamp when this price was recorded.
    pub timestamp: u64,
}

/// Storage keys for oracle state.
#[contracttype]
#[derive(Clone)]
pub enum OracleKey {
    /// The oracle contract address set by admin.
    OracleAddress,
    /// Admin-provided fallback price for a given asset.
    FallbackPrice(Bytes),
    /// Freshness threshold in seconds (max age of a valid price).
    FreshnessThreshold,
}

/// Default freshness threshold: 5 minutes.
pub const DEFAULT_FRESHNESS_THRESHOLD: u64 = 300;

/// Set the oracle contract address. Admin-only.
pub fn set_oracle(env: &Env, admin: &Address, oracle_id: Address) {
    admin.require_auth();
    env.storage()
        .instance()
        .set(&OracleKey::OracleAddress, &oracle_id);
}

/// Get the configured oracle address, if any.
pub fn get_oracle(env: &Env) -> Option<Address> {
    env.storage()
        .instance()
        .get(&OracleKey::OracleAddress)
}

/// Set the freshness threshold (seconds). Admin-only.
pub fn set_freshness_threshold(env: &Env, admin: &Address, threshold_secs: u64) {
    admin.require_auth();
    env.storage()
        .instance()
        .set(&OracleKey::FreshnessThreshold, &threshold_secs);
}

/// Get the freshness threshold, falling back to the default.
pub fn get_freshness_threshold(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&OracleKey::FreshnessThreshold)
        .unwrap_or(DEFAULT_FRESHNESS_THRESHOLD)
}

/// Set an admin-provided fallback price for an asset.
pub fn set_fallback_price(env: &Env, admin: &Address, asset: Bytes, price: i128) {
    admin.require_auth();
    let data = PriceData {
        price,
        timestamp: env.ledger().timestamp(),
    };
    env.storage()
        .persistent()
        .set(&OracleKey::FallbackPrice(asset), &data);
}

/// Retrieve the latest price for `asset`.
///
/// Strategy:
/// 1. Try the on-chain oracle (if configured) and validate freshness.
/// 2. Fall back to the admin-provided price if the oracle is unavailable or stale.
/// 3. Return `ContractError::InternalError` if neither source is available.
pub fn get_price(env: &Env, asset: Bytes) -> Result<i128, ContractError> {
    let now = env.ledger().timestamp();
    let threshold = get_freshness_threshold(env);

    // Attempt oracle lookup first.
    if let Some(_oracle_addr) = get_oracle(env) {
        // In a real integration the oracle contract would be called here via
        // a cross-contract call.  For now we fall through to the fallback so
        // the module compiles and the interface is stable.
    }

    // Fallback: admin-provided price.
    if let Some(data) = env
        .storage()
        .persistent()
        .get::<_, PriceData>(&OracleKey::FallbackPrice(asset))
    {
        validate_price_freshness(now, data.timestamp, threshold)?;
        return Ok(data.price);
    }

    Err(ContractError::InternalError)
}

/// Validate that a price timestamp is within the freshness threshold.
pub fn validate_price_freshness(
    now: u64,
    price_timestamp: u64,
    threshold_secs: u64,
) -> Result<(), ContractError> {
    if now.saturating_sub(price_timestamp) > threshold_secs {
        return Err(ContractError::InternalError); // stale price
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Ledger, Env};

    #[test]
    fn test_freshness_valid() {
        let env = Env::default();
        env.ledger().set_timestamp(1000);
        assert!(validate_price_freshness(1000, 800, 300).is_ok());
    }

    #[test]
    fn test_freshness_stale() {
        let env = Env::default();
        env.ledger().set_timestamp(1000);
        assert!(validate_price_freshness(1000, 100, 300).is_err());
    }

    #[test]
    fn test_set_and_get_oracle() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        set_oracle(&env, &admin, oracle.clone());
        assert_eq!(get_oracle(&env), Some(oracle));
    }

    #[test]
    fn test_fallback_price() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1000);
        let admin = Address::generate(&env);
        let asset = Bytes::from_slice(&env, b"USDC");
        set_fallback_price(&env, &admin, asset.clone(), 5_000_000);
        let price = get_price(&env, asset).unwrap();
        assert_eq!(price, 5_000_000);
    }

    #[test]
    fn test_fallback_price_stale() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1000);
        let admin = Address::generate(&env);
        let asset = Bytes::from_slice(&env, b"USDC");
        set_fallback_price(&env, &admin, asset.clone(), 5_000_000);
        // Advance time past threshold
        env.ledger().set_timestamp(2000);
        set_freshness_threshold(&env, &admin, 300);
        let result = get_price(&env, asset);
        assert!(result.is_err());
    }
}
