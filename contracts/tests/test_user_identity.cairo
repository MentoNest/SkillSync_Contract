use snforge_std::{declare, ContractClassTrait, DeclareResultTrait, start_cheat_caller_address, stop_cheat_caller_address, start_cheat_block_timestamp, stop_cheat_block_timestamp};
use starknet::{ContractAddress, contract_address_const};
use skillsync_contracts::{IUserIdentityDispatcher, IUserIdentityDispatcherTrait, UserProfile};

fn ADMIN() -> ContractAddress {
    contract_address_const::<'admin'>()
}

fn USER1() -> ContractAddress {
    contract_address_const::<'user1'>()
}

fn USER2() -> ContractAddress {
    contract_address_const::<'user2'>()
}

fn deploy_user_identity() -> IUserIdentityDispatcher {
    let contract = declare("UserIdentity").unwrap().contract_class();
    let (contract_address, _) = contract.deploy(@array![]).unwrap();
    IUserIdentityDispatcher { contract_address }
}

#[test]
fn test_user_registration() {
    let user_identity = deploy_user_identity();
    
    start_cheat_caller_address(user_identity.contract_address, USER1());
    start_cheat_block_timestamp(user_identity.contract_address, 1000);
    
    let user_id = user_identity.register_user('alice', 'email_hash_123');
    
    assert(user_id == 1, 'User ID should be 1');
    assert(user_identity.is_user_registered(USER1()), 'User should be registered');
    assert(user_identity.get_user_count() == 1, 'User count should be 1');
    
    let profile = user_identity.get_user_profile(user_id);
    assert(profile.username == 'alice', 'Username should match');
    assert(profile.email_hash == 'email_hash_123', 'Email hash should match');
    assert(profile.address == USER1(), 'Address should match');
    assert(profile.reputation_score == 0, 'Initial reputation should be 0');
    assert(profile.registration_timestamp == 1000, 'Timestamp should match');
    assert(profile.is_active == true, 'User should be active');
    
    stop_cheat_caller_address(user_identity.contract_address);
    stop_cheat_block_timestamp(user_identity.contract_address);
}

#[test]
#[should_panic(expected: ('User already registered',))]
fn test_duplicate_registration() {
    let user_identity = deploy_user_identity();
    
    start_cheat_caller_address(user_identity.contract_address, USER1());
    
    user_identity.register_user('alice', 'email_hash_123');
    user_identity.register_user('bob', 'email_hash_456'); // Should panic
    
    stop_cheat_caller_address(user_identity.contract_address);
}

#[test]
#[should_panic(expected: ('Username already taken',))]
fn test_duplicate_username() {
    let user_identity = deploy_user_identity();
    
    // Register first user
    start_cheat_caller_address(user_identity.contract_address, USER1());
    user_identity.register_user('alice', 'email_hash_123');
    stop_cheat_caller_address(user_identity.contract_address);
    
    // Try to register second user with same username
    start_cheat_caller_address(user_identity.contract_address, USER2());
    user_identity.register_user('alice', 'email_hash_456'); // Should panic
    stop_cheat_caller_address(user_identity.contract_address);
}

#[test]
fn test_profile_update() {
    let user_identity = deploy_user_identity();
    
    start_cheat_caller_address(user_identity.contract_address, USER1());
    
    let user_id = user_identity.register_user('alice', 'email_hash_123');
    user_identity.update_profile(user_id, 'alice_updated');
    
    let profile = user_identity.get_user_profile(user_id);
    assert(profile.username == 'alice_updated', 'Username should be updated');
    
    stop_cheat_caller_address(user_identity.contract_address);
}

#[test]
#[should_panic(expected: ('Not profile owner',))]
fn test_unauthorized_profile_update() {
    let user_identity = deploy_user_identity();
    
    // Register user with USER1
    start_cheat_caller_address(user_identity.contract_address, USER1());
    let user_id = user_identity.register_user('alice', 'email_hash_123');
    stop_cheat_caller_address(user_identity.contract_address);
    
    // Try to update with USER2
    start_cheat_caller_address(user_identity.contract_address, USER2());
    user_identity.update_profile(user_id, 'hacked'); // Should panic
    stop_cheat_caller_address(user_identity.contract_address);
}

#[test]
fn test_multiple_users() {
    let user_identity = deploy_user_identity();
    
    // Register first user
    start_cheat_caller_address(user_identity.contract_address, USER1());
    let user_id1 = user_identity.register_user('alice', 'email_hash_123');
    stop_cheat_caller_address(user_identity.contract_address);
    
    // Register second user
    start_cheat_caller_address(user_identity.contract_address, USER2());
    let user_id2 = user_identity.register_user('bob', 'email_hash_456');
    stop_cheat_caller_address(user_identity.contract_address);
    
    assert(user_id1 == 1, 'First user ID should be 1');
    assert(user_id2 == 2, 'Second user ID should be 2');
    assert(user_identity.get_user_count() == 2, 'User count should be 2');
    
    let profile1 = user_identity.get_user_profile(user_id1);
    let profile2 = user_identity.get_user_profile(user_id2);
    
    assert(profile1.username == 'alice', 'First username should match');
    assert(profile2.username == 'bob', 'Second username should match');
}
