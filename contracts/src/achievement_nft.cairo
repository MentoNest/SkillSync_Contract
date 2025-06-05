use starknet::ContractAddress;
use core::array::Array;

#[starknet::interface]
pub trait IAchievementNFT<TContractState> {
    fn mint(ref self: TContractState, user_id: ContractAddress, achievement_id: felt252) -> u256;
    fn get_achievements(self: @TContractState, user_id: ContractAddress) -> Array<felt252>;
    fn has_achievement(
        self: @TContractState, user_id: ContractAddress, achievement_id: felt252,
    ) -> bool;
    fn get_achievement_count(self: @TContractState, user_id: ContractAddress) -> u256;
    fn get_token_achievement(self: @TContractState, token_id: u256) -> felt252;
    fn add_minter(ref self: TContractState, minter: ContractAddress);
    fn remove_minter(ref self: TContractState, minter: ContractAddress);
    fn is_minter(self: @TContractState, account: ContractAddress) -> bool;
}

#[starknet::contract]
mod AchievementNFT {
    use openzeppelin_introspection::src5::SRC5Component;
    use openzeppelin_token::erc721::{ERC721Component, ERC721HooksEmptyImpl};
    use openzeppelin_access::ownable::OwnableComponent;
    use starknet::{ContractAddress, get_caller_address};
    use starknet::storage::{
        Map, StorageMapEntry, StoragePointerReadAccess, StoragePointerWriteAccess,
    };
    use core::array::ArrayTrait;

    component!(path: ERC721Component, storage: erc721, event: ERC721Event);
    component!(path: SRC5Component, storage: src5, event: SRC5Event);
    component!(path: OwnableComponent, storage: ownable, event: OwnableEvent);

    // ERC721 Mixin
    #[abi(embed_v0)]
    impl ERC721MixinImpl = ERC721Component::ERC721MixinImpl<ContractState>;
    impl ERC721InternalImpl = ERC721Component::InternalImpl<ContractState>;

    // Ownable Mixin
    #[abi(embed_v0)]
    impl OwnableMixinImpl = OwnableComponent::OwnableMixinImpl<ContractState>;
    impl OwnableInternalImpl = OwnableComponent::InternalImpl<ContractState>;

    #[storage]
    struct Storage {
        // Maps (user, achievement_id) to whether they have that achievement
        user_achievements: Map<(ContractAddress, felt252), bool>,
        // Maps (user, index) to achievement_id for enumeration
        user_achievement_by_index: Map<(ContractAddress, u256), felt252>,
        // Maps user to count of achievements
        user_achievement_count: Map<ContractAddress, u256>,
        // Maps token_id to achievement_id
        token_achievement: Map<u256, felt252>,
        // Maps achievement_id to description/metadata
        achievement_metadata: Map<felt252, ByteArray>,
        // Token ID counter
        token_id_counter: u256,
        // Authorized minters
        minters: Map<ContractAddress, bool>,
        #[substorage(v0)]
        erc721: ERC721Component::Storage,
        #[substorage(v0)]
        src5: SRC5Component::Storage,
        #[substorage(v0)]
        ownable: OwnableComponent::Storage,
    }

    #[event]
    #[derive(Drop, starknet::Event)]
    enum Event {
        AchievementMinted: AchievementMinted,
        MinterAdded: MinterAdded,
        MinterRemoved: MinterRemoved,
        #[flat]
        ERC721Event: ERC721Component::Event,
        #[flat]
        SRC5Event: SRC5Component::Event,
        #[flat]
        OwnableEvent: OwnableComponent::Event,
    }

    #[derive(Drop, starknet::Event)]
    struct AchievementMinted {
        #[key]
        user: ContractAddress,
        #[key]
        achievement_id: felt252,
        token_id: u256,
    }

    #[derive(Drop, starknet::Event)]
    struct MinterAdded {
        #[key]
        minter: ContractAddress,
    }

    #[derive(Drop, starknet::Event)]
    struct MinterRemoved {
        #[key]
        minter: ContractAddress,
    }

