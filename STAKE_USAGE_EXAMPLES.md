# Mentor Staking Contract - Usage Examples

## Quick Start Examples

### Example 1: Basic Staking Flow

```rust
// Mentor stakes 1000 SKILL tokens
let result = contract.stake(skill_token_address, 1000);
assert!(result.is_ok());

// Check current stake
let stake_info = contract.get_stake(mentor_address, skill_token_address);
assert_eq!(stake_info.unwrap().staked, 1000);
```

### Example 2: Requesting Unstake

```rust
// Mentor has 1000 staked, requests to unstake 300
let unlock_block = contract.request_unstake(skill_token_address, 300)?;

// New state
let stake_info = contract.get_stake(mentor_address, skill_token_address).unwrap();
assert_eq!(stake_info.staked, 700);           // Active stake reduced
assert_eq!(stake_info.pending_unstake, 300);  // Under cooldown
assert_eq!(stake_info.unlock_at, unlock_block);

// Calculate wait time
let current_block = env.block_number();
let blocks_to_wait = unlock_block - current_block;
println!("Must wait {} blocks (~{} hours)", blocks_to_wait, blocks_to_wait / 600);
```

### Example 3: Cooldown Period and Withdrawal

```rust
// Set up
contract.stake(token, 1000)?;
set_block_number(100);
let unlock_at = contract.request_unstake(token, 500)?; // unlock_at = 700 (cooldown=600)

// Attempt early withdrawal (fails)
set_block_number(650);
let result = contract.withdraw_unstaked(token);
assert_eq!(result, Err(StakeError::UnlockTimeNotReached));

// Wait until cooldown expires
set_block_number(700);
let result = contract.withdraw_unstaked(token);
assert!(result.is_ok());

// Verify final state
let stake_info = contract.get_stake(mentor_address, token).unwrap();
assert_eq!(stake_info.staked, 500);
assert_eq!(stake_info.pending_unstake, 0);
assert_eq!(stake_info.unlock_at, 0);
```

## Real-World Scenarios

### Scenario 1: Mentor Joining SkillSync

```rust
// Mentor deposits initial stake to build reputation
contract.stake(usdc_token, 5000)?;  // $5,000 commitment

// Later, needs to withdraw portion for personal use
contract.request_unstake(usdc_token, 1000)?;
// ... wait 600 blocks (~1 hour) ...
contract.withdraw_unstaked(usdc_token)?;

// Now has $4,000 staked (1000 remaining stake + 1000 withdrawn)
```

### Scenario 2: Mentor with Multiple Tokens

```rust
// Stake in different tokens for diversification
contract.stake(skill_token, 10_000)?;   // 10,000 SKILL
contract.stake(usdc_token, 5_000)?;     // 5,000 USDC

// Later, check total position
let skill_stake = contract.get_stake(mentor, skill_token).unwrap();
let usdc_stake = contract.get_stake(mentor, usdc_token).unwrap();

println!("SKILL staked: {}", skill_stake.staked);
println!("USDC staked: {}", usdc_stake.staked);

// Can unstake individually without affecting other tokens
contract.request_unstake(skill_token, 2_000)?;
// USDC stake unaffected
assert_eq!(
    contract.get_stake(mentor, usdc_token).unwrap().staked,
    5_000
);
```

### Scenario 3: Fraud Detection and Slashing

```rust
// Mentor gets caught violating terms (off-chain detection)
let mentor = accounts.bob;
let token = accounts.charlie;

// Setup: Mentor has staked 10,000
contract.stake(token, 10_000)?;

// Admin detects fraud and applies graduated penalties
set_caller(admin);

// First offense: 10% slash
contract.slash(mentor, token, 1_000)?;  // 9,000 remaining

// Mentor continues to violate
// Second offense: 20% of remaining slash
contract.slash(mentor, token, 1_800)?;  // 7,200 remaining

// Third offense: 50% of remaining slash
contract.slash(mentor, token, 3_600)?;  // 3,600 remaining

// Monitor: All slashing events logged for review
// Event: Slashed { mentor, token, 1_000 }
// Event: Slashed { mentor, token, 1_800 }
// Event: Slashed { mentor, token, 3_600 }
```

### Scenario 4: Concurrent Stake and Unstake

