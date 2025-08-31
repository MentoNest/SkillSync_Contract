use snforge_std::{declare, ContractClassTrait, DeclareResultTrait, start_cheat_caller_address, stop_cheat_caller_address, start_cheat_block_timestamp, stop_cheat_block_timestamp};
use starknet::{ContractAddress, contract_address_const};
use skillsync_contracts::{IAchievementNFTDispatcher, IAchievementNFTDispatcherTrait, AchievementMetadata};

fn OWNER() -> ContractAddress {
    contract_address_const::<'owner'>()
}

fn VERIFIER() -> ContractAddress {
    contract_address_const::<'verifier'>()
}

fn USER1() -> ContractAddress {
    contract_address_const::<'user1'>()
}

fn USER2() -> ContractAddress {
    contract_address_const::<'user2'>()
}

fn UNAUTHORIZED() -> ContractAddress {
    contract_address_const::<'unauthorized'>()
}

const SKILL_COMPLETION: felt252 = 'SKILL_COMPLETION';
const PROJECT_COMPLETION: felt252 = 'PROJECT_COMPLETION';
const CERTIFICATION: felt252 = 'CERTIFICATION';
const INVALID_TYPE: felt252 = 'INVALID_TYPE';

fn deploy_achievement_nft() -> IAchievementNFTDispatcher {
    let contract = declare("AchievementNFT").unwrap().contract_class();
    let (contract_address, _) = contract.deploy(@array![OWNER().into(), VERIFIER().into()]).unwrap();
    IAchievementNFTDispatcher { contract_address }
}

#[test]
fn test_mint_achievement() {
    let achievement_nft = deploy_achievement_nft();
    
    start_cheat_caller_address(achievement_nft.contract_address, VERIFIER());
    start_cheat_block_timestamp(achievement_nft.contract_address, 1000);
    
    let token_id = achievement_nft.mint_achievement(USER1(), SKILL_COMPLETION, 'metadata_uri_123');
    
    assert(token_id == 1, 'Token ID should be 1');
    assert(achievement_nft.owner_of(token_id) == USER1(), 'Owner should be USER1');
    assert(achievement_nft.balance_of(USER1()) == 1, 'Balance should be 1');
    assert(achievement_nft.get_total_supply() == 1, 'Total supply should be 1');
    
    let metadata = achievement_nft.get_achievement_metadata(token_id);
    assert(metadata.achievement_type == SKILL_COMPLETION, 'Achievement type should match');
    assert(metadata.metadata_uri == 'metadata_uri_123', 'Metadata URI should match');
    assert(metadata.issued_timestamp == 1000, 'Timestamp should match');
    assert(metadata.issuer == VERIFIER(), 'Issuer should be verifier');
    assert(metadata.is_transferable == false, 'Should be non-transferable');
    
    stop_cheat_caller_address(achievement_nft.contract_address);
    stop_cheat_block_timestamp(achievement_nft.contract_address);
}

#[test]
fn test_owner_can_mint() {
    let achievement_nft = deploy_achievement_nft();
    
    start_cheat_caller_address(achievement_nft.contract_address, OWNER());
    
    let token_id = achievement_nft.mint_achievement(USER1(), PROJECT_COMPLETION, 'project_metadata');
    
    assert(token_id == 1, 'Token ID should be 1');
    assert(achievement_nft.owner_of(token_id) == USER1(), 'Owner should be USER1');
    
    stop_cheat_caller_address(achievement_nft.contract_address);
}

#[test]
#[should_panic(expected: ('Unauthorized minter',))]
fn test_unauthorized_mint() {
    let achievement_nft = deploy_achievement_nft();
    
    start_cheat_caller_address(achievement_nft.contract_address, UNAUTHORIZED());
    
    achievement_nft.mint_achievement(USER1(), SKILL_COMPLETION, 'metadata'); // Should panic
    
    stop_cheat_caller_address(achievement_nft.contract_address);
}

