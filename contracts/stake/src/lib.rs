#![cfg_attr(not(feature = "std"), no_std)]

use ink::storage::Mapping;

/// Mentor staking contract for SkillSync
/// 
/// Allows mentors to stake tokens to signal commitment, with admin-enforced slashing
/// for fraudulent behavior. Implements cooldown periods for unstaking to prevent abuse.
#[ink::contract]
mod stake {
    use super::*;

    /// Event emitted when a mentor stakes tokens
    #[ink(event)]
    pub struct Staked {
        #[ink(topic)]
        mentor: AccountId,
        #[ink(topic)]
        token: AccountId,
        amount: Balance,
    }

    /// Event emitted when a mentor requests to unstake
    #[ink(event)]
    pub struct UnstakeRequested {
        #[ink(topic)]
        mentor: AccountId,
        #[ink(topic)]
        token: AccountId,
        amount: Balance,
        unlock_at: BlockNumber,
    }

    /// Event emitted when a mentor withdraws unstaked tokens
    #[ink(event)]
    pub struct Unstaked {
        #[ink(topic)]
        mentor: AccountId,
        #[ink(topic)]
        token: AccountId,
        amount: Balance,
    }

    /// Event emitted when admin slashes a mentor's stake
    #[ink(event)]
    pub struct Slashed {
        #[ink(topic)]
        mentor: AccountId,
        #[ink(topic)]
        token: AccountId,
        amount: Balance,
    }

