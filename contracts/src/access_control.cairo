use starknet::ContractAddress;

#[starknet::interface]
trait IAccessControl<TContractState> {
    fn grant_role(ref self: TContractState, role: felt252, account: ContractAddress);
    fn revoke_role(ref self: TContractState, role: felt252, account: ContractAddress);
    fn has_role(self: @TContractState, role: felt252, account: ContractAddress) -> bool;
    fn get_role_admin(self: @TContractState, role: felt252) -> felt252;
    fn set_role_admin(ref self: TContractState, role: felt252, admin_role: felt252);
}

#[starknet::contract]
mod AccessControl {
    use super::IAccessControl;
    use starknet::{ContractAddress, get_caller_address};
    use starknet::storage::{
        StoragePointerReadAccess, StoragePointerWriteAccess, StoragePathEntry, Map
    };

    const DEFAULT_ADMIN_ROLE: felt252 = 0;
    const MODERATOR_ROLE: felt252 = 'MODERATOR';
    const USER_ROLE: felt252 = 'USER';

    #[storage]
    struct Storage {
        roles: Map<(felt252, ContractAddress), bool>,
        role_admins: Map<felt252, felt252>,
    }

    #[event]
    #[derive(Drop, starknet::Event)]
    enum Event {
        RoleGranted: RoleGranted,
        RoleRevoked: RoleRevoked,
        RoleAdminChanged: RoleAdminChanged,
    }

    #[derive(Drop, starknet::Event)]
    struct RoleGranted {
        role: felt252,
        account: ContractAddress,
        sender: ContractAddress,
    }

    #[derive(Drop, starknet::Event)]
    struct RoleRevoked {
        role: felt252,
        account: ContractAddress,
        sender: ContractAddress,
    }

    #[derive(Drop, starknet::Event)]
    struct RoleAdminChanged {
        role: felt252,
        previous_admin_role: felt252,
        new_admin_role: felt252,
    }

    #[constructor]
    fn constructor(ref self: ContractState, admin: ContractAddress) {
        self.roles.entry((DEFAULT_ADMIN_ROLE, admin)).write(true);
    }

    #[abi(embed_v0)]
    impl AccessControlImpl of IAccessControl<ContractState> {
        fn grant_role(ref self: ContractState, role: felt252, account: ContractAddress) {
            let caller = get_caller_address();
            let admin_role = self.get_role_admin(role);
            assert(self.has_role(admin_role, caller), 'AccessControl: unauthorized');
            
            if !self.has_role(role, account) {
                self.roles.entry((role, account)).write(true);
                self.emit(RoleGranted { role, account, sender: caller });
            }
        }

        fn revoke_role(ref self: ContractState, role: felt252, account: ContractAddress) {
            let caller = get_caller_address();
            let admin_role = self.get_role_admin(role);
            assert(self.has_role(admin_role, caller), 'AccessControl: unauthorized');
            
            if self.has_role(role, account) {
                self.roles.entry((role, account)).write(false);
                self.emit(RoleRevoked { role, account, sender: caller });
            }
        }

        fn has_role(self: @ContractState, role: felt252, account: ContractAddress) -> bool {
            self.roles.entry((role, account)).read()
        }

        fn get_role_admin(self: @ContractState, role: felt252) -> felt252 {
            self.role_admins.entry(role).read()
        }

        fn set_role_admin(ref self: ContractState, role: felt252, admin_role: felt252) {
            let caller = get_caller_address();
            assert(self.has_role(DEFAULT_ADMIN_ROLE, caller), 'AccessControl: unauthorized');
            
            let previous_admin_role = self.get_role_admin(role);
            self.role_admins.entry(role).write(admin_role);
            
            self.emit(RoleAdminChanged { role, previous_admin_role, new_admin_role: admin_role });
        }
    }
}
