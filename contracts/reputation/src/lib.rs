#![cfg_attr(not(feature = "std"), no_std)]

use ink::storage::Mapping;

/// Reputation scoring contract for SkillSync
///
/// Maintains on-chain reputation scores for mentors and mentees,
/// updated after session completion and reviews.
#[ink::contract]
mod reputation {
    use super::*;

    /// Event emitted when reputation score is updated
    #[ink(event)]
    pub struct ReputationUpdated {
        #[ink(topic)]
        addr: AccountId,
        new_score: i64,
        reason: String,
    }

    /// Storage for the Reputation contract
    #[ink(storage)]
    pub struct Reputation {
        /// Mapping of address to reputation score
        scores: Mapping<AccountId, i64>,
        /// Admin account that can update scores
        admin: AccountId,
        /// Score increment for completed session
        session_bonus: i64,
    }

    /// Custom errors for the Reputation contract
    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum ReputationError {
        /// Caller is not authorized (admin-only operation)
        Unauthorized,
    }

    impl Reputation {
        /// Creates a new Reputation contract
        ///
        /// # Arguments
        /// * `admin` - The admin account that can update scores
        /// * `session_bonus` - Score increment for completed session
        #[ink(constructor)]
        pub fn new(admin: AccountId, session_bonus: i64) -> Self {
            Self {
                scores: Mapping::default(),
                admin,
                session_bonus,
            }
        }

        /// Bumps reputation score after session completion for both mentor and mentee
        ///
        /// Only admin can call this.
        #[ink(message)]
        pub fn bump_after_session(&mut self, mentor: AccountId, mentee: AccountId) -> Result<(), ReputationError> {
            if self.env().caller() != self.admin {
                return Err(ReputationError::Unauthorized);
            }

            // Bump mentor score
            let mentor_score = self.scores.get(mentor).unwrap_or(0);
            let new_mentor_score = mentor_score.saturating_add(self.session_bonus);
            self.scores.insert(mentor, &new_mentor_score);
            self.env().emit_event(ReputationUpdated {
                addr: mentor,
                new_score: new_mentor_score,
                reason: "session_completion".to_string(),
            });

            // Bump mentee score
            let mentee_score = self.scores.get(mentee).unwrap_or(0);
            let new_mentee_score = mentee_score.saturating_add(self.session_bonus);
            self.scores.insert(mentee, &new_mentee_score);
            self.env().emit_event(ReputationUpdated {
                addr: mentee,
                new_score: new_mentee_score,
                reason: "session_completion".to_string(),
            });

            Ok(())
        }

        /// Applies review rating to mentor's reputation score
        ///
        /// Only admin can call this.
        #[ink(message)]
        pub fn apply_review(&mut self, mentor: AccountId, rating: i32) -> Result<(), ReputationError> {
            if self.env().caller() != self.admin {
                return Err(ReputationError::Unauthorized);
            }

            let current_score = self.scores.get(mentor).unwrap_or(0);
            let new_score = current_score.saturating_add(rating as i64);
            self.scores.insert(mentor, &new_score);
            self.env().emit_event(ReputationUpdated {
                addr: mentor,
                new_score,
                reason: "review".to_string(),
            });

            Ok(())
        }

        /// Gets the reputation score for an address
        #[ink(message)]
        pub fn get(&self, addr: AccountId) -> i64 {
            self.scores.get(addr).unwrap_or(0)
        }

