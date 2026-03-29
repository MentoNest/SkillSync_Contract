use soroban_sdk::{Env, Address};

use crate::SkillSync;

#[test]
fn test_default_reputation() {
    let env = Env::default();
    let mentor = Address::random(&env);

    let rep = SkillSync::get_reputation(env.clone(), mentor);
    assert_eq!(rep, 0);
}

#[test]
fn test_increment() {
    let env = Env::default();
    let admin = Address::random(&env);
    let mentor = Address::random(&env);

    env.storage().instance().set(&"admin", &admin);

    SkillSync::increment_reputation(env.clone(), admin.clone(), mentor.clone(), 10);

    let rep = SkillSync::get_reputation(env.clone(), mentor);
    assert_eq!(rep, 10);
}

#[test]
fn test_decrement_floor() {
    let env = Env::default();
    let admin = Address::random(&env);
    let mentor = Address::random(&env);

    env.storage().instance().set(&"admin", &admin);

    SkillSync::increment_reputation(env.clone(), admin.clone(), mentor.clone(), 5);
    SkillSync::decrement_reputation(env.clone(), admin.clone(), mentor.clone(), 10);

    let rep = SkillSync::get_reputation(env.clone(), mentor);
    assert_eq!(rep, 0);
}

#[test]
fn test_overflow_protection() {
    let env = Env::default();
    let admin = Address::random(&env);
    let mentor = Address::random(&env);

    env.storage().instance().set(&"admin", &admin);

    SkillSync::increment_reputation(env.clone(), admin.clone(), mentor.clone(), u32::MAX);

    let result = std::panic::catch_unwind(|| {
        SkillSync::increment_reputation(env.clone(), admin.clone(), mentor.clone(), 1);
    });

    assert!(result.is_err());
}