#[test]
#[should_panic(expected: ('Invalid achievement type',))]
fn test_invalid_achievement_type() {
    let achievement_nft = deploy_achievement_nft();
    
    start_cheat_caller_address(achievement_nft.contract_address, VERIFIER());
    
    achievement_nft.mint_achievement(USER1(), INVALID_TYPE, 'metadata'); // Should panic
    
    stop_cheat_caller_address(achievement_nft.contract_address);
}

#[test]
fn test_multiple_achievements() {
    let achievement_nft = deploy_achievement_nft();
    
    start_cheat_caller_address(achievement_nft.contract_address, VERIFIER());
    
    let token_id1 = achievement_nft.mint_achievement(USER1(), SKILL_COMPLETION, 'metadata1');
    let token_id2 = achievement_nft.mint_achievement(USER1(), PROJECT_COMPLETION, 'metadata2');
    let token_id3 = achievement_nft.mint_achievement(USER2(), CERTIFICATION, 'metadata3');
    
    assert(token_id1 == 1, 'First token ID should be 1');
    assert(token_id2 == 2, 'Second token ID should be 2');
    assert(token_id3 == 3, 'Third token ID should be 3');
    
    assert(achievement_nft.balance_of(USER1()) == 2, 'USER1 should have 2 achievements');
    assert(achievement_nft.balance_of(USER2()) == 1, 'USER2 should have 1 achievement');
    assert(achievement_nft.get_total_supply() == 3, 'Total supply should be 3');
    
    stop_cheat_caller_address(achievement_nft.contract_address);
}

#[test]
fn test_get_user_achievements() {
    let achievement_nft = deploy_achievement_nft();
    
    start_cheat_caller_address(achievement_nft.contract_address, VERIFIER());
    
    let token_id1 = achievement_nft.mint_achievement(USER1(), SKILL_COMPLETION, 'metadata1');
    let token_id2 = achievement_nft.mint_achievement(USER1(), PROJECT_COMPLETION, 'metadata2');
    
    let user_achievements = achievement_nft.get_user_achievements(USER1());
    assert(user_achievements.len() == 2, 'Should have 2 achievements');
    assert(*user_achievements.at(0) == token_id1, 'First achievement should match');
    assert(*user_achievements.at(1) == token_id2, 'Second achievement should match');
    
    stop_cheat_caller_address(achievement_nft.contract_address);
}

#[test]
fn test_verify_achievement() {
    let achievement_nft = deploy_achievement_nft();
    
    start_cheat_caller_address(achievement_nft.contract_address, VERIFIER());
    
    achievement_nft.mint_achievement(USER1(), SKILL_COMPLETION, 'metadata1');
    achievement_nft.mint_achievement(USER1(), PROJECT_COMPLETION, 'metadata2');
    
    assert(achievement_nft.verify_achievement(USER1(), SKILL_COMPLETION), 'Should have SKILL_COMPLETION');
    assert(achievement_nft.verify_achievement(USER1(), PROJECT_COMPLETION), 'Should have PROJECT_COMPLETION');
    assert(!achievement_nft.verify_achievement(USER1(), CERTIFICATION), 'Should not have CERTIFICATION');
    assert(!achievement_nft.verify_achievement(USER2(), SKILL_COMPLETION), 'USER2 should not have any achievements');
    
    stop_cheat_caller_address(achievement_nft.contract_address);
}

#[test]
fn test_burn_achievement() {
    let achievement_nft = deploy_achievement_nft();
    
    start_cheat_caller_address(achievement_nft.contract_address, VERIFIER());
    
    let token_id = achievement_nft.mint_achievement(USER1(), SKILL_COMPLETION, 'metadata');
    assert(achievement_nft.balance_of(USER1()) == 1, 'Balance should be 1 before burn');
    
    achievement_nft.burn_achievement(token_id);
    assert(achievement_nft.balance_of(USER1()) == 0, 'Balance should be 0 after burn');
    
    stop_cheat_caller_address(achievement_nft.contract_address);
}

