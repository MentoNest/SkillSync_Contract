#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String, symbol_short, vec};

#[test]
fn test_skill_lifecycle() {
    let env = Env::default();

    // Generate admin address and mock auth
    let admin = Address::generate(&env);
    env.mock_auth(&admin, true);

    // Register contract and client
    let contract_id = env.register_contract(None, SkillsTaxonomy);
    let client = SkillsTaxonomyClient::new(&env, &contract_id);

    // Initialize
    client.initialize(&admin);

    // Add skills
    client.add_skill(&symbol_short!("rust"), &String::from_str(&env, "Rust Programming"));
    client.add_skill(&symbol_short!("solidity"), &String::from_str(&env, "Solidity Development"));
    client.add_skill(&symbol_short!("python"), &String::from_str(&env, "Python Programming"));

    // Verify skills
    assert_eq!(
        client.get_skill(&symbol_short!("rust")).unwrap(),
        String::from_str(&env, "Rust Programming")
    );
    assert_eq!(
        client.get_skill(&symbol_short!("solidity")).unwrap(),
        String::from_str(&env, "Solidity Development")
    );
    assert_eq!(
        client.get_skill(&symbol_short!("python")).unwrap(),
        String::from_str(&env, "Python Programming")
    );

    // List skills with pagination
    let first_page = client.list(0, 2);
    assert_eq!(first_page.len(), 2);
    let second_page = client.list(1, 2);
    assert_eq!(second_page.len(), 1);

    // Rename a skill
    client.rename_skill(&symbol_short!("rust"), &String::from_str(&env, "Rust Language"));
    assert_eq!(
        client.get_skill(&symbol_short!("rust")).unwrap(),
        String::from_str(&env, "Rust Language")
    );

    // Remove a skill
    client.remove_skill(&symbol_short!("python"));
    assert!(client.get_skill(&symbol_short!("python")).is_none());

    // Verify remaining skills
    let all_skills = client.list(0, 10);
    assert_eq!(all_skills.len(), 2);
}

#[test]
#[should_panic(expected = "Already initialized")]
fn test_already_initialized() {
    let env = Env::default();
    let admin = Address::generate(&env);
    env.mock_auth(&admin, true);

    let contract_id = env.register_contract(None, SkillsTaxonomy);
    let client = SkillsTaxonomyClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.initialize(&admin); // should panic
}

#[test]
#[should_panic(expected = "Skill already exists")]
fn test_add_existing_skill() {
    let env = Env::default();
    let admin = Address::generate(&env);
    env.mock_auth(&admin, true);

    let contract_id = env.register_contract(None, SkillsTaxonomy);
    let client = SkillsTaxonomyClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.add_skill(&symbol_short!("rust"), &String::from_str(&env, "Rust Programming"));
    client.add_skill(&symbol_short!("rust"), &String::from_str(&env, "Rust Language")); // should panic
}

#[test]
#[should_panic(expected = "Skill does not exist")]
fn test_remove_nonexistent_skill() {
    let env = Env::default();
    let admin = Address::generate(&env);
    env.mock_auth(&admin, true);

    let contract_id = env.register_contract(None, SkillsTaxonomy);
    let client = SkillsTaxonomyClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.remove_skill(&symbol_short!("noexist")); // should panic
}

#[test]
#[should_panic(expected = "Skill does not exist")]
fn test_rename_nonexistent_skill() {
    let env = Env::default();
    let admin = Address::generate(&env);
    env.mock_auth(&admin, true);

    let contract_id = env.register_contract(None, SkillsTaxonomy);
    let client = SkillsTaxonomyClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.rename_skill(&symbol_short!("noexist"), &String::from_str(&env, "New Name")); // should panic
}

#[test]
#[should_panic(expected = "auth failed")]
fn test_unauthorized_mutation() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_auth(&admin, true);
    env.mock_auth(&user, true);

    let contract_id = env.register_contract(None, SkillsTaxonomy);
    let client = SkillsTaxonomyClient::new(&env, &contract_id);

    client.initialize(&admin);

    // Non-admin tries to add skill
    env.mock_auth(&user, true);
    client.add_skill(&symbol_short!("go"), &String::from_str(&env, "Go Programming")); // should panic
}
