#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype,
    symbol_short, Address, Bytes, Env, Symbol, Vec,
};

// ============ Storage Keys ============

#[contracttype]
pub enum DataKey {
    Admin,
    NextIdx,
    Entry(u64),
    Writer(Address),
    TopicSeq(Symbol),
    TopicIdx(Symbol, u64),
}

// ============ Data Types ============

#[contracttype]
#[derive(Clone)]
pub struct Entry {
    pub topic: Symbol,
    pub ref_id: u64,
    pub data: Bytes,
    pub ts: u64,
}

// ============ Errors ============

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum AuditError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    EntryNotFound = 4,
}

// ============ Contract ============

#[contract]
pub struct AuditLogContract;

#[contractimpl]
impl AuditLogContract {
    /// Initialize with admin
    pub fn init(env: Env, admin: Address) -> Result<(), AuditError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(AuditError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().persistent().set(&DataKey::NextIdx, &0u64);
        Ok(())
    }

    /// Add authorized writer (admin only)
    pub fn add_writer(env: Env, writer: Address) -> Result<(), AuditError> {
        let admin = Self::get_admin_internal(&env)?;
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Writer(writer), &true);
        Ok(())
    }

    /// Remove writer (admin only)
    pub fn remove_writer(env: Env, writer: Address) -> Result<(), AuditError> {
        let admin = Self::get_admin_internal(&env)?;
        admin.require_auth();
        env.storage().persistent().remove(&DataKey::Writer(writer));
        Ok(())
    }

    /// Append entry (authorized writers only)
    pub fn append(
        env: Env,
        caller: Address,
        topic: Symbol,
        ref_id: u64,
        data: Bytes,
    ) -> Result<u64, AuditError> {
        // Check writer is authorized
        if !Self::is_writer_internal(&env, &caller) {
            return Err(AuditError::Unauthorized);
        }
        caller.require_auth();

        // Get next index
        let idx: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::NextIdx)
            .ok_or(AuditError::NotInitialized)?;

        // Create entry
        let entry = Entry {
            topic: topic.clone(),
            ref_id,
            data,
            ts: env.ledger().timestamp(),
        };

        // Store entry
        env.storage().persistent().set(&DataKey::Entry(idx), &entry);

        // Update global next index
        env.storage().persistent().set(&DataKey::NextIdx, &(idx + 1));

        // Update topic index
        let topic_seq: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::TopicSeq(topic.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::TopicIdx(topic.clone(), topic_seq), &idx);
        env.storage()
            .persistent()
            .set(&DataKey::TopicSeq(topic.clone()), &(topic_seq + 1));

        // Emit event
        env.events().publish(
            (symbol_short!("appended"), topic),
            (idx, ref_id),
        );

        Ok(idx)
    }

    /// Get single entry by index
    pub fn get(env: Env, idx: u64) -> Result<Entry, AuditError> {
        env.storage()
            .persistent()
            .get(&DataKey::Entry(idx))
            .ok_or(AuditError::EntryNotFound)
    }

    /// Get range of entries
    pub fn range(env: Env, start: u64, limit: u64) -> Vec<Entry> {
        let mut result = Vec::new(&env);
        let next: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::NextIdx)
            .unwrap_or(0);

        let end = if start + limit > next { next } else { start + limit };

        let mut i = start;
        while i < end {
            if let Some(entry) = env.storage().persistent().get(&DataKey::Entry(i)) {
                result.push_back(entry);
            }
            i += 1;
        }
        result
    }

    /// Get entries by topic with pagination
    pub fn by_topic(env: Env, topic: Symbol, page: u64, limit: u64) -> Vec<Entry> {
        let mut result = Vec::new(&env);
        let topic_count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::TopicSeq(topic.clone()))
            .unwrap_or(0);

        let start = page * limit;
        let end = if start + limit > topic_count {
            topic_count
        } else {
            start + limit
        };

        let mut seq = start;
        while seq < end {
            if let Some(idx) = env
                .storage()
                .persistent()
                .get::<_, u64>(&DataKey::TopicIdx(topic.clone(), seq))
            {
                if let Some(entry) = env.storage().persistent().get(&DataKey::Entry(idx)) {
                    result.push_back(entry);
                }
            }
            seq += 1;
        }
        result
    }

    /// Check if address is writer
    pub fn is_writer(env: Env, addr: Address) -> bool {
        Self::is_writer_internal(&env, &addr)
    }

    /// Get admin
    pub fn get_admin(env: Env) -> Result<Address, AuditError> {
        Self::get_admin_internal(&env)
    }

    /// Get total entry count
    pub fn count(env: Env) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::NextIdx)
            .unwrap_or(0)
    }

    // ============ Internal ============

    fn get_admin_internal(env: &Env) -> Result<Address, AuditError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(AuditError::NotInitialized)
    }

    fn is_writer_internal(env: &Env, addr: &Address) -> bool {
        // Admin is always a writer
        if let Some(admin) = env.storage().instance().get::<_, Address>(&DataKey::Admin) {
            if admin == *addr {
                return true;
            }
        }
        env.storage()
            .persistent()
            .get(&DataKey::Writer(addr.clone()))
            .unwrap_or(false)
    }
}

