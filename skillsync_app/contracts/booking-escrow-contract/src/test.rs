#![cfg(test)]

extern crate std;

use crate::{BookingEscrowContract, BookingEscrowContractClient, EscrowError, EscrowStatus};
use soroban_sdk::{
    testutils::{Address as _, AuthorizedFunction, AuthorizedInvocation},
    token, Address, Env, IntoVal, Symbol,
};
use token::Client as TokenClient;
use token::StellarAssetClient;

fn create_token_contract<'a>(
    env: &Env,
    admin: &Address,
) -> (TokenClient<'a>, StellarAssetClient<'a>) {
    let addr = env.register_stellar_asset_contract_v2(admin.clone());
    let token = TokenClient::new(env, &addr.address());
    let asset_client = StellarAssetClient::new(env, &addr.address());
    (token, asset_client)
}

fn create_escrow_contract(env: &Env) -> BookingEscrowContractClient<'_> {
    let contract_id = env.register(BookingEscrowContract, ());
    BookingEscrowContractClient::new(env, &contract_id)
}

// ============================================
// INITIALIZATION TESTS
// ============================================

#[test]
fn test_init_success() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    assert_eq!(escrow.get_admin(), admin);
}

#[test]
fn test_init_already_initialized() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    // Try to initialize again - should fail
    let result = escrow.try_init(&admin2);
    assert_eq!(result, Err(Ok(EscrowError::AlreadyInitialized)));
}

// ============================================
// FUND TESTS
// ============================================

#[test]
fn test_fund_success() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentee = Address::generate(&env);
    let mentor = Address::generate(&env);

    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    let amount: i128 = 1_000_0000000; // 1000 tokens with 7 decimals
    token_admin.mint(&mentee, &amount);

    // Fund escrow
    escrow.fund(&1u64, &mentee, &mentor, &token.address, &amount);

    // Verify escrow data
    let escrow_data = escrow.get(&1u64);
    assert_eq!(escrow_data.booking_id, 1u64);
    assert_eq!(escrow_data.mentee, mentee);
    assert_eq!(escrow_data.mentor, mentor);
    assert_eq!(escrow_data.token, token.address);
    assert_eq!(escrow_data.amount, amount);
    assert_eq!(escrow_data.status, EscrowStatus::Funded);

    // Verify balances
    assert_eq!(token.balance(&mentee), 0);
    assert_eq!(token.balance(&escrow.address), amount);
}

#[test]
fn test_fund_requires_mentee_auth() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentee = Address::generate(&env);
    let mentor = Address::generate(&env);

    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    let amount: i128 = 1000;
    token_admin.mint(&mentee, &amount);

    escrow.fund(&1u64, &mentee, &mentor, &token.address, &amount);

    // Verify mentee auth was required
    assert_eq!(
        env.auths(),
        std::vec![(
            mentee.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    escrow.address.clone(),
                    Symbol::new(&env, "fund"),
                    (1u64, mentee.clone(), mentor.clone(), token.address.clone(), amount).into_val(&env),
                )),
                sub_invocations: std::vec![AuthorizedInvocation {
                    function: AuthorizedFunction::Contract((
                        token.address.clone(),
                        Symbol::new(&env, "transfer"),
                        (mentee.clone(), escrow.address.clone(), amount).into_val(&env),
                    )),
                    sub_invocations: std::vec![],
                }],
            }
        )]
    );
}

#[test]
fn test_fund_double_fund_same_booking_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentee = Address::generate(&env);
    let mentor = Address::generate(&env);

    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    let amount: i128 = 1000;
    token_admin.mint(&mentee, &(amount * 2));

    // First fund succeeds
    escrow.fund(&1u64, &mentee, &mentor, &token.address, &amount);

    // Second fund with same booking_id should fail
    let result = escrow.try_fund(&1u64, &mentee, &mentor, &token.address, &amount);
    assert_eq!(result, Err(Ok(EscrowError::EscrowAlreadyExists)));
}

#[test]
fn test_fund_zero_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentee = Address::generate(&env);
    let mentor = Address::generate(&env);

    let (token, _) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    let result = escrow.try_fund(&1u64, &mentee, &mentor, &token.address, &0i128);
    assert_eq!(result, Err(Ok(EscrowError::InvalidAmount)));
}

#[test]
fn test_fund_negative_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentee = Address::generate(&env);
    let mentor = Address::generate(&env);

    let (token, _) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    let result = escrow.try_fund(&1u64, &mentee, &mentor, &token.address, &-100i128);
    assert_eq!(result, Err(Ok(EscrowError::InvalidAmount)));
}

#[test]
fn test_fund_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();

    let mentee = Address::generate(&env);
    let mentor = Address::generate(&env);
    let admin = Address::generate(&env);

    let (token, _) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);

    // Don't initialize - try to fund
    let result = escrow.try_fund(&1u64, &mentee, &mentor, &token.address, &1000i128);
    assert_eq!(result, Err(Ok(EscrowError::NotInitialized)));
}

// ============================================
// RELEASE TESTS
// ============================================

