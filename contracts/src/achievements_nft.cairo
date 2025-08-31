use starknet::ContractAddress;

#[starknet::interface]
trait IAchievementNFT<TContractState> {
    fn mint_achievement(ref self: TContractState, to: ContractAddress, achievement_type: felt252, metadata_uri: felt252) -> u256;
    fn burn_achievement(ref self: TContractState, token_id: u256);
    fn get_achievement_metadata(self: @TContractState, token_id: u256) -> AchievementMetadata;
    fn get_user_achievements(self: @TContractState, user: ContractAddress) -> Array<u256>;
    fn verify_achievement(self: @TContractState, user: ContractAddress, achievement_type: felt252) -> bool;
    fn set_achievement_verifier(ref self: TContractState, verifier: ContractAddress);
    fn get_total_supply(self: @TContractState) -> u256;
    fn owner_of(self: @TContractState, token_id: u256) -> ContractAddress;
    fn balance_of(self: @TContractState, owner: ContractAddress) -> u256;
}

#[derive(Drop, Serde, starknet::Store)]
struct AchievementMetadata {
    token_id: u256,
    achievement_type: felt252,
    metadata_uri: felt252,
    issued_timestamp: u64,
    issuer: ContractAddress,
    is_transferable: bool,
    skill_level: u8, // 1-5 skill level
}

#[starknet::contract]
mod AchievementNFT {
    use super::{IAchievementNFT, AchievementMetadata};
    use starknet::{ContractAddress, get_caller_address, get_block_timestamp};
    use starknet::storage::{
        StoragePointerReadAccess, StoragePointerWriteAccess, StoragePathEntry, Map, Vec, VecTrait, MutableVecTrait
    };

    #[storage]
    struct Storage {
        // NFT core storage
        owners: Map<u256, ContractAddress>,
        balances: Map<ContractAddress, u256>,
        token_approvals: Map<u256, ContractAddress>,
        operator_approvals: Map<(ContractAddress, ContractAddress), bool>,
        
        // Achievement specific storage
        achievements: Map<u256, AchievementMetadata>,
        user_achievements: Map<ContractAddress, Vec<u256>>,
        achievement_types: Map<felt252, bool>, // Valid achievement types
        token_counter: u256,
        
        // Access control
        owner: ContractAddress,
        achievement_verifier: ContractAddress,
    }

    #[event]
    #[derive(Drop, starknet::Event)]
    enum Event {
        Transfer: Transfer,
        AchievementMinted: AchievementMinted,
        AchievementBurned: AchievementBurned,
        VerifierUpdated: VerifierUpdated,
    }

    #[derive(Drop, starknet::Event)]
    struct Transfer {
        from: ContractAddress,
        to: ContractAddress,
        token_id: u256,
    }

    #[derive(Drop, starknet::Event)]
    struct AchievementMinted {
        token_id: u256,
        to: ContractAddress,
        achievement_type: felt252,
        issuer: ContractAddress,
    }

    #[derive(Drop, starknet::Event)]
    struct AchievementBurned {
        token_id: u256,
        owner: ContractAddress,
        achievement_type: felt252,
    }

    #[derive(Drop, starknet::Event)]
    struct VerifierUpdated {
        old_verifier: ContractAddress,
        new_verifier: ContractAddress,
    }

    #[constructor]
    fn constructor(ref self: ContractState, owner: ContractAddress, verifier: ContractAddress) {
        self.owner.write(owner);
        self.achievement_verifier.write(verifier);
        
        // Initialize valid achievement types
        self.achievement_types.entry('SKILL_COMPLETION').write(true);
        self.achievement_types.entry('PROJECT_COMPLETION').write(true);
        self.achievement_types.entry('PEER_REVIEW').write(true);
        self.achievement_types.entry('CERTIFICATION').write(true);
        self.achievement_types.entry('MILESTONE').write(true);
    }