```rust
// Mentor stakes 1000
contract.stake(token, 1000)?;

// Requests to unstake 400 (starts cooldown)
let unlock_at = contract.request_unstake(token, 400)?;

let stake_info = contract.get_stake(mentor, token).unwrap();
assert_eq!(stake_info.staked, 600);
assert_eq!(stake_info.pending_unstake, 400);

// While waiting for cooldown, stakes more
contract.stake(token, 300)?;

// Check state: new stake adds to active, doesn't affect pending
let updated = contract.get_stake(mentor, token).unwrap();
assert_eq!(updated.staked, 900);        // 600 + 300 new stake
assert_eq!(updated.pending_unstake, 400);  // unchanged

// Later: can request more unstaking while still in cooldown from previous
contract.request_unstake(token, 200)?;

// Still in original cooldown for first 400
// Second unstake creates new cooldown (overwrites unlock_at)
```

### Scenario 5: Admin Transfer

```rust
// Initial admin
assert_eq!(contract.get_admin(), alice);

// Alice transfers admin rights to governance contract
set_caller(alice);
contract.transfer_admin(governance_contract)?;

assert_eq!(contract.get_admin(), governance_contract);

// Now governance contract (multisig or DAO) controls slashing
set_caller(governance_contract);
contract.slash(bad_mentor, token, 5_000)?;  // ✓ Works

// Alice can no longer slash
set_caller(alice);
let result = contract.slash(bad_mentor, token, 1_000);
assert_eq!(result, Err(StakeError::Unauthorized));  // ✗ Fails
```

## Error Handling Examples

### Example: Handling InsufficientStake

```rust
contract.stake(token, 1000)?;

// Try to unstake more than available
match contract.request_unstake(token, 1500) {
    Ok(_) => panic!("Should have failed"),
    Err(StakeError::InsufficientStake) => {
        println!("Not enough stake to unstake 1500");
    }
    Err(e) => panic!("Unexpected error: {:?}", e),
}
```

### Example: Handling UnlockTimeNotReached

```rust
set_block_number(100);
contract.stake(token, 1000)?;

set_block_number(105);
let unlock_at = contract.request_unstake(token, 500)?;
// unlock_at = 705 (100 + 600 + 5)

set_block_number(700);

match contract.withdraw_unstaked(token) {
    Err(StakeError::UnlockTimeNotReached) => {
        let current = 700;
        let needed = 705;
        println!("Need to wait {} more blocks", needed - current);
    }
    _ => {}
}
```

### Example: Handling Unauthorized

```rust
let admin = accounts.alice;
let mentor = accounts.bob;
let non_admin = accounts.charlie;

// Setup
contract.stake(token, 1000)?;

// Non-admin tries to slash
set_caller(non_admin);
match contract.slash(mentor, token, 100) {
    Err(StakeError::Unauthorized) => {
        println!("Only admin can slash");
    }
    _ => {}
}
```

## Testing Patterns

### Pattern 1: Test Cooldown with Boundary Blocks

```rust
#[ink::test]
fn test_cooldown_exact_boundary() {
    let mut contract = Stake::new(admin, 100);
    
    set_caller(mentor);
    contract.stake(token, 1000).unwrap();
    
    set_block_number(50);
    let unlock_at = contract.request_unstake(token, 500).unwrap();
    assert_eq!(unlock_at, 150);
    
    // One block before should fail
    set_block_number(149);
    assert_eq!(
        contract.withdraw_unstaked(token),
        Err(StakeError::UnlockTimeNotReached)
    );
    
    // Exact block should succeed
    set_block_number(150);
    assert!(contract.withdraw_unstaked(token).is_ok());
}
```

### Pattern 2: Test Multi-Token Independence

```rust
#[ink::test]
fn test_multiple_tokens_independent() {
    let mut contract = Stake::new(admin, 100);
    
    set_caller(mentor);
    let token_a = accounts.charlie;
    let token_b = accounts.dave;
    
    // Stake different amounts in different tokens
    contract.stake(token_a, 1000).unwrap();
    contract.stake(token_b, 2000).unwrap();
    
    // Slash token_a
    set_caller(admin);
    contract.slash(mentor, token_a, 300).unwrap();
    
    // token_b should be unaffected
    let stake_b = contract.get_stake(mentor, token_b).unwrap();
    assert_eq!(stake_b.staked, 2000);
}
```

