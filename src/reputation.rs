use soroban_sdk::{Address, Env, Symbol};

use crate::storage::{get_rep, set_rep};

// 🔐 simple admin check (replace with real ACL if needed)
fn require_admin(e: &Env, caller: &Address) {
    let admin: Address = e.storage().instance().get(&Symbol::short("admin")).unwrap();
    if &admin != caller {
        panic!("Unauthorized");
    }
}

// #93 READ
pub fn get_reputation(e: Env, mentor: Address) -> u32 {
    get_rep(&e, &mentor)
}

// #91 INCREMENT
pub fn increment_reputation(e: Env, caller: Address, mentor: Address, amount: u32) {
    require_admin(&e, &caller);

    let current = get_rep(&e, &mentor);

    let updated = current
        .checked_add(amount)
        .expect("Overflow");

    set_rep(&e, &mentor, updated);

    e.events().publish(
        (Symbol::short("rep_inc"), mentor),
        updated,
    );
}

// #92 DECREMENT
pub fn decrement_reputation(e: Env, caller: Address, mentor: Address, amount: u32) {
    require_admin(&e, &caller);

    let current = get_rep(&e, &mentor);

    let updated = current
        .checked_sub(amount)
        .unwrap_or(0); // floor at 0

    set_rep(&e, &mentor, updated);

    e.events().publish(
        (Symbol::short("rep_dec"), mentor),
        updated,
    );
}
