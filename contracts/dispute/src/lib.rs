#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, Symbol, String};

#[derive(Clone, PartialEq, Debug)]
#[contracttype]
pub enum DisputeStatus {
    Open,
    Resolved,
}

#[derive(Clone, PartialEq, Debug)]
#[contracttype]
pub enum DisputeOutcome {
    MentorWins,
    MenteeWins,
}

#[derive(Clone)]
#[contracttype]
pub struct Dispute {
    pub status: DisputeStatus,
    pub raiser: Address,
    pub opened_at: u64,
    pub outcome: Option<DisputeOutcome>,
    pub reason: Option<String>,
}

const DISPUTES: Symbol = symbol_short!("DISPUTES");
const ADMIN: Symbol = symbol_short!("ADMIN");

#[contract]
pub struct DisputeContract;

#[contractimpl]
impl DisputeContract {
    /// Initialize contract with admin
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        env.storage().instance().set(&ADMIN, &admin);
    }

    /// Raise a dispute for a booking (freezes escrow)
    pub fn raise(env: Env, booking_id: u64, raiser: Address) {
        raiser.require_auth();

        // Check dispute doesn't already exist
        if Self::get(&env, booking_id).is_some() {
            panic!("Dispute already exists");
        }

        let dispute = Dispute {
            status: DisputeStatus::Open,
            raiser: raiser.clone(),
            opened_at: env.ledger().timestamp(),
            outcome: None,
            reason: None,
        };

        env.storage().persistent().set(&(DISPUTES, booking_id), &dispute);

        // Emit event
        env.events().publish(
            (symbol_short!("DISPUTE"), symbol_short!("OPENED")),
            (booking_id, raiser),
        );
    }

    /// Resolve a dispute (admin only)
    pub fn resolve(env: Env, booking_id: u64, outcome: DisputeOutcome, reason: Option<String>) {
        let admin: Address = env.storage().instance().get(&ADMIN).unwrap();
        admin.require_auth();

        let mut dispute: Dispute = Self::get(&env, booking_id).expect("Dispute not found");

        if dispute.status != DisputeStatus::Open {
            panic!("Dispute already resolved");
        }

        dispute.status = DisputeStatus::Resolved;
        dispute.outcome = Some(outcome.clone());
        dispute.reason = reason;

        env.storage().persistent().set(&(DISPUTES, booking_id), &dispute);

        // Emit event
        env.events().publish(
            (symbol_short!("DISPUTE"), symbol_short!("RESOLVED")),
            (booking_id, outcome),
        );
    }

    /// Get dispute state
    pub fn get(env: &Env, booking_id: u64) -> Option<Dispute> {
        env.storage().persistent().get(&(DISPUTES, booking_id))
    }

    /// Check if booking has active (unresolved) dispute
    pub fn is_frozen(env: &Env, booking_id: u64) -> bool {
        if let Some(dispute) = Self::get(env, booking_id) {
            dispute.status == DisputeStatus::Open
        } else {
            false
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    #[test]
    fn test_raise_dispute() {
        let env = Env::default();
        let contract_id = env.register_contract(None, DisputeContract);
        let client = DisputeContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let mentor = Address::generate(&env);
        let booking_id = 1;

        env.mock_all_auths();

        client.initialize(&admin);
        client.raise(&booking_id, &mentor);

        let dispute = client.get(&booking_id).unwrap();
        assert_eq!(dispute.status, DisputeStatus::Open);
        assert_eq!(dispute.raiser, mentor);
        assert!(client.is_frozen(&booking_id));
    }

    #[test]
    fn test_resolve_dispute_mentor_wins() {
        let env = Env::default();
        let contract_id = env.register_contract(None, DisputeContract);
        let client = DisputeContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let mentor = Address::generate(&env);
        let booking_id = 1;

        env.mock_all_auths();

        client.initialize(&admin);
        client.raise(&booking_id, &mentor);
        client.resolve(&booking_id, &DisputeOutcome::MentorWins, &None);

        let dispute = client.get(&booking_id).unwrap();
        assert_eq!(dispute.status, DisputeStatus::Resolved);
        assert_eq!(dispute.outcome, Some(DisputeOutcome::MentorWins));
        assert!(!client.is_frozen(&booking_id));
    }

    #[test]
    fn test_resolve_dispute_mentee_wins() {
        let env = Env::default();
        let contract_id = env.register_contract(None, DisputeContract);
        let client = DisputeContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let mentee = Address::generate(&env);
        let booking_id = 1;

        env.mock_all_auths();

        client.initialize(&admin);
        client.raise(&booking_id, &mentee);
        client.resolve(&booking_id, &DisputeOutcome::MenteeWins, &Some(String::from_str(&env, "Service not delivered")));

        let dispute = client.get(&booking_id).unwrap();
        assert_eq!(dispute.status, DisputeStatus::Resolved);
        assert_eq!(dispute.outcome, Some(DisputeOutcome::MenteeWins));
    }

    #[test]
    #[should_panic(expected = "Dispute already exists")]
    fn test_cannot_raise_duplicate_dispute() {
        let env = Env::default();
        let contract_id = env.register_contract(None, DisputeContract);
        let client = DisputeContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let mentor = Address::generate(&env);
        let booking_id = 1;

        env.mock_all_auths();

        client.initialize(&admin);
        client.raise(&booking_id, &mentor);
        client.raise(&booking_id, &mentor); // Should panic
    }

    #[test]
    #[should_panic(expected = "Dispute already resolved")]
    fn test_cannot_resolve_twice() {
        let env = Env::default();
        let contract_id = env.register_contract(None, DisputeContract);
        let client = DisputeContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let mentor = Address::generate(&env);
        let booking_id = 1;

        env.mock_all_auths();

        client.initialize(&admin);
        client.raise(&booking_id, &mentor);
        client.resolve(&booking_id, &DisputeOutcome::MentorWins, &None);
        client.resolve(&booking_id, &DisputeOutcome::MenteeWins, &None); // Should panic
    }

    #[test]
    #[should_panic(expected = "Dispute not found")]
    fn test_cannot_resolve_nonexistent_dispute() {
        let env = Env::default();
        let contract_id = env.register_contract(None, DisputeContract);
        let client = DisputeContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let booking_id = 999;

        env.mock_all_auths();

        client.initialize(&admin);
        client.resolve(&booking_id, &DisputeOutcome::MentorWins, &None); // Should panic
    }

    #[test]
    fn test_no_dispute_returns_none() {
        let env = Env::default();
        let contract_id = env.register_contract(None, DisputeContract);
        let client = DisputeContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        env.mock_all_auths();

        client.initialize(&admin);

        assert!(client.get(&999).is_none());
        assert!(!client.is_frozen(&999));
    }
}