### Pattern 3: Test Event Emissions

```rust
#[ink::test]
fn test_staked_event() {
    let mut contract = Stake::new(admin, 100);
    
    set_caller(mentor);
    let token = accounts.charlie;
    
    // Emit Staked event
    contract.stake(token, 1000).unwrap();
    
    // In real environment, check that event was emitted:
    // Event: Staked {
    //   mentor: <mentor_address>,
    //   token: <token_address>,
    //   amount: 1000
    // }
}
```

## Monitoring and Observability

### Key Metrics to Track

```
1. Total Staked Value
   - Sum of all staked balances across mentors
   - Indicator of mentor commitment

2. Slash Rate
   - Count(Slashed events) / Count(Active mentors)
   - Indicator of contract health

3. Cooldown Completion Rate
   - Count(Unstaked events) / Count(UnstakeRequested events)
   - Indicator of withdrawal success

4. Average Stake Duration
   - Time between Staked and Unstaked events
   - Indicator of mentor retention

5. Admin Activity
   - Count(Slash events) per admin per period
   - Monitor for over/under-enforcement
```

### Event Monitoring Example

```rust
// Off-chain listener
enum StakeEvent {
    Staked { mentor, token, amount },
    UnstakeRequested { mentor, token, amount, unlock_at },
    Unstaked { mentor, token, amount },
    Slashed { mentor, token, amount },
}

// Track slashing patterns
fn analyze_slash_patterns(events: &[StakeEvent]) {
    let slashes: Vec<_> = events.iter()
        .filter_map(|e| {
            if let StakeEvent::Slashed { mentor, amount, .. } = e {
                Some((mentor, amount))
            } else {
                None
            }
        })
        .collect();
    
    // Alert if same mentor slashed 3+ times
    for (mentor, slashes) in group_by_mentor(&slashes) {
        if slashes.len() >= 3 {
            alert!("Mentor {:?} has {} slashes - consider removal", mentor, slashes.len());
        }
    }
}
```

## Common Gotchas

### ❌ Don't: Assume Instant Withdrawal
```rust
// Wrong - cooldown is enforced
contract.request_unstake(token, 500)?;
contract.withdraw_unstaked(token)?;  // ✗ Will fail - cooldown not elapsed
```

### ✓ Do: Wait for Cooldown
```rust
// Right - wait for cooldown
contract.request_unstake(token, 500)?;
// ... wait 600+ blocks ...
contract.withdraw_unstaked(token)?;  // ✓ Works
```

### ❌ Don't: Assume Partial Unstakes Work
```rust
// Wrong - can only withdraw all pending_unstake
contract.request_unstake(token, 500)?;
// Later need only 300 back
contract.withdraw_unstaked(token)?;  // Takes all 500, not just 300
```

### ✓ Do: Request Exact Amount
```rust
// Right - be precise with unstake amounts
contract.request_unstake(token, 300)?;  // Request only what you need
// Wait...
contract.withdraw_unstaked(token)?;  // Gets exactly 300
```

### ❌ Don't: Forget Zero Validation
```rust
// Wrong - zero amounts rejected
contract.stake(token, 0)?;  // ✗ Error: ZeroAmount
contract.request_unstake(token, 0)?;  // ✗ Error: ZeroAmount
```

### ✓ Do: Validate Amounts
```rust
// Right - always validate
if amount > 0 {
    contract.stake(token, amount)?;
}
```

## Performance Considerations

### Gas Optimization
- Stake operations are O(1) with storage lookups
- No loops or iterations required
- Minimal arithmetic overhead

### Storage Efficiency
- One mapping entry per (mentor, token) pair
- Entries remain after withdrawal (for historical queries)
- Can be archived to cheaper storage if needed

### Scaling
- No gas cost increases with number of mentors
- Each mentor/token pair is independent
- Suitable for high-volume mentor populations

## Conclusion

The Stake contract provides a robust, secure foundation for mentor commitment signaling with admin-enforced penalties for fraud. The cooldown mechanism prevents abuse while maintaining high throughput and low operational costs.