        /// Gets the admin address
        #[ink(message)]
        pub fn admin(&self) -> AccountId {
            self.admin
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn default_accounts() -> ink::env::test::DefaultAccounts<ink::env::DefaultEnvironment> {
            ink::env::test::default_accounts::<ink::env::DefaultEnvironment>()
        }

        fn set_caller(caller: AccountId) {
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(caller);
        }

        #[ink::test]
        fn test_bump_after_session() {
            let accounts = default_accounts();
            let mut reputation = Reputation::new(accounts.alice, 10);

            // Set caller to admin
            set_caller(accounts.alice);

            let mentor = accounts.bob;
            let mentee = accounts.charlie;

            // Initial scores should be 0
            assert_eq!(reputation.get(mentor), 0);
            assert_eq!(reputation.get(mentee), 0);

            // Bump after session
            reputation.bump_after_session(mentor, mentee).unwrap();

            // Scores should be 10
            assert_eq!(reputation.get(mentor), 10);
            assert_eq!(reputation.get(mentee), 10);
        }

        #[ink::test]
        fn test_apply_review() {
            let accounts = default_accounts();
            let mut reputation = Reputation::new(accounts.alice, 10);

            set_caller(accounts.alice);

            let mentor = accounts.bob;

            // Initial score 0
            assert_eq!(reputation.get(mentor), 0);

            // Apply positive review
            reputation.apply_review(mentor, 5).unwrap();
            assert_eq!(reputation.get(mentor), 5);

            // Apply negative review
            reputation.apply_review(mentor, -3).unwrap();
            assert_eq!(reputation.get(mentor), 2);
        }

        #[ink::test]
        fn test_unauthorized() {
            let accounts = default_accounts();
            let mut reputation = Reputation::new(accounts.alice, 10);

            // Set caller to non-admin
            set_caller(accounts.bob);

            let result = reputation.bump_after_session(accounts.charlie, accounts.django);
            assert_eq!(result, Err(ReputationError::Unauthorized));

            let result = reputation.apply_review(accounts.charlie, 5);
            assert_eq!(result, Err(ReputationError::Unauthorized));
        }

        #[ink::test]
        fn test_no_overflow() {
            let accounts = default_accounts();
            let mut reputation = Reputation::new(accounts.alice, 10);

            set_caller(accounts.alice);

            let addr = accounts.bob;

            // Set score to max
            reputation.scores.insert(addr, &i64::MAX);

            // Apply review with positive rating, should not overflow
            reputation.apply_review(addr, 1).unwrap();
            assert_eq!(reputation.get(addr), i64::MAX);

            // Set to min
            reputation.scores.insert(addr, &i64::MIN);

            // Apply negative review, should not underflow
            reputation.apply_review(addr, -1).unwrap();
            assert_eq!(reputation.get(addr), i64::MIN);
        }

        #[ink::test]
        fn test_events_emitted() {
            let accounts = default_accounts();
            let mut reputation = Reputation::new(accounts.alice, 10);

            set_caller(accounts.alice);

            let mentor = accounts.bob;
            let mentee = accounts.charlie;

            // Bump after session should emit two events
            reputation.bump_after_session(mentor, mentee).unwrap();

            // Check events
            let events = ink::env::test::recorded_events().collect::<Vec<_>>();
            assert_eq!(events.len(), 2);
        }

        #[ink::test]
        fn test_apply_review_event() {
            let accounts = default_accounts();
            let mut reputation = Reputation::new(accounts.alice, 10);

            set_caller(accounts.alice);

            let mentor = accounts.bob;

            reputation.apply_review(mentor, 5).unwrap();

            let events = ink::env::test::recorded_events().collect::<Vec<_>>();
            assert_eq!(events.len(), 1);
        }

        #[ink::test]
        fn test_get_nonexistent() {
            let accounts = default_accounts();
            let reputation = Reputation::new(accounts.alice, 10);

            let addr = accounts.bob;
            assert_eq!(reputation.get(addr), 0);
        }

        #[ink::test]
        fn test_admin_function() {
            let accounts = default_accounts();
            let reputation = Reputation::new(accounts.alice, 10);

            assert_eq!(reputation.admin(), accounts.alice);
        }

        #[ink::test]
        fn test_multiple_sessions() {
            let accounts = default_accounts();
            let mut reputation = Reputation::new(accounts.alice, 5);

            set_caller(accounts.alice);

            let mentor = accounts.bob;
            let mentee = accounts.charlie;

            // First session
            reputation.bump_after_session(mentor, mentee).unwrap();
            assert_eq!(reputation.get(mentor), 5);
            assert_eq!(reputation.get(mentee), 5);

            // Second session
            reputation.bump_after_session(mentor, mentee).unwrap();
            assert_eq!(reputation.get(mentor), 10);
            assert_eq!(reputation.get(mentee), 10);
        }

        #[ink::test]
        fn test_session_and_review() {
            let accounts = default_accounts();
            let mut reputation = Reputation::new(accounts.alice, 10);

            set_caller(accounts.alice);

            let mentor = accounts.bob;

            // Session bump
            reputation.bump_after_session(mentor, accounts.charlie).unwrap();
            assert_eq!(reputation.get(mentor), 10);

            // Positive review
            reputation.apply_review(mentor, 5).unwrap();
            assert_eq!(reputation.get(mentor), 15);

            // Negative review
            reputation.apply_review(mentor, -3).unwrap();
            assert_eq!(reputation.get(mentor), 12);
        }

        #[ink::test]
        fn test_zero_rating() {
            let accounts = default_accounts();
            let mut reputation = Reputation::new(accounts.alice, 10);

            set_caller(accounts.alice);

            let mentor = accounts.bob;

            reputation.apply_review(mentor, 0).unwrap();
            assert_eq!(reputation.get(mentor), 0);
        }

        #[ink::test]
        fn test_large_rating() {
            let accounts = default_accounts();
            let mut reputation = Reputation::new(accounts.alice, 10);

            set_caller(accounts.alice);

            let mentor = accounts.bob;

            reputation.apply_review(mentor, i32::MAX).unwrap();
            assert_eq!(reputation.get(mentor), i32::MAX as i64);
        }

        #[ink::test]
        fn test_negative_rating() {
            let accounts = default_accounts();
            let mut reputation = Reputation::new(accounts.alice, 10);

            set_caller(accounts.alice);

            let mentor = accounts.bob;

            reputation.apply_review(mentor, -10).unwrap();
            assert_eq!(reputation.get(mentor), -10);
        }

        #[ink::test]
        fn test_different_callers_unauthorized() {
            let accounts = default_accounts();
            let mut reputation = Reputation::new(accounts.alice, 10);

            // Test with different non-admin callers
            set_caller(accounts.bob);
            assert_eq!(reputation.bump_after_session(accounts.charlie, accounts.django), Err(ReputationError::Unauthorized));

            set_caller(accounts.charlie);
            assert_eq!(reputation.apply_review(accounts.bob, 5), Err(ReputationError::Unauthorized));
        }

        #[ink::test]
        fn test_constructor() {
            let accounts = default_accounts();
            let reputation = Reputation::new(accounts.alice, 15);

            assert_eq!(reputation.admin(), accounts.alice);
            assert_eq!(reputation.get(accounts.bob), 0); // score should be 0 initially
        }

        #[ink::test]
        fn test_zero_session_bonus() {
            let accounts = default_accounts();
            let mut reputation = Reputation::new(accounts.alice, 0);

            set_caller(accounts.alice);

            let mentor = accounts.bob;
            let mentee = accounts.charlie;

            reputation.bump_after_session(mentor, mentee).unwrap();
            assert_eq!(reputation.get(mentor), 0);
            assert_eq!(reputation.get(mentee), 0);
        }
    }
}