    mod Errors {
        pub const ACHIEVEMENT_ALREADY_MINTED: felt252 = 'Achievement already minted';
        pub const NOT_MINTER: felt252 = 'Caller is not a minter';
        pub const INVALID_USER: felt252 = 'Invalid user address';
        pub const INVALID_ACHIEVEMENT: felt252 = 'Invalid achievement ID';
        pub const TOKEN_NOT_EXISTS: felt252 = 'Token does not exist';
    }

    #[constructor]
    fn constructor(
        ref self: ContractState,
        owner: ContractAddress,
        name: ByteArray,
        symbol: ByteArray,
        base_uri: ByteArray,
    ) {
        self.erc721.initializer(name, symbol, base_uri);
        self.ownable.initializer(owner);
        self.token_id_counter.write(0);

        // Add owner as initial minter
        self.minters.write(owner, true);
        self.emit(MinterAdded { minter: owner });
    }

    #[abi(embed_v0)]
    impl AchievementNFTImpl of super::IAchievementNFT<ContractState> {
        fn mint(
            ref self: ContractState, user_id: ContractAddress, achievement_id: felt252,
        ) -> u256 {
            // Check if caller is authorized minter
            let caller = get_caller_address();
            assert(self.minters.read(caller), Errors::NOT_MINTER);

            // Validate inputs
            assert(!user_id.is_zero(), Errors::INVALID_USER);
            assert(achievement_id != 0, Errors::INVALID_ACHIEVEMENT);

            // Check if user already has this achievement
            assert(
                !self.user_achievements.read((user_id, achievement_id)),
                Errors::ACHIEVEMENT_ALREADY_MINTED,
            );

            // Generate new token ID
            let token_id = self.token_id_counter.read() + 1;
            self.token_id_counter.write(token_id);

            // Mint the NFT
            self.erc721.mint(user_id, token_id);

            // Update user achievements tracking
            self.user_achievements.write((user_id, achievement_id), true);

            let current_count = self.user_achievement_count.read(user_id);
            self.user_achievement_by_index.write((user_id, current_count), achievement_id);
            self.user_achievement_count.write(user_id, current_count + 1);

            // Map token to achievement
            self.token_achievement.write(token_id, achievement_id);

            // Emit event
            self.emit(AchievementMinted { user: user_id, achievement_id, token_id });

            token_id
        }

        fn get_achievements(self: @ContractState, user_id: ContractAddress) -> Array<felt252> {
            let mut achievements = ArrayTrait::new();
            let count = self.user_achievement_count.read(user_id);

            let mut i: u256 = 0;
            loop {
                if i >= count {
                    break;
                }
                let achievement_id = self.user_achievement_by_index.read((user_id, i));
                achievements.append(achievement_id);
                i += 1;
            };

            achievements
        }

        fn has_achievement(
            self: @ContractState, user_id: ContractAddress, achievement_id: felt252,
        ) -> bool {
            self.user_achievements.read((user_id, achievement_id))
        }

        fn get_achievement_count(self: @ContractState, user_id: ContractAddress) -> u256 {
            self.user_achievement_count.read(user_id)
        }

        fn get_token_achievement(self: @ContractState, token_id: u256) -> felt252 {
            assert(self.erc721.exists(token_id), Errors::TOKEN_NOT_EXISTS);
            self.token_achievement.read(token_id)
        }

        fn add_minter(ref self: ContractState, minter: ContractAddress) {
            self.ownable.assert_only_owner();
            self.minters.write(minter, true);
            self.emit(MinterAdded { minter });
        }

        fn remove_minter(ref self: ContractState, minter: ContractAddress) {
            self.ownable.assert_only_owner();
            self.minters.write(minter, false);
            self.emit(MinterRemoved { minter });
        }

        fn is_minter(self: @ContractState, account: ContractAddress) -> bool {
            self.minters.read(account)
        }
    }

    #[generate_trait]
    impl InternalFunctions of InternalFunctionsTrait {
        fn set_achievement_metadata(
            ref self: ContractState, achievement_id: felt252, metadata: ByteArray,
        ) {
            self.ownable.assert_only_owner();
            self.achievement_metadata.write(achievement_id, metadata);
        }

        fn get_achievement_metadata(self: @ContractState, achievement_id: felt252) -> ByteArray {
            self.achievement_metadata.read(achievement_id)
        }
    }
}
