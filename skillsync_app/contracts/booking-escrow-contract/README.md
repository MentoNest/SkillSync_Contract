# Booking Escrow Contract

A Soroban smart contract that manages escrow functionality for booking payments between mentees and mentors on SkillSync.

## Overview

This contract locks mentee funds for a booking session and releases them to the mentor upon session completion, or refunds to the mentee upon cancellation. It ensures secure payment handling with proper authorization controls.

## Contract Interface

### Entrypoints

#### `init(admin: Address)`
Initializes the contract with an admin address who can authorize releases and refunds.

#### `fund(booking_id: u64, mentee: Address, mentor: Address, token: Address, amount: i128)`
Creates a new escrow for a booking. Only the mentee can call this function.
- Transfers tokens from mentee to the contract
- Stores escrow details with `Funded` status

#### `release(booking_id: u64)`
Releases escrowed funds to the mentor. Only callable by the admin.
- Transfers tokens from contract to mentor
- Updates status to `Released`

#### `refund(booking_id: u64)`
Refunds escrowed funds to the mentee. Only callable by the admin.
- Transfers tokens from contract to mentee
- Updates status to `Refunded`

#### `get(booking_id: u64) -> Escrow`
Returns the full escrow details for a booking.

#### `status(booking_id: u64) -> EscrowStatus`
Returns just the status of an escrow.

#### `get_admin() -> Address`
Returns the admin address.

## Data Structures

### Escrow
```rust
struct Escrow {
    booking_id: u64,
    mentee: Address,
    mentor: Address,
    token: Address,
    amount: i128,
    status: EscrowStatus,
    created_at: u64,
}
```

### EscrowStatus
```rust
enum EscrowStatus {
    Funded = 0,
    Released = 1,
    Refunded = 2,
}
```

### Error Codes
| Code | Name | Description |
|------|------|-------------|
| 1 | NotInitialized | Contract not initialized |
| 2 | AlreadyInitialized | Contract already initialized |
| 3 | EscrowNotFound | Booking ID does not exist |
| 4 | EscrowAlreadyExists | Booking ID already has an escrow |
| 5 | InvalidAmount | Amount must be greater than 0 |
| 6 | EscrowAlreadyTerminal | Escrow already released or refunded |
| 7 | Unauthorized | Caller not authorized |

## Events

### EscrowFunded
Emitted when an escrow is created.
- Topics: `("funded", booking_id)`
- Data: `(mentee, mentor, token, amount)`

### EscrowReleased
Emitted when funds are released to mentor.
- Topics: `("released", booking_id)`
- Data: `(mentor, amount)`

### EscrowRefunded
Emitted when funds are refunded to mentee.
- Topics: `("refunded", booking_id)`
- Data: `(mentee, amount)`

## Usage Example

```bash
# Initialize the contract
soroban contract invoke \
  --id $CONTRACT_ID \
  -- init \
  --admin $ADMIN_ADDRESS

# Fund an escrow (as mentee)
soroban contract invoke \
  --id $CONTRACT_ID \
  --source mentee \
  -- fund \
  --booking_id 1 \
  --mentee $MENTEE_ADDRESS \
  --mentor $MENTOR_ADDRESS \
  --token $TOKEN_ADDRESS \
  --amount 1000000000

# Release funds to mentor (as admin)
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  -- release \
  --booking_id 1

# Refund to mentee (as admin)
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  -- refund \
  --booking_id 1

# Get escrow details
soroban contract invoke \
  --id $CONTRACT_ID \
  -- get \
  --booking_id 1
```

## Security Considerations

- **Authorization**: Only the mentee can fund an escrow. Only the admin can release or refund.
- **Idempotency**: Double release/refund is prevented by checking terminal status.
- **CEI Pattern**: Checks-Effects-Interactions pattern is followed to prevent reentrancy.
- **Amount Validation**: Zero and negative amounts are rejected.

## Building

```bash
cd skillsync_app
cargo build --release --target wasm32-unknown-unknown -p booking-escrow-contract
```

## Testing

```bash
cd skillsync_app
cargo test -p booking-escrow-contract
```

## Test Coverage

The test suite covers:
- Initialization (success and double-init prevention)
- Fund operations (success, auth requirements, double-fund prevention, zero amount rejection)
- Release operations (success, idempotency, auth requirements)
- Refund operations (success, idempotency, auth requirements)
- Cross-state validation (no refund after release, no release after refund)
- Token decimal handling (0 decimals vs 7 decimals)
- Multiple bookings handling
