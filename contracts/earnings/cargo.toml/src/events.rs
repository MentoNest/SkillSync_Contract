use soroban_sdk::{symbol_short, Address, Env};

pub fn emit_credited(
    env: &Env,
    mentor: &Address,
    token: &Address,
    amount: u128,
    booking_id: u64,
) {
    env.events().publish(
        (symbol_short!("credited"), mentor, token),
        (amount, booking_id),
    );
}
