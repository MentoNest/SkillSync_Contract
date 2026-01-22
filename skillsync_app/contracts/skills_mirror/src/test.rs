#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String, Symbol, Vec};

#[test]
fn test_skill_lifecycle() {
    // 1. Setup environment
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, SkillsTaxonomy);
    let client = SkillsTaxonomyClient::new(&env, &contract_id);

    // 2. Setup admin
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin);

    // 3. Admin adds skills
    env.set_caller(admin.clone());
    client.add_skill(&Symbol::short("rust"), &String::from_str(&env, "Rust Programming"));
    client.add_skill(&Symbol::short("solidity"), &String::from_str(&env, "Solidity Development"));
    client.add_skill(&Symbol::short("python"), &String::from_str(&env, "Python Programming"));

    // Verify skills are stored
    assert_eq!(
        client.get_skill(&Symbol::short("rust")).unwrap(),
        String::from_str(&env, "Rust Programming")
    );
    assert_eq!(
        client.get_skill(&Symbol::short("solidity")).unwrap(),
        String::from_str(&env, "Solidity Development")
    );
    assert_eq!(
        client.get_skill(&Symbol::short("python")).unwrap(),
        String::from_str(&env, "Python Programming")
    );

    // 4. List skills with pagination
    let first_page = client.list(&0, &2); // page 0, limit 2
    assert_eq!(first_page.len(), 2);
    let second_page = client.list(&1, &2); // page 1, limit 2
    assert_eq!(second_page.len(), 1);

    // 5. Rename a skill
    client.rename_skill(
        &Symbol::short("rust"),
        &String::from_str(&env, "Rust Language"),
    );
    assert_eq!(
        client.get_skill(&Symbol::short("rust")).unwrap(),
        String::from_str(&env, "Rust Language")
    );

    // 6. Remove a skill
    client.remove_skill(&Symbol::short("python"));
    assert!(client.get_skill(&Symbol::short("python")).is_none());

    // Verify list after removal
    let all_skills = client.list(&0, &10);
    assert_eq!(all_skills.len(), 2); // rust + solidity
}

#[test]
#[should_panic(expected = "Already initialized")]
fn test_already_initialized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SkillsTaxonomy);
    let client = SkillsTaxonomyClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);
    client.initialize(&admin); // Should panic
}

#[test]
#[should_panic(expected = "Skill already exists")]
fn test_add_existing_skill() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, SkillsTaxonomy);
    let client = SkillsTaxonomyClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    env.set_caller(admin.clone());
    client.add_skill(&Symbol::short("rust"), &String::from_str(&env, "Rust Programming"));

    // Adding again should panic
    client.add_skill(&Symbol::short("rust"), &String::from_str(&env, "Rust Language"));
}

#[test]
#[should_panic(expected = "Skill does not exist")]
fn test_remove_nonexistent_skill() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, SkillsTaxonomy);
    let client = SkillsTaxonomyClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    env.set_caller(admin.clone());
    client.remove_skill(&Symbol::short("nonexistent")); // Should panic
}

#[test]
#[should_panic(expected = "Skill does not exist")]
fn test_rename_nonexistent_skill() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, SkillsTaxonomy);
    let client = SkillsTaxonomyClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    env.set_caller(admin.clone());
    client.rename_skill(
        &Symbol::short("nonexistent"),
        &String::from_str(&env, "New Name"),
    ); // Should panic
}

#[test]
#[should_panic(expected = "auth failed")]
fn test_unauthorized_mutation() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, SkillsTaxonomy);
    let client = SkillsTaxonomyClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    client.initialize(&admin);

    // Non-admin caller tries to add a skill
    env.set_caller(user.clone());
    client.add_skill(&Symbol::short("go"), &String::from_str(&env, "Go Programming"));
}