#[test]
fn test_release_success() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentee = Address::generate(&env);
    let mentor = Address::generate(&env);

    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    let amount: i128 = 1000;
    token_admin.mint(&mentee, &amount);

    // Fund escrow
    escrow.fund(&1u64, &mentee, &mentor, &token.address, &amount);

    // Verify initial balances
    assert_eq!(token.balance(&mentor), 0);
    assert_eq!(token.balance(&escrow.address), amount);

    // Release to mentor
    escrow.release(&1u64);

    // Verify final balances
    assert_eq!(token.balance(&mentor), amount);
    assert_eq!(token.balance(&escrow.address), 0);

    // Verify status
    assert_eq!(escrow.status(&1u64), EscrowStatus::Released);
}

#[test]
fn test_release_idempotency() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentee = Address::generate(&env);
    let mentor = Address::generate(&env);

    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    let amount: i128 = 1000;
    token_admin.mint(&mentee, &amount);

    escrow.fund(&1u64, &mentee, &mentor, &token.address, &amount);

    // Release once
    escrow.release(&1u64);

    // Try to release again - should fail
    let result = escrow.try_release(&1u64);
    assert_eq!(result, Err(Ok(EscrowError::EscrowAlreadyTerminal)));

    // Balance should still be correct (not double released)
    assert_eq!(token.balance(&mentor), amount);
}

#[test]
fn test_release_not_found() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    let result = escrow.try_release(&999u64);
    assert_eq!(result, Err(Ok(EscrowError::EscrowNotFound)));
}

#[test]
fn test_release_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentee = Address::generate(&env);
    let mentor = Address::generate(&env);

    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    let amount: i128 = 1000;
    token_admin.mint(&mentee, &amount);

    escrow.fund(&1u64, &mentee, &mentor, &token.address, &amount);

    // Clear previous auths
    env.auths();

    escrow.release(&1u64);

    // Verify admin auth was required for release
    let auths = env.auths();
    assert!(!auths.is_empty());
    assert_eq!(auths[0].0, admin);
}

// ============================================
// REFUND TESTS
// ============================================

#[test]
fn test_refund_success() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentee = Address::generate(&env);
    let mentor = Address::generate(&env);

    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    let amount: i128 = 1000;
    token_admin.mint(&mentee, &amount);

    // Fund escrow
    escrow.fund(&1u64, &mentee, &mentor, &token.address, &amount);

    // Verify initial balances
    assert_eq!(token.balance(&mentee), 0);
    assert_eq!(token.balance(&escrow.address), amount);

    // Refund to mentee
    escrow.refund(&1u64);

    // Verify final balances
    assert_eq!(token.balance(&mentee), amount);
    assert_eq!(token.balance(&escrow.address), 0);

    // Verify status
    assert_eq!(escrow.status(&1u64), EscrowStatus::Refunded);
}

#[test]
fn test_refund_idempotency() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentee = Address::generate(&env);
    let mentor = Address::generate(&env);

    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    let amount: i128 = 1000;
    token_admin.mint(&mentee, &amount);

    escrow.fund(&1u64, &mentee, &mentor, &token.address, &amount);

    // Refund once
    escrow.refund(&1u64);

    // Try to refund again - should fail
    let result = escrow.try_refund(&1u64);
    assert_eq!(result, Err(Ok(EscrowError::EscrowAlreadyTerminal)));

    // Balance should still be correct (not double refunded)
    assert_eq!(token.balance(&mentee), amount);
}

#[test]
fn test_refund_not_found() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    let result = escrow.try_refund(&999u64);
    assert_eq!(result, Err(Ok(EscrowError::EscrowNotFound)));
}

#[test]
fn test_refund_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentee = Address::generate(&env);
    let mentor = Address::generate(&env);

    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    let amount: i128 = 1000;
    token_admin.mint(&mentee, &amount);

    escrow.fund(&1u64, &mentee, &mentor, &token.address, &amount);

    // Clear previous auths
    env.auths();

    escrow.refund(&1u64);

    // Verify admin auth was required for refund
    let auths = env.auths();
    assert!(!auths.is_empty());
    assert_eq!(auths[0].0, admin);
}

// ============================================
// CROSS-STATE TESTS
// ============================================

#[test]
fn test_cannot_refund_after_release() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentee = Address::generate(&env);
    let mentor = Address::generate(&env);

    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    let amount: i128 = 1000;
    token_admin.mint(&mentee, &amount);

    escrow.fund(&1u64, &mentee, &mentor, &token.address, &amount);

    // Release
    escrow.release(&1u64);

    // Try to refund - should fail
    let result = escrow.try_refund(&1u64);
    assert_eq!(result, Err(Ok(EscrowError::EscrowAlreadyTerminal)));
}

#[test]
fn test_cannot_release_after_refund() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentee = Address::generate(&env);
    let mentor = Address::generate(&env);

    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    let amount: i128 = 1000;
    token_admin.mint(&mentee, &amount);

    escrow.fund(&1u64, &mentee, &mentor, &token.address, &amount);

    // Refund
    escrow.refund(&1u64);

    // Try to release - should fail
    let result = escrow.try_release(&1u64);
    assert_eq!(result, Err(Ok(EscrowError::EscrowAlreadyTerminal)));
}