    /// Stake information for a mentor and token
    #[derive(scale::Encode, scale::Decode, Clone, Debug)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub struct StakeInfo {
        /// Amount currently staked
        staked: Balance,
        /// Amount pending withdrawal
        pending_unstake: Balance,
        /// Block number when unstaked tokens can be withdrawn
        unlock_at: BlockNumber,
    }

    /// Storage for the Stake contract
    #[ink(storage)]
    pub struct Stake {
        /// Mapping of (mentor, token) to StakeInfo
        stakes: Mapping<(AccountId, AccountId), StakeInfo>,
        /// Admin account that can slash stakes
        admin: AccountId,
        /// Cooldown period in blocks before unstaked tokens can be withdrawn
        cooldown_blocks: BlockNumber,
    }

    /// Custom errors for the Stake contract
    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum StakeError {
        /// Caller is not authorized (admin-only operation)
        Unauthorized,
        /// Insufficient staked amount
        InsufficientStake,
        /// Insufficient pending unstake amount
        InsufficientPendingUnstake,
        /// Unstaking period has not elapsed yet
        UnlockTimeNotReached,
        /// Amount is zero
        ZeroAmount,
        /// Stake not found for this mentor/token pair
        StakeNotFound,
    }

    impl Stake {
        /// Creates a new Stake contract
        /// 
        /// # Arguments
        /// * `admin` - The admin account that can slash stakes
        /// * `cooldown_blocks` - Cooldown period in blocks before unstaked tokens can be withdrawn
        #[ink(constructor)]
        pub fn new(admin: AccountId, cooldown_blocks: BlockNumber) -> Self {
            Self {
                stakes: Mapping::default(),
                admin,
                cooldown_blocks,
            }
        }

        /// Stakes tokens for a mentor
        /// 
        /// # Arguments
        /// * `token` - The token address being staked
        /// * `amount` - The amount to stake
        /// 
        /// # Emits
        /// * `Staked` event
        #[ink(message)]
        pub fn stake(&mut self, token: AccountId, amount: Balance) -> Result<(), StakeError> {
            if amount == 0 {
                return Err(StakeError::ZeroAmount);
            }

            let mentor = self.env().caller();
            let key = (mentor, token);

            let mut stake_info = self.stakes.get(&key).unwrap_or(StakeInfo {
                staked: 0,
                pending_unstake: 0,
                unlock_at: 0,
            });

            stake_info.staked = stake_info.staked.checked_add(amount)
                .ok_or(StakeError::InsufficientStake)?;

            self.stakes.insert(&key, &stake_info);

            self.env().emit_event(Staked {
                mentor,
                token,
                amount,
            });

            Ok(())
        }

        /// Requests unstaking of tokens, initiating the cooldown period
        /// 
        /// # Arguments
        /// * `token` - The token address to unstake
        /// * `amount` - The amount to unstake
        /// 
        /// # Returns
        /// The block number when tokens can be withdrawn
        /// 
        /// # Emits
        /// * `UnstakeRequested` event
        #[ink(message)]
        pub fn request_unstake(&mut self, token: AccountId, amount: Balance) -> Result<BlockNumber, StakeError> {
            if amount == 0 {
                return Err(StakeError::ZeroAmount);
            }

            let mentor = self.env().caller();
            let key = (mentor, token);

            let mut stake_info = self.stakes.get(&key)
                .ok_or(StakeError::StakeNotFound)?;

            if stake_info.staked < amount {
                return Err(StakeError::InsufficientStake);
            }

            stake_info.staked = stake_info.staked.checked_sub(amount)
                .ok_or(StakeError::InsufficientStake)?;

            let current_block = self.env().block_number();
            let unlock_at = current_block.checked_add(self.cooldown_blocks)
                .ok_or(StakeError::InsufficientStake)?;

            stake_info.pending_unstake = stake_info.pending_unstake.checked_add(amount)
                .ok_or(StakeError::InsufficientStake)?;
            stake_info.unlock_at = unlock_at;

            self.stakes.insert(&key, &stake_info);

            self.env().emit_event(UnstakeRequested {
                mentor,
                token,
                amount,
                unlock_at,
            });

            Ok(unlock_at)
        }

        /// Withdraws unstaked tokens after cooldown period has elapsed
        /// 
        /// # Arguments
        /// * `token` - The token address to withdraw
        /// 
        /// # Emits
        /// * `Unstaked` event
        #[ink(message)]
        pub fn withdraw_unstaked(&mut self, token: AccountId) -> Result<(), StakeError> {
            let mentor = self.env().caller();
            let key = (mentor, token);

            let mut stake_info = self.stakes.get(&key)
                .ok_or(StakeError::StakeNotFound)?;

            if stake_info.pending_unstake == 0 {
                return Err(StakeError::InsufficientPendingUnstake);
            }

            let current_block = self.env().block_number();
            if current_block < stake_info.unlock_at {
                return Err(StakeError::UnlockTimeNotReached);
            }

            let amount = stake_info.pending_unstake;
            stake_info.pending_unstake = 0;
            stake_info.unlock_at = 0;

            self.stakes.insert(&key, &stake_info);

            self.env().emit_event(Unstaked {
                mentor,
                token,
                amount,
            });

            Ok(())
        }

        /// Slashes a mentor's stake for fraudulent behavior (admin only)
        /// 
        /// # Arguments
        /// * `mentor` - The mentor account to slash
        /// * `token` - The token address to slash
        /// * `amount` - The amount to slash
        /// 
        /// # Emits
        /// * `Slashed` event
        #[ink(message)]
        pub fn slash(&mut self, mentor: AccountId, token: AccountId, amount: Balance) -> Result<(), StakeError> {
            if self.env().caller() != self.admin {
                return Err(StakeError::Unauthorized);
            }

            if amount == 0 {
                return Err(StakeError::ZeroAmount);
            }

            let key = (mentor, token);

            let mut stake_info = self.stakes.get(&key)
                .ok_or(StakeError::StakeNotFound)?;

            if stake_info.staked < amount {
                return Err(StakeError::InsufficientStake);
            }

            stake_info.staked = stake_info.staked.checked_sub(amount)
                .ok_or(StakeError::InsufficientStake)?;

            self.stakes.insert(&key, &stake_info);

            self.env().emit_event(Slashed {
                mentor,
                token,
                amount,
            });

            Ok(())
        }

        /// Returns the stake information for a mentor and token
        /// 
        /// # Arguments
        /// * `mentor` - The mentor account
        /// * `token` - The token address
        /// 
        /// # Returns
        /// The StakeInfo, or None if no stake exists
        #[ink(message)]
        pub fn get_stake(&self, mentor: AccountId, token: AccountId) -> Option<StakeInfo> {
            self.stakes.get(&(mentor, token))
        }

        /// Returns the admin account
        #[ink(message)]
        pub fn get_admin(&self) -> AccountId {
            self.admin
        }

        /// Returns the cooldown period in blocks
        #[ink(message)]
        pub fn get_cooldown_blocks(&self) -> BlockNumber {
            self.cooldown_blocks
        }

        /// Transfers admin rights to a new account (admin only)
        /// 
        /// # Arguments
        /// * `new_admin` - The new admin account
        #[ink(message)]
        pub fn transfer_admin(&mut self, new_admin: AccountId) -> Result<(), StakeError> {
            if self.env().caller() != self.admin {
                return Err(StakeError::Unauthorized);
            }

            self.admin = new_admin;
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use ink::env::test::{default_accounts, set_caller, set_block_number, DefaultAccounts};

        fn get_accounts() -> DefaultAccounts<ink::env::DefaultEnvironment> {
            default_accounts::<ink::env::DefaultEnvironment>()
        }

        #[ink::test]
        fn test_new_contract() {
            let accounts = get_accounts();
            let contract = Stake::new(accounts.alice, 100);
            assert_eq!(contract.get_admin(), accounts.alice);
            assert_eq!(contract.get_cooldown_blocks(), 100);
        }

        #[ink::test]
        fn test_stake_success() {
            let accounts = get_accounts();
            let mut contract = Stake::new(accounts.alice, 100);

            set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            let token = accounts.charlie;

            assert!(contract.stake(token, 1000).is_ok());

            let stake_info = contract.get_stake(accounts.bob, token).unwrap();
            assert_eq!(stake_info.staked, 1000);
            assert_eq!(stake_info.pending_unstake, 0);
        }

        #[ink::test]
        fn test_stake_zero_amount() {
            let accounts = get_accounts();
            let mut contract = Stake::new(accounts.alice, 100);

            set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            let token = accounts.charlie;

            assert_eq!(contract.stake(token, 0), Err(StakeError::ZeroAmount));
        }

        #[ink::test]
        fn test_stake_multiple_deposits() {
            let accounts = get_accounts();
            let mut contract = Stake::new(accounts.alice, 100);

            set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            let token = accounts.charlie;

            assert!(contract.stake(token, 1000).is_ok());
            assert!(contract.stake(token, 500).is_ok());

            let stake_info = contract.get_stake(accounts.bob, token).unwrap();
            assert_eq!(stake_info.staked, 1500);
        }

        #[ink::test]
        fn test_request_unstake_success() {
            let accounts = get_accounts();
            let mut contract = Stake::new(accounts.alice, 100);
            let token = accounts.charlie;

            set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            assert!(contract.stake(token, 1000).is_ok());

            set_block_number::<ink::env::DefaultEnvironment>(5);
            let unlock_at = contract.request_unstake(token, 400).unwrap();

            assert_eq!(unlock_at, 105); // current block (5) + cooldown (100)

            let stake_info = contract.get_stake(accounts.bob, token).unwrap();
            assert_eq!(stake_info.staked, 600);
            assert_eq!(stake_info.pending_unstake, 400);
            assert_eq!(stake_info.unlock_at, 105);
        }

        #[ink::test]
        fn test_request_unstake_insufficient_stake() {
            let accounts = get_accounts();
            let mut contract = Stake::new(accounts.alice, 100);
            let token = accounts.charlie;

            set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            assert!(contract.stake(token, 1000).is_ok());

            assert_eq!(
                contract.request_unstake(token, 1500),
                Err(StakeError::InsufficientStake)
            );
        }

        #[ink::test]
        fn test_request_unstake_zero_amount() {
            let accounts = get_accounts();
            let mut contract = Stake::new(accounts.alice, 100);
            let token = accounts.charlie;

            set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            assert!(contract.stake(token, 1000).is_ok());

            assert_eq!(contract.request_unstake(token, 0), Err(StakeError::ZeroAmount));
        }

        #[ink::test]
        fn test_request_unstake_no_stake() {
            let accounts = get_accounts();
            let contract = Stake::new(accounts.alice, 100);
            let token = accounts.charlie;

            let mut mut_contract = contract;
            set_caller::<ink::env::DefaultEnvironment>(accounts.bob);

            assert_eq!(
                mut_contract.request_unstake(token, 100),
                Err(StakeError::StakeNotFound)
            );
        }

        #[ink::test]
        fn test_withdraw_unstaked_success() {
            let accounts = get_accounts();
            let mut contract = Stake::new(accounts.alice, 100);
            let token = accounts.charlie;

            set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            assert!(contract.stake(token, 1000).is_ok());

            set_block_number::<ink::env::DefaultEnvironment>(5);
            let unlock_at = contract.request_unstake(token, 400).unwrap();

            // Move to unlock block
            set_block_number::<ink::env::DefaultEnvironment>(unlock_at);
            assert!(contract.withdraw_unstaked(token).is_ok());

            let stake_info = contract.get_stake(accounts.bob, token).unwrap();
            assert_eq!(stake_info.staked, 600);
            assert_eq!(stake_info.pending_unstake, 0);
            assert_eq!(stake_info.unlock_at, 0);
        }

        #[ink::test]
        fn test_withdraw_unstaked_cooldown_not_reached() {
            let accounts = get_accounts();
            let mut contract = Stake::new(accounts.alice, 100);
            let token = accounts.charlie;

            set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            assert!(contract.stake(token, 1000).is_ok());

            set_block_number::<ink::env::DefaultEnvironment>(5);
            contract.request_unstake(token, 400).ok();

            // Try to withdraw before cooldown expires
            set_block_number::<ink::env::DefaultEnvironment>(50);
            assert_eq!(
                contract.withdraw_unstaked(token),
                Err(StakeError::UnlockTimeNotReached)
            );
        }

        #[ink::test]
        fn test_withdraw_unstaked_no_pending() {
            let accounts = get_accounts();
            let mut contract = Stake::new(accounts.alice, 100);
            let token = accounts.charlie;

            set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            assert!(contract.stake(token, 1000).is_ok());

            assert_eq!(
                contract.withdraw_unstaked(token),
                Err(StakeError::InsufficientPendingUnstake)
            );
        }

        #[ink::test]
        fn test_slash_success() {
            let accounts = get_accounts();
            let mut contract = Stake::new(accounts.alice, 100);
            let token = accounts.charlie;

            set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            assert!(contract.stake(token, 1000).is_ok());

            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            assert!(contract.slash(accounts.bob, token, 300).is_ok());

            let stake_info = contract.get_stake(accounts.bob, token).unwrap();
            assert_eq!(stake_info.staked, 700);
        }

        #[ink::test]
        fn test_slash_unauthorized() {
            let accounts = get_accounts();
            let mut contract = Stake::new(accounts.alice, 100);
            let token = accounts.charlie;

            set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            assert!(contract.stake(token, 1000).is_ok());

            set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            assert_eq!(
                contract.slash(accounts.bob, token, 300),
                Err(StakeError::Unauthorized)
            );
        }

        #[ink::test]
        fn test_slash_insufficient_stake() {
            let accounts = get_accounts();
            let mut contract = Stake::new(accounts.alice, 100);
            let token = accounts.charlie;

            set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            assert!(contract.stake(token, 1000).is_ok());

            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            assert_eq!(
                contract.slash(accounts.bob, token, 1500),
                Err(StakeError::InsufficientStake)
            );
        }

        #[ink::test]
        fn test_slash_zero_amount() {
            let accounts = get_accounts();
            let mut contract = Stake::new(accounts.alice, 100);
            let token = accounts.charlie;

            set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            assert!(contract.stake(token, 1000).is_ok());

            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            assert_eq!(
                contract.slash(accounts.bob, token, 0),
                Err(StakeError::ZeroAmount)
            );
        }

        #[ink::test]
        fn test_slash_no_stake() {
            let accounts = get_accounts();
            let mut contract = Stake::new(accounts.alice, 100);
            let token = accounts.charlie;

            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            assert_eq!(
                contract.slash(accounts.bob, token, 100),
                Err(StakeError::StakeNotFound)
            );
        }

        #[ink::test]
        fn test_transfer_admin_success() {
            let accounts = get_accounts();
            let mut contract = Stake::new(accounts.alice, 100);

            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            assert!(contract.transfer_admin(accounts.bob).is_ok());
            assert_eq!(contract.get_admin(), accounts.bob);
        }

        #[ink::test]
        fn test_transfer_admin_unauthorized() {
            let accounts = get_accounts();
            let mut contract = Stake::new(accounts.alice, 100);

            set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            assert_eq!(
                contract.transfer_admin(accounts.charlie),
                Err(StakeError::Unauthorized)
            );
        }

        #[ink::test]
        fn test_cooldown_boundary() {
            let accounts = get_accounts();
            let mut contract = Stake::new(accounts.alice, 100);
            let token = accounts.charlie;

            set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            assert!(contract.stake(token, 1000).is_ok());

            set_block_number::<ink::env::DefaultEnvironment>(10);
            let unlock_at = contract.request_unstake(token, 500).unwrap();
            assert_eq!(unlock_at, 110);

            // Should fail at block 109
            set_block_number::<ink::env::DefaultEnvironment>(109);
            assert_eq!(
                contract.withdraw_unstaked(token),
                Err(StakeError::UnlockTimeNotReached)
            );

            // Should succeed at block 110
            set_block_number::<ink::env::DefaultEnvironment>(110);
            assert!(contract.withdraw_unstaked(token).is_ok());
        }

        #[ink::test]
        fn test_partial_stake_slashing() {
            let accounts = get_accounts();
            let mut contract = Stake::new(accounts.alice, 100);
            let token = accounts.charlie;

            set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            assert!(contract.stake(token, 1000).is_ok());

            set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            assert!(contract.slash(accounts.bob, token, 250).is_ok());
            assert!(contract.slash(accounts.bob, token, 250).is_ok());
            assert!(contract.slash(accounts.bob, token, 250).is_ok());

            let stake_info = contract.get_stake(accounts.bob, token).unwrap();
            assert_eq!(stake_info.staked, 250);
        }

        #[ink::test]
        fn test_stake_after_partial_unstake() {
            let accounts = get_accounts();
            let mut contract = Stake::new(accounts.alice, 100);
            let token = accounts.charlie;

            set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            assert!(contract.stake(token, 1000).is_ok());

            set_block_number::<ink::env::DefaultEnvironment>(5);
            assert!(contract.request_unstake(token, 400).is_ok());

            // Stake more while unstaking is pending
            assert!(contract.stake(token, 300).is_ok());

            let stake_info = contract.get_stake(accounts.bob, token).unwrap();
            assert_eq!(stake_info.staked, 900); // 1000 - 400 + 300
            assert_eq!(stake_info.pending_unstake, 400);
        }
    }
}