#[test]
fn test_owner_can_burn() {
    let achievement_nft = deploy_achievement_nft();
    
    // Mint as verifier
    start_cheat_caller_address(achievement_nft.contract_address, VERIFIER());
    let token_id = achievement_nft.mint_achievement(USER1(), SKILL_COMPLETION, 'metadata');
    stop_cheat_caller_address(achievement_nft.contract_address);
    
    // Burn as owner
    start_cheat_caller_address(achievement_nft.contract_address, OWNER());
    achievement_nft.burn_achievement(token_id);
    assert(achievement_nft.balance_of(USER1()) == 0, 'Balance should be 0 after burn');
    stop_cheat_caller_address(achievement_nft.contract_address);
}

#[test]
fn test_token_owner_can_burn() {
    let achievement_nft = deploy_achievement_nft();
    
    // Mint as verifier
    start_cheat_caller_address(achievement_nft.contract_address, VERIFIER());
    let token_id = achievement_nft.mint_achievement(USER1(), SKILL_COMPLETION, 'metadata');
    stop_cheat_caller_address(achievement_nft.contract_address);
    
    // Burn as token owner
    start_cheat_caller_address(achievement_nft.contract_address, USER1());
    achievement_nft.burn_achievement(token_id);
    assert(achievement_nft.balance_of(USER1()) == 0, 'Balance should be 0 after burn');
    stop_cheat_caller_address(achievement_nft.contract_address);
}

#[test]
#[should_panic(expected: ('Unauthorized burn',))]
fn test_unauthorized_burn() {
    let achievement_nft = deploy_achievement_nft();
    
    // Mint as verifier
    start_cheat_caller_address(achievement_nft.contract_address, VERIFIER());
    let token_id = achievement_nft.mint_achievement(USER1(), SKILL_COMPLETION, 'metadata');
    stop_cheat_caller_address(achievement_nft.contract_address);
    
    // Try to burn as unauthorized user
    start_cheat_caller_address(achievement_nft.contract_address, UNAUTHORIZED());
    achievement_nft.burn_achievement(token_id); // Should panic
    stop_cheat_caller_address(achievement_nft.contract_address);
}

#[test]
fn test_set_verifier() {
    let achievement_nft = deploy_achievement_nft();
    let new_verifier = contract_address_const::<'new_verifier'>();
    
    start_cheat_caller_address(achievement_nft.contract_address, OWNER());
    
    achievement_nft.set_achievement_verifier(new_verifier);
    
    stop_cheat_caller_address(achievement_nft.contract_address);
    
    // Test that new verifier can mint
    start_cheat_caller_address(achievement_nft.contract_address, new_verifier);
    
    let token_id = achievement_nft.mint_achievement(USER1(), SKILL_COMPLETION, 'metadata');
    assert(token_id == 1, 'New verifier should be able to mint');
    
    stop_cheat_caller_address(achievement_nft.contract_address);
}

#[test]
#[should_panic(expected: ('Only owner can set verifier',))]
fn test_unauthorized_set_verifier() {
    let achievement_nft = deploy_achievement_nft();
    let new_verifier = contract_address_const::<'new_verifier'>();
    
    start_cheat_caller_address(achievement_nft.contract_address, UNAUTHORIZED());
    
    achievement_nft.set_achievement_verifier(new_verifier); // Should panic
    
    stop_cheat_caller_address(achievement_nft.contract_address);
}

#[test]
#[should_panic(expected: ('Token does not exist',))]
fn test_owner_of_nonexistent_token() {
    let achievement_nft = deploy_achievement_nft();
    
    achievement_nft.owner_of(999); // Should panic
}

#[test]
fn test_balance_of_zero() {
    let achievement_nft = deploy_achievement_nft();
    
    assert(achievement_nft.balance_of(USER1()) == 0, 'Initial balance should be 0');
    assert(achievement_nft.balance_of(USER2()) == 0, 'Initial balance should be 0');
}
