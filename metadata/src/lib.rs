use soroban_sdk::{contractimpl, Env, Address, BytesN, Symbol};

pub struct MetadataContract;

/// Contract for storing off-chain metadata per session
#[contractimpl]
impl MetadataContract {
    /// Set metadata URI for a given session
    /// Only buyer or seller can set metadata
    pub fn set_session_metadata(env: Env, session_id: BytesN<32>, metadata_uri: String, caller: Address) {
        // Retrieve buyer and seller for session
        let buyer: Address = env.storage().get(&(session_id.clone(), Symbol::short("buyer"))).unwrap();
        let seller: Address = env.storage().get(&(session_id.clone(), Symbol::short("seller"))).unwrap();

        if caller != buyer && caller != seller {
            panic!("Only buyer or seller can set metadata");
        }

        // Store metadata URI in instance storage (not persistent)
        env.storage().set_temp(&(session_id.clone(), Symbol::short("metadata_uri")), &metadata_uri);

        // Emit event
        env.events().publish(
            (Symbol::short("MetadataUpdated"), session_id.clone()),
            metadata_uri.clone(),
        );
    }

    /// Get metadata URI for a given session
    pub fn get_session_metadata(env: Env, session_id: BytesN<32>) -> Option<String> {
        env.storage().get_temp(&(session_id, Symbol::short("metadata_uri")))
    }
}