// ============================================
// TOKEN DECIMAL TESTS
// ============================================

#[test]
fn test_token_with_zero_decimals() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentee = Address::generate(&env);
    let mentor = Address::generate(&env);

    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    let amount: i128 = 1000; // Whole units
    token_admin.mint(&mentee, &amount);

    escrow.fund(&1u64, &mentee, &mentor, &token.address, &amount);

    // Verify balance integrity
    assert_eq!(token.balance(&escrow.address), amount);

    // Release and verify
    escrow.release(&1u64);
    assert_eq!(token.balance(&mentor), amount);
}

#[test]
fn test_token_with_seven_decimals() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentee = Address::generate(&env);
    let mentor = Address::generate(&env);

    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    let amount: i128 = 1_234_5678901; // 1234.5678901 tokens (with 7 decimals)
    token_admin.mint(&mentee, &amount);

    escrow.fund(&1u64, &mentee, &mentor, &token.address, &amount);

    // Verify exact amount integrity
    let escrow_data = escrow.get(&1u64);
    assert_eq!(escrow_data.amount, amount);

    // Release and verify
    escrow.release(&1u64);
    assert_eq!(token.balance(&mentor), amount);
}

// ============================================
// GETTER TESTS
// ============================================

#[test]
fn test_get_escrow_details() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentee = Address::generate(&env);
    let mentor = Address::generate(&env);

    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    let amount: i128 = 1000;
    token_admin.mint(&mentee, &amount);

    escrow.fund(&1u64, &mentee, &mentor, &token.address, &amount);

    let escrow_data = escrow.get(&1u64);
    assert_eq!(escrow_data.booking_id, 1u64);
    assert_eq!(escrow_data.mentee, mentee);
    assert_eq!(escrow_data.mentor, mentor);
    assert_eq!(escrow_data.token, token.address);
    assert_eq!(escrow_data.amount, amount);
    assert_eq!(escrow_data.status, EscrowStatus::Funded);
}

#[test]
fn test_get_escrow_not_found() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    let result = escrow.try_get(&999u64);
    assert_eq!(result, Err(Ok(EscrowError::EscrowNotFound)));
}

#[test]
fn test_status_funded() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentee = Address::generate(&env);
    let mentor = Address::generate(&env);

    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    let amount: i128 = 1000;
    token_admin.mint(&mentee, &amount);

    escrow.fund(&1u64, &mentee, &mentor, &token.address, &amount);

    assert_eq!(escrow.status(&1u64), EscrowStatus::Funded);
}

#[test]
fn test_status_released() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentee = Address::generate(&env);
    let mentor = Address::generate(&env);

    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    let amount: i128 = 1000;
    token_admin.mint(&mentee, &amount);

    escrow.fund(&1u64, &mentee, &mentor, &token.address, &amount);
    escrow.release(&1u64);

    assert_eq!(escrow.status(&1u64), EscrowStatus::Released);
}

#[test]
fn test_status_refunded() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentee = Address::generate(&env);
    let mentor = Address::generate(&env);

    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    let amount: i128 = 1000;
    token_admin.mint(&mentee, &amount);

    escrow.fund(&1u64, &mentee, &mentor, &token.address, &amount);
    escrow.refund(&1u64);

    assert_eq!(escrow.status(&1u64), EscrowStatus::Refunded);
}

#[test]
fn test_status_not_found() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    let result = escrow.try_status(&999u64);
    assert_eq!(result, Err(Ok(EscrowError::EscrowNotFound)));
}

// ============================================
// MULTIPLE BOOKINGS TEST
// ============================================

#[test]
fn test_multiple_bookings() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mentee = Address::generate(&env);
    let mentor = Address::generate(&env);
    let mentor2 = Address::generate(&env);
    let mentor3 = Address::generate(&env);

    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);

    escrow.init(&admin);

    let amount1: i128 = 1000;
    let amount2: i128 = 2000;
    let amount3: i128 = 3000;
    token_admin.mint(&mentee, &(amount1 + amount2 + amount3));

    // Fund three different bookings
    escrow.fund(&1u64, &mentee, &mentor, &token.address, &amount1);
    escrow.fund(&2u64, &mentee, &mentor2, &token.address, &amount2);
    escrow.fund(&3u64, &mentee, &mentor3, &token.address, &amount3);

    // Verify all escrows exist
    assert_eq!(escrow.get(&1u64).amount, amount1);
    assert_eq!(escrow.get(&2u64).amount, amount2);
    assert_eq!(escrow.get(&3u64).amount, amount3);

    // Release one, refund another
    escrow.release(&1u64);
    escrow.refund(&2u64);

    // Verify states
    assert_eq!(escrow.status(&1u64), EscrowStatus::Released);
    assert_eq!(escrow.status(&2u64), EscrowStatus::Refunded);
    assert_eq!(escrow.status(&3u64), EscrowStatus::Funded);

    // Verify balances
    assert_eq!(token.balance(&mentor), amount1);
    assert_eq!(token.balance(&mentee), amount2);
    assert_eq!(token.balance(&escrow.address), amount3);
}