    #[abi(embed_v0)]
    impl AchievementNFTImpl of IAchievementNFT<ContractState> {
        fn mint_achievement(ref self: ContractState, to: ContractAddress, achievement_type: felt252, metadata_uri: felt252) -> u256 {
            let caller = get_caller_address();
            let verifier = self.achievement_verifier.read();
            
            // Only verifier or owner can mint achievements
            assert(caller == verifier || caller == self.owner.read(), 'Unauthorized minter');
            assert(self.achievement_types.entry(achievement_type).read(), 'Invalid achievement type');
            
            let token_id = self.token_counter.read() + 1;
            self.token_counter.write(token_id);
            
            // Create achievement metadata
            let metadata = AchievementMetadata {
                token_id,
                achievement_type,
                metadata_uri,
                issued_timestamp: get_block_timestamp(),
                issuer: caller,
                is_transferable: false, // Achievements are non-transferable by default
                skill_level: 1, // Default skill level
            };
            
            // Store achievement
            self.achievements.entry(token_id).write(metadata);
            self.owners.entry(token_id).write(to);
            
            // Update balances
            let current_balance = self.balances.entry(to).read();
            self.balances.entry(to).write(current_balance + 1);
            
            // Add to user's achievement list
            let mut user_achievements = self.user_achievements.entry(to).read();
            user_achievements.append().write(token_id);
            self.user_achievements.entry(to).write(user_achievements);
            
            // Emit events
            self.emit(Transfer { from: Zeroable::zero(), to, token_id });
            self.emit(AchievementMinted { token_id, to, achievement_type, issuer: caller });
            
            token_id
        }

        fn burn_achievement(ref self: ContractState, token_id: u256) {
            let caller = get_caller_address();
            let owner = self.owners.entry(token_id).read();
            
            // Only owner or verifier can burn
            assert(caller == owner || caller == self.achievement_verifier.read() || caller == self.owner.read(), 'Unauthorized burn');
            
            let metadata = self.achievements.entry(token_id).read();
            
            // Clear ownership
            self.owners.entry(token_id).write(Zeroable::zero());
            
            // Update balance
            let current_balance = self.balances.entry(owner).read();
            self.balances.entry(owner).write(current_balance - 1);
            
            // Remove from user achievements (simplified - in production would need proper removal)
            // Note: This is a simplified implementation. In production, you'd want to properly remove from the array
            
            self.emit(Transfer { from: owner, to: Zeroable::zero(), token_id });
            self.emit(AchievementBurned { token_id, owner, achievement_type: metadata.achievement_type });
        }

        fn get_achievement_metadata(self: @ContractState, token_id: u256) -> AchievementMetadata {
            self.achievements.entry(token_id).read()
        }

        fn get_user_achievements(self: @ContractState, user: ContractAddress) -> Array<u256> {
            let user_achievements = self.user_achievements.entry(user).read();
            let mut result = ArrayTrait::new();
            let mut i = 0;
            
            loop {
                if i >= user_achievements.len() {
                    break;
                }
                result.append(user_achievements.at(i).read());
                i += 1;
            };
            
            result
        }

        fn verify_achievement(self: @ContractState, user: ContractAddress, achievement_type: felt252) -> bool {
            let user_achievements = self.user_achievements.entry(user).read();
            let mut i = 0;
            
            loop {
                if i >= user_achievements.len() {
                    break false;
                }
                
                let token_id = user_achievements.at(i).read();
                let metadata = self.achievements.entry(token_id).read();
                
                if metadata.achievement_type == achievement_type {
                    break true;
                }
                
                i += 1;
            }
        }

        fn set_achievement_verifier(ref self: ContractState, verifier: ContractAddress) {
            let caller = get_caller_address();
            assert(caller == self.owner.read(), 'Only owner can set verifier');
            
            let old_verifier = self.achievement_verifier.read();
            self.achievement_verifier.write(verifier);
            
            self.emit(VerifierUpdated { old_verifier, new_verifier: verifier });
        }

        fn get_total_supply(self: @ContractState) -> u256 {
            self.token_counter.read()
        }

        fn owner_of(self: @ContractState, token_id: u256) -> ContractAddress {
            let owner = self.owners.entry(token_id).read();
            assert(!owner.is_zero(), 'Token does not exist');
            owner
        }

        fn balance_of(self: @ContractState, owner: ContractAddress) -> u256 {
            self.balances.entry(owner).read()
        }
    }
}
