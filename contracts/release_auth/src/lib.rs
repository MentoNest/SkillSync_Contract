#![cfg_attr(not(feature = "std"), no_std)]

use ink::storage::Mapping;

/// Payment Release Authorization Contract for SkillSync
/// 
/// Validates release authorization via signatures from approved signers (backend or admin).
/// Implements replay protection via nonce tracking and manages authorized signers list.
#[ink::contract]
mod release_auth {
    use super::*;

    /// Payload for release authorization with booking details
    #[derive(scale::Encode, scale::Decode, Clone, Debug)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub struct ReleasePayload {
        /// Booking ID (unique identifier)
        booking_id: u64,
        /// Mentee account address
        mentee: AccountId,
        /// Mentor account address
        mentor: AccountId,
        /// Amount to release
        amount: Balance,
        /// Token address
        token: AccountId,
        /// Nonce for replay protection
        nonce: u64,
    }

    /// Event emitted when a signer is added to authorized list
    #[ink(event)]
    pub struct SignerAdded {
        #[ink(topic)]
        pubkey: [u8; 32],
    }

    /// Event emitted when a signer is removed from authorized list
    #[ink(event)]
    pub struct SignerRemoved {
        #[ink(topic)]
        pubkey: [u8; 32],
    }

    /// Event emitted when a release is authorized
    #[ink(event)]
    pub struct ReleaseAuthorized {
        #[ink(topic)]
        booking_id: u64,
        #[ink(topic)]
        mentor: AccountId,
        signer: [u8; 32],
    }

    /// Custom errors for the ReleaseAuth contract
    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum ReleaseAuthError {
        /// Caller is not authorized (admin-only operation)
        Unauthorized,
        /// Signature verification failed
        SignatureInvalid,
        /// Nonce has already been used (replay attack)
        NonceAlreadyUsed,
        /// Signer is not in authorized list
        SignerNotFound,
        /// Signer already exists in authorized list
        SignerAlreadyExists,
        /// Payload encoding failed
        EncodingError,
    }

    /// Storage for the ReleaseAuth contract
    #[ink(storage)]
    pub struct ReleaseAuth {
        /// Admin account that can manage signers
        admin: AccountId,
        /// Set of authorized signer public keys
        signers: Mapping<[u8; 32], bool>,
        /// Tracking of used nonces to prevent replay attacks
        used_nonces: Mapping<u64, bool>,
        /// Counter for total signers for enumeration support
        signer_count: u32,
    }

    impl ReleaseAuth {
        /// Creates a new ReleaseAuth contract
        /// 
        /// # Arguments
        /// * `admin` - The admin account that can manage signer list
        #[ink(constructor)]
        pub fn new(admin: AccountId) -> Self {
            Self {
                admin,
                signers: Mapping::default(),
                used_nonces: Mapping::default(),
                signer_count: 0,
            }
        }

        /// Adds a new authorized signer to the list (admin only)
        /// 
        /// # Arguments
        /// * `pubkey` - The 32-byte public key to authorize
        /// 
        /// # Emits
        /// * `SignerAdded` event
        #[ink(message)]
        pub fn add_signer(&mut self, pubkey: [u8; 32]) -> Result<(), ReleaseAuthError> {
            if self.env().caller() != self.admin {
                return Err(ReleaseAuthError::Unauthorized);
            }

            if self.signers.get(&pubkey).unwrap_or(false) {
                return Err(ReleaseAuthError::SignerAlreadyExists);
            }

            self.signers.insert(&pubkey, &true);
            self.signer_count = self.signer_count.saturating_add(1);

            self.env().emit_event(SignerAdded { pubkey });

            Ok(())
        }

        /// Removes an authorized signer from the list (admin only)
        /// 
        /// # Arguments
        /// * `pubkey` - The 32-byte public key to remove
        /// 
        /// # Emits
        /// * `SignerRemoved` event
        #[ink(message)]
        pub fn remove_signer(&mut self, pubkey: [u8; 32]) -> Result<(), ReleaseAuthError> {
            if self.env().caller() != self.admin {
                return Err(ReleaseAuthError::Unauthorized);
            }

            if !self.signers.get(&pubkey).unwrap_or(false) {
                return Err(ReleaseAuthError::SignerNotFound);
            }

            self.signers.insert(&pubkey, &false);
            self.signer_count = self.signer_count.saturating_sub(1);

            self.env().emit_event(SignerRemoved { pubkey });

            Ok(())
        }

        /// Authorizes a release by verifying the signature against the payload
        /// 
        /// This is a simplified signature verification. In production, this would:
        /// 1. Hash the payload
        /// 2. Recover the signer's public key from the signature
        /// 3. Check if the recovered key is in the authorized signers list
        /// 4. Verify the nonce hasn't been used before
        /// 
        /// # Arguments
        /// * `booking_id` - The booking ID from the payload
        /// * `mentee` - The mentee account from the payload
        /// * `mentor` - The mentor account from the payload
        /// * `amount` - The release amount from the payload
        /// * `token` - The token address from the payload
        /// * `nonce` - The nonce from the payload
        /// * `signature` - The 65-byte signature (r || s || v format)
        /// 
        /// # Returns
        /// * `true` if signature is valid and not a replay
        /// * `false` if signature is invalid
        /// 
        /// # Emits
        /// * `ReleaseAuthorized` event on successful verification
        #[ink(message)]
        pub fn authorize(
            &mut self,
            booking_id: u64,
            mentee: AccountId,
            mentor: AccountId,
            amount: Balance,
            token: AccountId,
            nonce: u64,
            signature: [u8; 65],
        ) -> Result<bool, ReleaseAuthError> {
            // Check if nonce has been used (replay protection)
            if self.used_nonces.get(&nonce).unwrap_or(false) {
                return Err(ReleaseAuthError::NonceAlreadyUsed);
            }

            // Construct the payload
            let payload = ReleasePayload {
                booking_id,
                mentee,
                mentor,
                amount,
                token,
                nonce,
            };

            // Encode the payload
            let payload_bytes = payload.encode();
            let payload_hash = self.env().hash_bytes::<Blake2x256>(&payload_bytes);

            // Extract signer pubkey from signature (simplified verification)
            // In production, this would use proper ECDSA recovery
            let signer_pubkey = self.recover_signer(&payload_hash, &signature)?;

            // Check if signer is authorized
            if !self.signers.get(&signer_pubkey).unwrap_or(false) {
                return Err(ReleaseAuthError::SignerNotFound);
            }

            // Mark nonce as used to prevent replay
            self.used_nonces.insert(&nonce, &true);

            // Emit authorization event
            self.env().emit_event(ReleaseAuthorized {
                booking_id,
                mentor,
                signer: signer_pubkey,
            });

            Ok(true)
        }

        /// Checks if a nonce has already been used (replay prevention check)
        /// 
        /// # Arguments
        /// * `nonce` - The nonce to check
        /// 
        /// # Returns
        /// * `true` if nonce has been used
        /// * `false` if nonce is available
        #[ink(message)]
        pub fn is_nonce_used(&self, nonce: u64) -> bool {
            self.used_nonces.get(&nonce).unwrap_or(false)
        }

        /// Checks if a signer is authorized
        /// 
        /// # Arguments
        /// * `pubkey` - The 32-byte public key to check
        /// 
        /// # Returns
        /// * `true` if signer is authorized
        /// * `false` otherwise
        #[ink(message)]
        pub fn is_signer_authorized(&self, pubkey: [u8; 32]) -> bool {
            self.signers.get(&pubkey).unwrap_or(false)
        }

        /// Returns the admin account
        #[ink(message)]
        pub fn get_admin(&self) -> AccountId {
            self.admin
        }

        /// Returns the number of authorized signers
        #[ink(message)]
        pub fn get_signer_count(&self) -> u32 {
            self.signer_count
        }

        /// Transfers admin rights to a new account (admin only)
        /// 
        /// # Arguments
        /// * `new_admin` - The new admin account
        #[ink(message)]
        pub fn transfer_admin(&mut self, new_admin: AccountId) -> Result<(), ReleaseAuthError> {
            if self.env().caller() != self.admin {
                return Err(ReleaseAuthError::Unauthorized);
            }

            self.admin = new_admin;
            Ok(())
        }

        /// Internal helper to recover signer from signature and payload hash
        /// 
        /// This is a simplified implementation. In production, this would:
        /// 1. Use proper ECDSA recovery (secp256k1 or Ed25519)
        /// 2. Convert the signature format correctly
        /// 3. Return the recovered public key
        /// 
        /// For testing purposes, we use a deterministic derivation
        fn recover_signer(
            &self,
            _payload_hash: &[u8; 32],
            signature: &[u8; 65],
        ) -> Result<[u8; 32], ReleaseAuthError> {
            // Extract the public key from the first 32 bytes of signature (testing only)
            // In production, use proper ECDSA recovery algorithm
            let mut pubkey = [0u8; 32];
            pubkey.copy_from_slice(&signature[0..32]);
            Ok(pubkey)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use ink::env::test::{default_accounts, set_caller, DefaultAccounts};

        fn get_accounts() -> DefaultAccounts<ink::env::DefaultEnvironment> {
            default_accounts::<ink::env::DefaultEnvironment>()
        }

        fn create_signature_from_pubkey(pubkey: [u8; 32]) -> [u8; 65] {
            let mut sig = [0u8; 65];
            sig[0..32].copy_from_slice(&pubkey);
            sig[32] = 1; // dummy s value
            sig[64] = 27; // dummy v value
            sig
        }

        #[ink::test]
        fn test_new_contract() {
            let accounts = get_accounts();
            let contract = ReleaseAuth::new(accounts.alice);
            assert_eq!(contract.get_admin(), accounts.alice);
            assert_eq!(contract.get_signer_count(), 0);
        }

        #[ink::test]
        fn test_add_signer_success() {
            let accounts = get_accounts();
            let mut contract = ReleaseAuth::new(accounts.alice);

            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let pubkey = [1u8; 32];

            assert!(contract.add_signer(pubkey).is_ok());
            assert!(contract.is_signer_authorized(pubkey));
            assert_eq!(contract.get_signer_count(), 1);
        }

        #[ink::test]
        fn test_add_signer_unauthorized() {
            let accounts = get_accounts();
            let mut contract = ReleaseAuth::new(accounts.alice);

            set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            let pubkey = [1u8; 32];

            assert_eq!(
                contract.add_signer(pubkey),
                Err(ReleaseAuthError::Unauthorized)
            );
        }

        #[ink::test]
        fn test_add_signer_already_exists() {
            let accounts = get_accounts();
            let mut contract = ReleaseAuth::new(accounts.alice);

            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let pubkey = [1u8; 32];

            assert!(contract.add_signer(pubkey).is_ok());
            assert_eq!(
                contract.add_signer(pubkey),
                Err(ReleaseAuthError::SignerAlreadyExists)
            );
        }

        #[ink::test]
        fn test_remove_signer_success() {
            let accounts = get_accounts();
            let mut contract = ReleaseAuth::new(accounts.alice);

            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let pubkey = [1u8; 32];

            assert!(contract.add_signer(pubkey).is_ok());
            assert_eq!(contract.get_signer_count(), 1);

            assert!(contract.remove_signer(pubkey).is_ok());
            assert!(!contract.is_signer_authorized(pubkey));
            assert_eq!(contract.get_signer_count(), 0);
        }

        #[ink::test]
        fn test_remove_signer_unauthorized() {
            let accounts = get_accounts();
            let mut contract = ReleaseAuth::new(accounts.alice);

            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let pubkey = [1u8; 32];
            assert!(contract.add_signer(pubkey).is_ok());

            set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            assert_eq!(
                contract.remove_signer(pubkey),
                Err(ReleaseAuthError::Unauthorized)
            );
        }

        #[ink::test]
        fn test_remove_signer_not_found() {
            let accounts = get_accounts();
            let mut contract = ReleaseAuth::new(accounts.alice);

            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let pubkey = [1u8; 32];

            assert_eq!(
                contract.remove_signer(pubkey),
                Err(ReleaseAuthError::SignerNotFound)
            );
        }

        #[ink::test]
        fn test_authorize_success() {
            let accounts = get_accounts();
            let mut contract = ReleaseAuth::new(accounts.alice);

            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let pubkey = [5u8; 32];
            assert!(contract.add_signer(pubkey).is_ok());

            let token = accounts.charlie;
            let signature = create_signature_from_pubkey(pubkey);
            let nonce = 1u64;

            let result = contract.authorize(
                100,
                accounts.bob,
                accounts.alice,
                1000,
                token,
                nonce,
                signature,
            );

            assert!(result.is_ok());
            assert!(result.unwrap());
            assert!(contract.is_nonce_used(nonce));
        }

        #[ink::test]
        fn test_authorize_invalid_signature() {
            let accounts = get_accounts();
            let mut contract = ReleaseAuth::new(accounts.alice);

            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let pubkey = [5u8; 32];
            assert!(contract.add_signer(pubkey).is_ok());

            let token = accounts.charlie;
            let invalid_pubkey = [10u8; 32];
            let signature = create_signature_from_pubkey(invalid_pubkey);
            let nonce = 1u64;

            let result = contract.authorize(
                100,
                accounts.bob,
                accounts.alice,
                1000,
                token,
                nonce,
                signature,
            );

            assert_eq!(result, Err(ReleaseAuthError::SignerNotFound));
        }

        #[ink::test]
        fn test_authorize_replay_prevention() {
            let accounts = get_accounts();
            let mut contract = ReleaseAuth::new(accounts.alice);

            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let pubkey = [5u8; 32];
            assert!(contract.add_signer(pubkey).is_ok());

            let token = accounts.charlie;
            let signature = create_signature_from_pubkey(pubkey);
            let nonce = 1u64;

            // First authorization succeeds
            let result1 = contract.authorize(
                100,
                accounts.bob,
                accounts.alice,
                1000,
                token,
                nonce,
                signature,
            );
            assert!(result1.is_ok());

            // Replay with same nonce fails
            let result2 = contract.authorize(
                100,
                accounts.bob,
                accounts.alice,
                1000,
                token,
                nonce,
                signature,
            );
            assert_eq!(result2, Err(ReleaseAuthError::NonceAlreadyUsed));
        }

        #[ink::test]
        fn test_authorize_different_nonce_succeeds() {
            let accounts = get_accounts();
            let mut contract = ReleaseAuth::new(accounts.alice);

            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let pubkey = [5u8; 32];
            assert!(contract.add_signer(pubkey).is_ok());

            let token = accounts.charlie;
            let signature = create_signature_from_pubkey(pubkey);

            // First authorization with nonce 1
            let result1 = contract.authorize(
                100,
                accounts.bob,
                accounts.alice,
                1000,
                token,
                1,
                signature,
            );
            assert!(result1.is_ok());

            // Second authorization with nonce 2 succeeds
            let result2 = contract.authorize(
                100,
                accounts.bob,
                accounts.alice,
                1000,
                token,
                2,
                signature,
            );
            assert!(result2.is_ok());
        }

        #[ink::test]
        fn test_nonce_tracking() {
            let accounts = get_accounts();
            let mut contract = ReleaseAuth::new(accounts.alice);

            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let pubkey = [5u8; 32];
            assert!(contract.add_signer(pubkey).is_ok());

            assert!(!contract.is_nonce_used(100));

            let token = accounts.charlie;
            let signature = create_signature_from_pubkey(pubkey);

            contract
                .authorize(50, accounts.bob, accounts.alice, 1000, token, 100, signature)
                .ok();

            assert!(contract.is_nonce_used(100));
        }

        #[ink::test]
        fn test_is_signer_authorized() {
            let accounts = get_accounts();
            let mut contract = ReleaseAuth::new(accounts.alice);

            let pubkey1 = [1u8; 32];
            let pubkey2 = [2u8; 32];

            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            assert!(contract.add_signer(pubkey1).is_ok());

            assert!(contract.is_signer_authorized(pubkey1));
            assert!(!contract.is_signer_authorized(pubkey2));
        }

        #[ink::test]
        fn test_transfer_admin_success() {
            let accounts = get_accounts();
            let mut contract = ReleaseAuth::new(accounts.alice);

            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            assert!(contract.transfer_admin(accounts.bob).is_ok());
            assert_eq!(contract.get_admin(), accounts.bob);
        }

        #[ink::test]
        fn test_transfer_admin_unauthorized() {
            let accounts = get_accounts();
            let mut contract = ReleaseAuth::new(accounts.alice);

            set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            assert_eq!(
                contract.transfer_admin(accounts.charlie),
                Err(ReleaseAuthError::Unauthorized)
            );
        }

        #[ink::test]
        fn test_multiple_signers() {
            let accounts = get_accounts();
            let mut contract = ReleaseAuth::new(accounts.alice);

            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);

            let pubkey1 = [1u8; 32];
            let pubkey2 = [2u8; 32];
            let pubkey3 = [3u8; 32];

            assert!(contract.add_signer(pubkey1).is_ok());
            assert!(contract.add_signer(pubkey2).is_ok());
            assert!(contract.add_signer(pubkey3).is_ok());

            assert_eq!(contract.get_signer_count(), 3);
            assert!(contract.is_signer_authorized(pubkey1));
            assert!(contract.is_signer_authorized(pubkey2));
            assert!(contract.is_signer_authorized(pubkey3));

            assert!(contract.remove_signer(pubkey2).is_ok());
            assert_eq!(contract.get_signer_count(), 2);
            assert!(!contract.is_signer_authorized(pubkey2));
        }

        #[ink::test]
        fn test_authorize_multiple_releases() {
            let accounts = get_accounts();
            let mut contract = ReleaseAuth::new(accounts.alice);

            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let pubkey = [5u8; 32];
            assert!(contract.add_signer(pubkey).is_ok());

            let token = accounts.charlie;
            let signature = create_signature_from_pubkey(pubkey);

            // Authorize multiple releases with different nonces
            for i in 1..5 {
                let result = contract.authorize(
                    100 + i as u64,
                    accounts.bob,
                    accounts.alice,
                    1000,
                    token,
                    i as u64,
                    signature,
                );
                assert!(result.is_ok());
                assert!(contract.is_nonce_used(i as u64));
            }
        }

        #[ink::test]
        fn test_signer_count_accuracy() {
            let accounts = get_accounts();
            let mut contract = ReleaseAuth::new(accounts.alice);

            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);

            assert_eq!(contract.get_signer_count(), 0);

            let pubkey1 = [1u8; 32];
            let pubkey2 = [2u8; 32];

            assert!(contract.add_signer(pubkey1).is_ok());
            assert_eq!(contract.get_signer_count(), 1);

            assert!(contract.add_signer(pubkey2).is_ok());
            assert_eq!(contract.get_signer_count(), 2);

            assert!(contract.remove_signer(pubkey1).is_ok());
            assert_eq!(contract.get_signer_count(), 1);

            assert!(contract.remove_signer(pubkey2).is_ok());
            assert_eq!(contract.get_signer_count(), 0);
        }

        #[ink::test]
        fn test_nonce_isolation() {
            let accounts = get_accounts();
            let mut contract = ReleaseAuth::new(accounts.alice);

            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let pubkey = [5u8; 32];
            assert!(contract.add_signer(pubkey).is_ok());

            let token = accounts.charlie;
            let signature = create_signature_from_pubkey(pubkey);

            // Use nonce 1
            contract
                .authorize(100, accounts.bob, accounts.alice, 1000, token, 1, signature)
                .ok();

            // Nonce 1 is used
            assert!(contract.is_nonce_used(1));
            // Nonce 2 is not used
            assert!(!contract.is_nonce_used(2));
            // Nonce 3 is not used
            assert!(!contract.is_nonce_used(3));
        }
    }
}
