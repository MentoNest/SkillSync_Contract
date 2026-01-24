# Mentor Staking Contract

A secure ink! smart contract for SkillSync that enables mentors to stake tokens as a commitment signal, with admin-enforced slashing for fraudulent behavior.

## Overview

The Stake contract implements a lifecycle-based staking mechanism with cooldown periods and admin oversight:

- **Stake**: Mentors deposit tokens to signal commitment
- **Request Unstake**: Initiate withdrawal with a cooldown period
- **Withdraw Unstaked**: Complete withdrawal after cooldown expires
- **Slash**: Admin-only enforcement against fraudulent mentors

## Features

### Core Functionality

- **Multi-token Support**: Mentors can stake different tokens simultaneously
- **Cooldown Protection**: Configurable delay between unstake request and withdrawal to prevent flash attacks
- **Admin Slashing**: Only authorized admin can penalize fraudulent behavior
- **Event Logging**: All state changes emit events for transparency

### Security

- **Reentrancy Prevention**: No external calls within state-mutating functions
- **Amount Validation**: Enforces non-zero amounts and prevents underflow
- **Admin Authorization**: All admin operations checked at entry
- **Cooldown Enforcement**: Withdrawal blocked before unlock block

## Interface

### Constructor

```rust
fn new(admin: AccountId, cooldown_blocks: BlockNumber) -> Self
```

Initializes the contract with an admin account and cooldown duration.

### Messages

#### `stake(token: AccountId, amount: Balance) -> Result<(), StakeError>`

Stakes tokens for the caller (mentor).

**Parameters:**
- `token`: The token address being staked
- `amount`: The amount to stake (must be > 0)

**Events:** Emits `Staked`

**Example:**
```rust
contract.stake(token_address, 1000)?;
```

#### `request_unstake(token: AccountId, amount: Balance) -> Result<BlockNumber, StakeError>`

Requests unstaking with cooldown period.

**Parameters:**
- `token`: The token address to unstake
- `amount`: The amount to unstake (must be <= current stake)

**Returns:** Block number when tokens can be withdrawn

**Events:** Emits `UnstakeRequested`

**Example:**
```rust
let unlock_block = contract.request_unstake(token_address, 500)?;
```

#### `withdraw_unstaked(token: AccountId) -> Result<(), StakeError>`

Withdraws all pending unstaked tokens after cooldown expires.

**Parameters:**
- `token`: The token address to withdraw

**Requirements:**
- Current block >= unlock_at block
- pending_unstake > 0

**Events:** Emits `Unstaked`

**Example:**
```rust
contract.withdraw_unstaked(token_address)?;
```

#### `slash(mentor: AccountId, token: AccountId, amount: Balance) -> Result<(), StakeError>`

Admin-only operation to slash a mentor's active stake.

**Parameters:**
- `mentor`: The mentor account to slash
- `token`: The token to slash from
- `amount`: Amount to slash (must be <= current stake)

**Requirements:**
- Caller must be admin

**Events:** Emits `Slashed`

**Example:**
```rust
contract.slash(mentor_address, token_address, 250)?;
```

#### `get_stake(mentor: AccountId, token: AccountId) -> Option<StakeInfo>`

Queries stake information for a mentor/token pair.

**Returns:**
```rust
StakeInfo {
    staked: Balance,              // Currently locked amount
    pending_unstake: Balance,     // Awaiting cooldown
    unlock_at: BlockNumber,       // Withdrawal available at this block
}
```

#### `get_admin() -> AccountId`

Returns the admin account.

#### `get_cooldown_blocks() -> BlockNumber`

Returns the configured cooldown period.

#### `transfer_admin(new_admin: AccountId) -> Result<(), StakeError>`

Transfers admin rights (admin only).

## Storage

### Mapping: `stakes`
- **Key**: `(mentor: AccountId, token: AccountId)`
- **Value**: `StakeInfo { staked, pending_unstake, unlock_at }`

### Constants
- `admin: AccountId` - Admin account for slashing
- `cooldown_blocks: BlockNumber` - Blocks before unstaked tokens can be withdrawn

## Events

### Staked
```rust
pub struct Staked {
    mentor: AccountId,
    token: AccountId,
    amount: Balance,
}
```

### UnstakeRequested
```rust
pub struct UnstakeRequested {
    mentor: AccountId,
    token: AccountId,
    amount: Balance,
    unlock_at: BlockNumber,
}
```

### Unstaked
```rust
pub struct Unstaked {
    mentor: AccountId,
    token: AccountId,
    amount: Balance,
}
```

### Slashed
```rust
pub struct Slashed {
    mentor: AccountId,
    token: AccountId,
    amount: Balance,
}
```

## Error Handling

```rust
pub enum StakeError {
    Unauthorized,              // Caller is not authorized
    InsufficientStake,        // Not enough staked tokens
    InsufficientPendingUnstake, // No pending unstake to withdraw
    UnlockTimeNotReached,     // Cooldown period not elapsed
    ZeroAmount,               // Amount is zero
    StakeNotFound,            // No stake for this mentor/token
}
```

## Typical Flows

### Staking
1. Mentor calls `stake(token, amount)` with tokens
2. Contract updates stake balance
3. `Staked` event emitted

### Unstaking with Cooldown
1. Mentor calls `request_unstake(token, amount)`
2. Amount moved to `pending_unstake`
3. Unlock block calculated as `current_block + cooldown_blocks`
4. `UnstakeRequested` event emitted
5. After cooldown expires, mentor calls `withdraw_unstaked(token)`
6. `Unstaked` event emitted

### Slashing for Fraud
1. Admin detects fraudulent behavior
2. Admin calls `slash(mentor, token, amount)`
3. Amount deducted from `staked` balance
4. `Slashed` event emitted
5. Mentor may lose additional reputation/rights

## Testing

The contract includes comprehensive tests covering:

- ✓ Stake/unstake lifecycle
- ✓ Cooldown enforcement
- ✓ Admin-only slashing
- ✓ Multiple tokens per mentor
- ✓ Partial slashing
- ✓ Boundary conditions
- ✓ Error cases
- ✓ Authorization checks

Run tests:
```bash
cargo test --lib
```

## Security Considerations

### Reentrancy
The contract uses no external calls in state-modifying functions, eliminating reentrancy risks.

### Cooldown Protection
The cooldown period prevents flash-loan style attacks where tokens are staked and unstaked in a single transaction.

### Admin Override
Only the admin can slash stakes, providing a last-resort enforcement mechanism against fraud.

### Amount Validation
- Zero amounts rejected
- Overflow/underflow prevented with checked arithmetic
- All balances validated before operations

## Deployment

```bash
cargo build --lib --release

# Deploy with:
# - admin: <admin_account>
# - cooldown_blocks: <number_of_blocks>
```

Suggested deployment with 600 block cooldown (roughly 1 hour on Substrate-based chains):
```rust
Stake::new(admin_address, 600)
```

## Future Enhancements

- Multi-level slashing (graduated penalties)
- Slashing appeals mechanism
- Unbounded staking periods
- Compound rewards for long-term stakes
- Delegation (mentors delegating stake to others)
