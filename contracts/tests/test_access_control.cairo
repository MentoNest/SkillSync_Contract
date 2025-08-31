use snforge_std::{declare, ContractClassTrait, DeclareResultTrait, start_cheat_caller_address, stop_cheat_caller_address};
use starknet::{ContractAddress, contract_address_const};
use skillsync_contracts::{IAccessControlDispatcher, IAccessControlDispatcherTrait};

fn ADMIN() -> ContractAddress {
    contract_address_const::<'admin'>()
}

fn USER1() -> ContractAddress {
    contract_address_const::<'user1'>()
}

fn USER2() -> ContractAddress {
    contract_address_const::<'user2'>()
}

const DEFAULT_ADMIN_ROLE: felt252 = 0;
const MODERATOR_ROLE: felt252 = 'MODERATOR';
const USER_ROLE: felt252 = 'USER';

fn deploy_access_control() -> IAccessControlDispatcher {
    let contract = declare("AccessControl").unwrap().contract_class();
    let (contract_address, _) = contract.deploy(@array![ADMIN().into()]).unwrap();
    IAccessControlDispatcher { contract_address }
}

#[test]
fn test_initial_admin_role() {
    let access_control = deploy_access_control();
    
    assert(access_control.has_role(DEFAULT_ADMIN_ROLE, ADMIN()), 'Admin should have default role');
    assert(!access_control.has_role(DEFAULT_ADMIN_ROLE, USER1()), 'User1 should not have admin role');
}

#[test]
fn test_grant_role() {
    let access_control = deploy_access_control();
    
    start_cheat_caller_address(access_control.contract_address, ADMIN());
    
    access_control.grant_role(MODERATOR_ROLE, USER1());
    
    assert(access_control.has_role(MODERATOR_ROLE, USER1()), 'User1 should have moderator role');
    assert(!access_control.has_role(MODERATOR_ROLE, USER2()), 'User2 should not have moderator role');
    
    stop_cheat_caller_address(access_control.contract_address);
}

#[test]
fn test_revoke_role() {
    let access_control = deploy_access_control();
    
    start_cheat_caller_address(access_control.contract_address, ADMIN());
    
    // Grant then revoke
    access_control.grant_role(MODERATOR_ROLE, USER1());
    assert(access_control.has_role(MODERATOR_ROLE, USER1()), 'User1 should have moderator role');
    
    access_control.revoke_role(MODERATOR_ROLE, USER1());
    assert(!access_control.has_role(MODERATOR_ROLE, USER1()), 'User1 should not have moderator role');
    
    stop_cheat_caller_address(access_control.contract_address);
}

#[test]
#[should_panic(expected: ('AccessControl: unauthorized',))]
fn test_unauthorized_grant_role() {
    let access_control = deploy_access_control();
    
    start_cheat_caller_address(access_control.contract_address, USER1());
    
    access_control.grant_role(MODERATOR_ROLE, USER2()); // Should panic
    
    stop_cheat_caller_address(access_control.contract_address);
}

#[test]
#[should_panic(expected: ('AccessControl: unauthorized',))]
fn test_unauthorized_revoke_role() {
    let access_control = deploy_access_control();
    
    // First grant role as admin
    start_cheat_caller_address(access_control.contract_address, ADMIN());
    access_control.grant_role(MODERATOR_ROLE, USER2());
    stop_cheat_caller_address(access_control.contract_address);
    
    // Try to revoke as non-admin
    start_cheat_caller_address(access_control.contract_address, USER1());
    access_control.revoke_role(MODERATOR_ROLE, USER2()); // Should panic
    stop_cheat_caller_address(access_control.contract_address);
}

#[test]
fn test_role_admin_management() {
    let access_control = deploy_access_control();
    
    start_cheat_caller_address(access_control.contract_address, ADMIN());
    
    // Initially, DEFAULT_ADMIN_ROLE is admin of all roles
    assert(access_control.get_role_admin(MODERATOR_ROLE) == DEFAULT_ADMIN_ROLE, 'Default admin should be role admin');
    
    // Set new role admin
    access_control.set_role_admin(MODERATOR_ROLE, USER_ROLE);
    assert(access_control.get_role_admin(MODERATOR_ROLE) == USER_ROLE, 'Role admin should be updated');
    
    stop_cheat_caller_address(access_control.contract_address);
}

#[test]
fn test_hierarchical_role_management() {
    let access_control = deploy_access_control();
    
    start_cheat_caller_address(access_control.contract_address, ADMIN());
    
    // Set up role hierarchy: MODERATOR_ROLE can manage USER_ROLE
    access_control.set_role_admin(USER_ROLE, MODERATOR_ROLE);
    access_control.grant_role(MODERATOR_ROLE, USER1());
    
    stop_cheat_caller_address(access_control.contract_address);
    
    // Now USER1 (moderator) should be able to grant USER_ROLE
    start_cheat_caller_address(access_control.contract_address, USER1());
    
    access_control.grant_role(USER_ROLE, USER2());
    assert(access_control.has_role(USER_ROLE, USER2()), 'User2 should have user role');
    
    stop_cheat_caller_address(access_control.contract_address);
}
