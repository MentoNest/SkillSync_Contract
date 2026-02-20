#![no_std]

use soroban_sdk::{Address, Env};

pub fn init(env: Env, admin: Address) {
    let _ = (env, admin);
}

pub fn ping(env: Env) -> u32 {
    let _ = env;
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ping() {
        let env = Env::default();
        assert_eq!(ping(env), 1);
    }
}
