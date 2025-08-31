use starknet::ContractAddress;

#[starknet::interface]
trait IUserIdentity<TContractState> {
    fn register_user(ref self: TContractState, username: felt252, email_hash: felt252) -> u256;
    fn get_user_profile(self: @TContractState, user_id: u256) -> UserProfile;
    fn update_profile(ref self: TContractState, user_id: u256, new_username: felt252);
    fn is_user_registered(self: @TContractState, address: ContractAddress) -> bool;
    fn get_user_count(self: @TContractState) -> u256;
}

#[derive(Drop, Serde, starknet::Store)]
struct UserProfile {
    user_id: u256,
    address: ContractAddress,
    username: felt252,
    email_hash: felt252,
    reputation_score: u64,
    registration_timestamp: u64,
    is_active: bool,
}

#[starknet::contract]
mod UserIdentity {
    use super::{IUserIdentity, UserProfile};
    use starknet::{ContractAddress, get_caller_address, get_block_timestamp};
    use starknet::storage::{
        StoragePointerReadAccess, StoragePointerWriteAccess, StoragePathEntry, Map
    };

    #[storage]
    struct Storage {
        users: Map<u256, UserProfile>,
        address_to_user_id: Map<ContractAddress, u256>,
        user_counter: u256,
        usernames: Map<felt252, bool>, // Track unique usernames
    }

    #[event]
    #[derive(Drop, starknet::Event)]
    enum Event {
        UserRegistered: UserRegistered,
        ProfileUpdated: ProfileUpdated,
    }

    #[derive(Drop, starknet::Event)]
    struct UserRegistered {
        user_id: u256,
        address: ContractAddress,
        username: felt252,
    }

    #[derive(Drop, starknet::Event)]
    struct ProfileUpdated {
        user_id: u256,
        new_username: felt252,
    }

    #[abi(embed_v0)]
    impl UserIdentityImpl of IUserIdentity<ContractState> {
        fn register_user(ref self: ContractState, username: felt252, email_hash: felt252) -> u256 {
            let caller = get_caller_address();
            
            // Check if user already registered
            assert(!self.is_user_registered(caller), 'User already registered');
            
            // Check if username is taken
            assert(!self.usernames.entry(username).read(), 'Username already taken');
            
            let user_id = self.user_counter.read() + 1;
            self.user_counter.write(user_id);
            
            let profile = UserProfile {
                user_id,
                address: caller,
                username,
                email_hash,
                reputation_score: 0,
                registration_timestamp: get_block_timestamp(),
                is_active: true,
            };
            
            self.users.entry(user_id).write(profile);
            self.address_to_user_id.entry(caller).write(user_id);
            self.usernames.entry(username).write(true);
            
            self.emit(UserRegistered { user_id, address: caller, username });
            
            user_id
        }

        fn get_user_profile(self: @ContractState, user_id: u256) -> UserProfile {
            self.users.entry(user_id).read()
        }

        fn update_profile(ref self: ContractState, user_id: u256, new_username: felt252) {
            let caller = get_caller_address();
            let mut profile = self.users.entry(user_id).read();
            
            // Verify ownership
            assert(profile.address == caller, 'Not profile owner');
            assert(!self.usernames.entry(new_username).read(), 'Username already taken');
            
            // Free old username
            self.usernames.entry(profile.username).write(false);
            
            // Update profile
            profile.username = new_username;
            self.users.entry(user_id).write(profile);
            self.usernames.entry(new_username).write(true);
            
            self.emit(ProfileUpdated { user_id, new_username });
        }

        fn is_user_registered(self: @ContractState, address: ContractAddress) -> bool {
            let user_id = self.address_to_user_id.entry(address).read();
            user_id != 0
        }

        fn get_user_count(self: @ContractState) -> u256 {
            self.user_counter.read()
        }
    }
}
