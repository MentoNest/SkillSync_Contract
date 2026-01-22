#![cfg_attr(not(feature = "std"), no_std)]

use ink::storage::Mapping;

/// Session completion gate contract for SkillSync escrow system
/// 
/// This contract records session completion status and provides authorization
/// for escrow payout. Only authorized admin can mark sessions as completed.
#[ink::contract]
mod session_gate {
    use super::*;

    /// Event emitted when a session is marked as completed
    #[ink(event)]
    pub struct SessionCompleted {
        #[ink(topic)]
        booking_id: AccountId,
    }

    /// Storage for the SessionGate contract
    #[ink(storage)]
    pub struct SessionGate {
        /// Mapping of booking_id to completion status
        completed: Mapping<AccountId, bool>,
        /// Admin account that can mark sessions as completed
        admin: AccountId,
    }

    impl SessionGate {
        /// Creates a new SessionGate contract
        /// 
        /// # Arguments
        /// * `admin` - The admin account that can mark sessions as completed
        #[ink(constructor)]
        pub fn new(admin: AccountId) -> Self {
            let mut instance = Self {
                completed: Mapping::default(),
                admin,
            };
            
            // Initialize admin's completion status to false (not completed)
            instance.completed.insert(&admin, &false);
            
            instance
        }

        /// Marks a booking as completed (admin only)
        /// 
        /// # Arguments
        /// * `booking_id` - The booking ID to mark as completed
        /// 
        /// # Requirements
        /// - Only the admin can call this function
        /// - The booking must not already be completed
        #[ink(message)]
        pub fn set_completed(&mut self, booking_id: AccountId) -> Result<(), SessionGateError> {
            // Check if caller is admin
            if self.env().caller() != self.admin {
                return Err(SessionGateError::Unauthorized);
            }

            // Check if already completed
            if self.is_completed(booking_id) {
                return Err(SessionGateError::AlreadyCompleted);
            }

            // Mark as completed
            self.completed.insert(&booking_id, &true);

            // Emit event
            self.env().emit_event(SessionCompleted { booking_id });

            Ok(())
        }

        /// Checks if a booking has been completed
        /// 
        /// # Arguments
        /// * `booking_id` - The booking ID to check
        /// 
        /// # Returns
        /// * `bool` - True if completed, false otherwise
        #[ink(message)]
        pub fn is_completed(&self, booking_id: AccountId) -> bool {
            self.completed.get(&booking_id).unwrap_or(false)
        }

        /// Returns the admin account
        #[ink(message)]
        pub fn admin(&self) -> AccountId {
            self.admin
        }

        /// Transfers admin rights to a new account (admin only)
        /// 
        /// # Arguments
        /// * `new_admin` - The new admin account
        #[ink(message)]
        pub fn transfer_admin(&mut self, new_admin: AccountId) -> Result<(), SessionGateError> {
            if self.env().caller() != self.admin {
                return Err(SessionGateError::Unauthorized);
            }

            self.admin = new_admin;
            Ok(())
        }
    }

    /// Custom errors for the SessionGate contract
    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum SessionGateError {
        /// Caller is not authorized to perform this action
        Unauthorized,
        /// Booking has already been marked as completed
        AlreadyCompleted,
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use ink::env::test::{default_accounts, set_caller, DefaultAccounts};

        fn get_accounts() -> DefaultAccounts<ink::env::DefaultEnvironment> {
            default_accounts::<ink::env::DefaultEnvironment>()
        }

        #[ink::test]
        fn test_new_contract() {
            let accounts = get_accounts();
            let contract = SessionGate::new(accounts.alice);
            assert_eq!(contract.admin(), accounts.alice);
        }

        #[ink::test]
        fn test_set_completed_success() {
            let accounts = get_accounts();
            let mut contract = SessionGate::new(accounts.alice);
            
            // Set caller to admin
            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            
            let booking_id = accounts.bob;
            assert!(!contract.is_completed(booking_id));
            
            assert!(contract.set_completed(booking_id).is_ok());
            assert!(contract.is_completed(booking_id));
        }

        #[ink::test]
        fn test_set_completed_unauthorized() {
            let accounts = get_accounts();
            let mut contract = SessionGate::new(accounts.alice);
            
            // Set caller to non-admin
            set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            
            let booking_id = accounts.charlie;
            let result = contract.set_completed(booking_id);
            
            assert_eq!(result, Err(SessionGateError::Unauthorized));
            assert!(!contract.is_completed(booking_id));
        }

        #[ink::test]
        fn test_set_completed_already_completed() {
            let accounts = get_accounts();
            let mut contract = SessionGate::new(accounts.alice);
            
            // Set caller to admin
            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            
            let booking_id = accounts.bob;
            
            // First completion should succeed
            assert!(contract.set_completed(booking_id).is_ok());
            
            // Second completion should fail
            let result = contract.set_completed(booking_id);
            assert_eq!(result, Err(SessionGateError::AlreadyCompleted));
        }

        #[ink::test]
        fn test_transfer_admin_success() {
            let accounts = get_accounts();
            let mut contract = SessionGate::new(accounts.alice);
            
            // Set caller to admin
            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            
            assert!(contract.transfer_admin(accounts.bob).is_ok());
            assert_eq!(contract.admin(), accounts.bob);
        }

        #[ink::test]
        fn test_transfer_admin_unauthorized() {
            let accounts = get_accounts();
            let mut contract = SessionGate::new(accounts.alice);
            
            // Set caller to non-admin
            set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            
            let result = contract.transfer_admin(accounts.charlie);
            assert_eq!(result, Err(SessionGateError::Unauthorized));
            assert_eq!(contract.admin(), accounts.alice);
        }

        #[ink::test]
        fn test_is_completed_default() {
            let accounts = get_accounts();
            let contract = SessionGate::new(accounts.alice);
            
            // Non-existent booking should return false
            let booking_id = accounts.bob;
            assert!(!contract.is_completed(booking_id));
        }
    }
}
