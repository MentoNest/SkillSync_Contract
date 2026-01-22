# SessionGate Contract

A smart contract for recording session completion status in the SkillSync platform. This contract serves as an authorization gate for escrow payouts, ensuring that sessions are only marked as completed by authorized administrators.

## Overview

The SessionGate contract provides a secure and immutable way to track session completion status. It acts as a critical component in the SkillSync escrow system, allowing the escrow contract to verify completion before releasing funds.

## Features

- **Admin-only control**: Only authorized administrators can mark sessions as completed
- **Immutable completion**: Once marked as completed, a session status cannot be changed
- **Event emission**: Emits events for transparency and off-chain tracking
- **Admin transfer**: Allows secure transfer of admin rights
- **Gas efficient**: Uses storage mapping for optimal performance

## Contract Interface

### Constructor

#### `new(admin: AccountId) -> Self`
Creates a new SessionGate contract instance.

**Parameters:**
- `admin`: The account ID that will have admin privileges

**Example:**
```rust
let contract = SessionGate::new(admin_account);
```

### Public Functions

#### `set_completed(booking_id: AccountId) -> Result<(), SessionGateError>`
Marks a booking as completed. Only callable by the admin.

**Parameters:**
- `booking_id`: The unique identifier for the booking/session

**Returns:**
- `Ok(())` on success
- `Err(SessionGateError)` on failure

**Errors:**
- `Unauthorized`: Caller is not the admin
- `AlreadyCompleted`: Booking is already marked as completed

**Example:**
```rust
contract.set_completed(booking_id)?;
```

#### `is_completed(booking_id: &AccountId) -> bool`
Checks if a booking has been marked as completed.

**Parameters:**
- `booking_id`: The booking ID to check

**Returns:**
- `true` if completed, `false` otherwise

**Example:**
```rust
let completed = contract.is_completed(&booking_id);
```

#### `admin() -> AccountId`
Returns the current admin account.

**Returns:**
- The admin account ID

**Example:**
```rust
let admin = contract.admin();
```

#### `transfer_admin(new_admin: AccountId) -> Result<(), SessionGateError>`
Transfers admin rights to a new account. Only callable by the current admin.

**Parameters:**
- `new_admin`: The new admin account ID

**Returns:**
- `Ok(())` on success
- `Err(SessionGateError::Unauthorized)` if caller is not admin

**Example:**
```rust
contract.transfer_admin(new_admin)?;
```

## Events

### `SessionCompleted`
Emitted when a booking is marked as completed.

**Fields:**
- `booking_id`: The booking ID that was marked as completed

**Example Event:**
```rust
SessionCompleted { booking_id: 0x1234... }
```

## Error Types

### `SessionGateError`

- **`Unauthorized`**: The caller is not authorized to perform the action
- **`AlreadyCompleted`**: The booking has already been marked as completed

## Security Considerations

1. **Admin Security**: The admin account should be a secure multi-sig or hardware wallet
2. **Immutable Status**: Once completed, booking status cannot be reversed
3. **Access Control**: All state-changing functions require admin authorization
4. **Event Transparency**: All completions are emitted as events for off-chain tracking

## Integration with Escrow

The SessionGate contract is designed to be queried by the escrow contract before releasing funds:

```rust
// In escrow contract
if session_gate.is_completed(&booking_id) {
    // Release funds to mentor
    release_payment(mentor_address, amount);
} else {
    // Keep funds in escrow
    revert("Session not completed");
}
```

## Testing

The contract includes comprehensive unit tests covering:

- Contract initialization
- Successful completion marking
- Unauthorized access prevention
- Duplicate completion prevention
- Admin transfer functionality
- Default completion status

Run tests with:
```bash
cargo test --package session_gate
```

## Deployment

### Prerequisites
- Rust toolchain
- ink! CLI tools
- Substrate node (for local testing)

### Build
```bash
cargo contract build --package session_gate
```

### Deploy
```bash
cargo contract instantiate \
  --constructor new \
  --args <ADMIN_ADDRESS> \
  --suri <DEPLOYER_SEED> \
  --endowment 1000000000000000 \
  --salt $(date +%s) \
  --manifest-path target/ink/metadata.json \
  --wasm target/ink/session_gate.wasm
```

## License

This contract is part of the SkillSync platform and follows the project's licensing terms.
