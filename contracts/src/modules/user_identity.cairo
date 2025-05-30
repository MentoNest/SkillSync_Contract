use starknet::ContractAddress;

#[starknet::interface]
pub trait IUserIdentity<TContractState> {
    fn register_user(ref self: TContractState, user_id: felt252, address: ContractAddress);
    fn get_user_address(self: @TContractState, user_id: felt252) -> ContractAddress;
}

#[starknet::contract]
mod UserIdentity {
    use core::num::traits::Zero;
    use starknet::storage::{
        Map, StoragePathEntry, StoragePointerReadAccess, StoragePointerWriteAccess,
    };
    use starknet::ContractAddress;

    // Storage variables
    #[storage]
    struct Storage {
        registered_user: Map<felt252, ContractAddress>,
    }

   #[abi(embed_v0)]
    impl UserIdentityImpl of super::IUserIdentity<ContractState> {

        //This function is used to register a new user ID and associate it with a specific 
        // contract address in the contract's storage. 
        // It ensures that a user ID is only registered once.
        fn register_user(ref self: ContractState, user_id: felt252, address: ContractAddress){
            let user_exist = self.registered_user.entry(user_id).read();
            assert(user_exist.is_zero(), 'Already Registered');

            self.registered_user.entry(user_id).write(address);
        }

        // This function is used to retrieve the contract address that is currently registered for a given user ID.
        fn get_user_address(self: @ContractState, user_id: felt252) -> ContractAddress {
            let address = self.registered_user.entry(user_id).read();
            return address;
        }
    }
}