// ============ Tests ============

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    #[test]
    fn test_append_and_get() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, AuditLogContract);
        let client = AuditLogContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let topic = Symbol::new(&env, "DISPUTE");
        let data = Bytes::from_slice(&env, b"{\"action\":\"opened\"}");

        let idx = client.append(&admin, &topic, &100u64, &data);
        assert_eq!(idx, 0);

        let entry = client.get(&0u64);
        assert_eq!(entry.topic, topic);
        assert_eq!(entry.ref_id, 100);
    }

    #[test]
    fn test_range() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, AuditLogContract);
        let client = AuditLogContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let topic = Symbol::new(&env, "PAYOUT");
        let data = Bytes::from_slice(&env, b"{}");

        // Append 5 entries
        for i in 0..5u64 {
            client.append(&admin, &topic, &i, &data);
        }

        let entries = client.range(&1u64, &3u64);
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn test_by_topic() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, AuditLogContract);
        let client = AuditLogContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let dispute = Symbol::new(&env, "DISPUTE");
        let payout = Symbol::new(&env, "PAYOUT");
        let data = Bytes::from_slice(&env, b"{}");

        // Mix topics
        client.append(&admin, &dispute, &1u64, &data);
        client.append(&admin, &payout, &2u64, &data);
        client.append(&admin, &dispute, &3u64, &data);
        client.append(&admin, &payout, &4u64, &data);
        client.append(&admin, &dispute, &5u64, &data);

        let disputes = client.by_topic(&dispute, &0u64, &10u64);
        assert_eq!(disputes.len(), 3);

        let payouts = client.by_topic(&payout, &0u64, &10u64);
        assert_eq!(payouts.len(), 2);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #3)")]
    fn test_unauthorized_append() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, AuditLogContract);
        let client = AuditLogContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let random = Address::generate(&env);
        client.init(&admin);

        let topic = Symbol::new(&env, "DISPUTE");
        let data = Bytes::from_slice(&env, b"{}");

        // Should fail - random is not a writer
        client.append(&random, &topic, &1u64, &data);
    }

    #[test]
    fn test_add_remove_writer() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, AuditLogContract);
        let client = AuditLogContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let writer = Address::generate(&env);
        client.init(&admin);

        // Add writer
        client.add_writer(&writer);
        assert!(client.is_writer(&writer));

        // Writer can append
        let topic = Symbol::new(&env, "POLICY");
        let data = Bytes::from_slice(&env, b"{}");
        client.append(&writer, &topic, &1u64, &data);

        // Remove writer
        client.remove_writer(&writer);
        assert!(!client.is_writer(&writer));
    }
}
