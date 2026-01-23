#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::{Address as _, MockAuth, MockAuthInvoke}, Address, Env, String, IntoVal};


#[test]
fn test_skill_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);

    let contract_id = env.register_contract(None, SkillsTaxonomy);
    let client = SkillsTaxonomyClient::new(&env, &contract_id);

    client.initialize(&admin);

    client.add_skill(&symbol_short!("rust"), &String::from_str(&env, "Rust Programming"));
    client.add_skill(&symbol_short!("solidity"), &String::from_str(&env, "Solidity Development"));
    client.add_skill(&symbol_short!("python"), &String::from_str(&env, "Python Programming"));

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

    let first_page = client.list(&0, &2);
    assert_eq!(first_page.len(), 2);
    let second_page = client.list(&1, &2);
    assert_eq!(second_page.len(), 1);

    client.rename_skill(&symbol_short!("rust"), &String::from_str(&env, "Rust Language"));
    assert_eq!(
        client.get_skill(&symbol_short!("rust")).unwrap(),
        String::from_str(&env, "Rust Language")
    );

    client.remove_skill(&symbol_short!("python"));
    assert!(client.get_skill(&symbol_short!("python")).is_none());

    let all_skills = client.list(&0, &10);
    assert_eq!(all_skills.len(), 2);
}

#[test]
#[should_panic(expected = "Already initialized")]
fn test_already_initialized() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);

    let contract_id = env.register_contract(None, SkillsTaxonomy);
    let client = SkillsTaxonomyClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.initialize(&admin);
}

#[test]
#[should_panic(expected = "Skill already exists")]
fn test_add_existing_skill() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);

    let contract_id = env.register_contract(None, SkillsTaxonomy);
    let client = SkillsTaxonomyClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.add_skill(&symbol_short!("rust"), &String::from_str(&env, "Rust Programming"));
    client.add_skill(&symbol_short!("rust"), &String::from_str(&env, "Rust Language"));
}

#[test]
#[should_panic(expected = "Skill does not exist")]
fn test_remove_nonexistent_skill() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);

    let contract_id = env.register_contract(None, SkillsTaxonomy);
    let client = SkillsTaxonomyClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.remove_skill(&symbol_short!("noexist"));
}

#[test]
#[should_panic(expected = "Skill does not exist")]
fn test_rename_nonexistent_skill() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);

    let contract_id = env.register_contract(None, SkillsTaxonomy);
    let client = SkillsTaxonomyClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.rename_skill(&symbol_short!("noexist"), &String::from_str(&env, "New Name"));
}

#[test]
#[should_panic(expected = "Not authorized")]
fn test_unauthorized_mutation() {
    let env = Env::default();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let contract_id = env.register_contract(None, SkillsTaxonomy);
    let client = SkillsTaxonomyClient::new(&env, &contract_id);

    // We only mock auth for the *admin* calling initialize
    env.mock_auths(&[
        MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "initialize",
                args: (&admin,).into_val(&env),
                sub_invokes: &[],
            },
        },
    ]);

    client.initialize(&admin);

    // No auth mocked for add_skill → should panic on admin.require_auth()
    client.add_skill(
        &symbol_short!("go"),
        &String::from_str(&env, "Go Programming"),
    );
}