# Payment Release Authorization Contract

A secure ink! smart contract for SkillSync that validates payment release authorization via cryptographic signatures and implements replay protection.

## Overview

The ReleaseAuth contract enables trusted authorization of mentor payouts through:
- **Authorized Signers**: Admin-managed list of backend signers or trusted parties
- **Signature Verification**: Cryptographic validation of release authorization
- **Replay Protection**: Nonce tracking prevents duplicate authorizations
- **Event Logging**: All authorizations logged for audit trail

## Features

### Core Functionality

- **Multi-Signer Support**: Multiple authorized signers can approve releases
- **Admin-Controlled**: Only admin manages the authorized signers list
- **Nonce Protection**: Prevents replay attacks with one-time nonce tracking
- **Event Logging**: All operations emit events for transparency

### Security

- **Signature Verification**: Validates cryptographic signatures against payloads
- **Replay Prevention**: Each nonce can only be used once
- **Admin Authorization**: Caller checks on sensitive operations
- **Stateless Verification**: Can be called from external contracts

## Interface

### Constructor

```rust
fn new(admin: AccountId) -> Self
```

Initializes the contract with an admin account.

### Messages

#### `add_signer(pubkey: [u8; 32]) -> Result<(), ReleaseAuthError>`

Adds a new authorized signer (admin only).

**Parameters:**
- `pubkey`: The 32-byte public key to authorize

**Emits:** `SignerAdded` event

**Example:**
```rust
contract.add_signer([1u8; 32])?;
```

#### `remove_signer(pubkey: [u8; 32]) -> Result<(), ReleaseAuthError>`

Removes an authorized signer (admin only).

**Parameters:**
- `pubkey`: The 32-byte public key to remove

**Emits:** `SignerRemoved` event

**Example:**
```rust
contract.remove_signer([1u8; 32])?;
```

#### `authorize(booking_id, mentee, mentor, amount, token, nonce, signature) -> Result<bool, ReleaseAuthError>`

Authorizes a release by verifying the signature against the payload.

**Parameters:**
- `booking_id`: Unique booking identifier (u64)
- `mentee`: Mentee account address
- `mentor`: Mentor account address
- `amount`: Release amount (Balance)
- `token`: Token contract address
- `nonce`: Replay protection nonce (u64)
- `signature`: 65-byte signature (r || s || v format)

**Returns:** `Ok(true)` if signature is valid and not a replay

**Emits:** `ReleaseAuthorized` event on success

**Errors:**
- `NonceAlreadyUsed`: Nonce has been used before
- `SignerNotFound`: Signer not in authorized list
- `Unauthorized`: Caller not authorized (for future extensions)

**Example:**
```rust
let result = contract.authorize(
    booking_id,
    mentee,
    mentor,
    amount,
    token,
    nonce,
    signature,
)?;
```

#### `is_nonce_used(nonce: u64) -> bool`

Checks if a nonce has already been used.

**Parameters:**
- `nonce`: The nonce to check

**Returns:** `true` if nonce is marked as used

**Example:**
```rust
if contract.is_nonce_used(100) {
    println!("Nonce 100 has already been used");
}
```

#### `is_signer_authorized(pubkey: [u8; 32]) -> bool`

Checks if a public key is authorized to sign releases.

**Parameters:**
- `pubkey`: The 32-byte public key to check

**Returns:** `true` if signer is authorized

**Example:**
```rust
if contract.is_signer_authorized(pubkey) {
    println!("This signer is authorized");
}
```

#### `get_admin() -> AccountId`

Returns the admin account.

#### `get_signer_count() -> u32`

Returns the number of authorized signers.

#### `transfer_admin(new_admin: AccountId) -> Result<(), ReleaseAuthError>`

Transfers admin rights to a new account (admin only).

**Parameters:**
- `new_admin`: The new admin account

## Storage

### Mapping: `signers`
- **Key**: `[u8; 32]` - Public key
- **Value**: `bool` - Whether authorized

### Mapping: `used_nonces`
- **Key**: `u64` - Nonce value
- **Value**: `bool` - Whether already used

### Constants
- `admin: AccountId` - Admin account for signer management
- `signer_count: u32` - Total number of authorized signers

## Events

### SignerAdded
```rust
pub struct SignerAdded {
    pubkey: [u8; 32],  // topic
}
```
Emitted when a signer is added to the authorized list.

### SignerRemoved
```rust
pub struct SignerRemoved {
    pubkey: [u8; 32],  // topic
}
```
Emitted when a signer is removed from the authorized list.

### ReleaseAuthorized
```rust
pub struct ReleaseAuthorized {
    booking_id: u64,      // topic
    mentor: AccountId,    // topic
    signer: [u8; 32],     // Which signer authorized
}
```
Emitted when a release is successfully authorized.

## Payload Schema

The authorization payload includes all booking details:

```rust
pub struct ReleasePayload {
    booking_id: u64,    // Unique booking identifier
    mentee: AccountId,  // Mentee account
    mentor: AccountId,  // Mentor account
    amount: Balance,    // Release amount
    token: AccountId,   // Token address
    nonce: u64,         // Replay protection nonce
}
```

### Payload Encoding

The payload is encoded using SCALE codec before hashing:
```
Encoded = scale_codec::encode(ReleasePayload)
Hash = blake2x256(Encoded)
```

### Signature Format

Signatures are 65 bytes in the format: `r || s || v`
- **r**: 32 bytes - ECDSA r component
- **s**: 32 bytes - ECDSA s component  
- **v**: 1 byte - Recovery ID (27-28 or 0-1 depending on curve)

## Error Handling

```rust
pub enum ReleaseAuthError {
    Unauthorized,           // Caller not admin
    SignatureInvalid,      // Signature verification failed
    NonceAlreadyUsed,      // Replay attack detected
    SignerNotFound,        // Public key not authorized
    SignerAlreadyExists,   // Signer already in list
    EncodingError,         // Payload encoding failed
}
```

## Typical Flows

### Setting Up Signers

```rust
// Admin initializes contract
let contract = ReleaseAuth::new(admin_address);

// Admin adds backend signer
contract.add_signer(backend_pubkey)?;

// Admin adds secondary signer
contract.add_signer(secondary_pubkey)?;
```

### Authorizing a Release

```rust
// Backend generates payload and signs it
let payload = ReleasePayload {
    booking_id: 12345,
    mentee: mentee_addr,
    mentor: mentor_addr,
    amount: 5000,
    token: token_addr,
    nonce: 1,
};

let signature = backend.sign(&payload)?;

// Contract verifies signature
let authorized = contract.authorize(
    payload.booking_id,
    payload.mentee,
    payload.mentor,
    payload.amount,
    payload.token,
    payload.nonce,
    signature,
)?;

// Release funds if authorized (off-contract)
if authorized {
    release_funds(mentor, amount, token);
}
```

### Replay Prevention

```rust
// First authorization succeeds
contract.authorize(..., nonce=1, signature)?;  // ✓ OK

// Replay with same nonce fails
contract.authorize(..., nonce=1, signature)?;  // ✗ NonceAlreadyUsed

// Different nonce succeeds
contract.authorize(..., nonce=2, signature)?;  // ✓ OK
```

## Security Considerations

### Signature Verification

- Payloads are SCALE-encoded before hashing
- Hash algorithm: Blake2x256
- Signature recovery depends on the cryptographic curve (ECDSA, Ed25519, etc.)

### Nonce Management

- Each nonce can only be used once (monotonicity not required)
- Nonces should be randomly generated (not sequential for privacy)
- Off-chain systems must track issued nonces

### Admin Responsibilities

- Carefully manage the signer list
- Remove compromised signers immediately
- Audit all `SignerAdded`/`SignerRemoved` events
- Consider using a multisig for admin account

### Integration Points

- **Backend Signing**: Off-chain system generates and signs payloads
- **Release Execution**: Separate contract/system that checks authorization
- **Nonce Generation**: Backend responsible for unique nonce per release

## Testing

The contract includes 13 comprehensive test cases:

✅ Contract initialization
✅ Add signer success (authorized, unauthorized, duplicates)
✅ Remove signer success (authorized, unauthorized, not found)
✅ Authorization success with valid signature
✅ Authorization failure with invalid signature
✅ Replay protection (prevent reuse of nonce)
✅ Different nonce allows new authorization
✅ Nonce tracking and checking
✅ Signer authorization queries
✅ Admin transfer
✅ Multiple signers management
✅ Multiple releases with unique nonces
✅ Signer count accuracy
✅ Nonce isolation

Run tests:
```bash
cargo test --lib
```

## Deployment

```bash
cargo build --lib --release

# Deploy with:
# - admin: <admin_account>
```

Suggested deployment:
```rust
ReleaseAuth::new(governance_multisig_address)
```

## Integration with Other Contracts

The ReleaseAuth contract can be called by payment/escrow contracts:

```rust
// In a release/escrow contract
fn release_payment(&mut self, ..., nonce: u64, signature: [u8; 65]) {
    // Verify authorization
    let authorized = self.auth_contract.authorize(
        booking_id, mentee, mentor, amount, token,
        nonce, signature
    )?;
    
    if authorized {
        // Transfer funds
        transfer_token(token, mentor, amount)?;
    }
}
```

## Future Enhancements

- **Weighted Signers**: Different signer weights (1-of-N, 2-of-3, etc.)
- **Time-Locked Releases**: Payouts only valid after certain block
- **Amount Limits**: Per-signer release amount caps
- **Whitelist**: Token/mentee whitelist for additional security
- **Signature Batching**: Batch multiple authorizations in one call
- **Off-Chain Verification**: Include merkle proofs for large batches

## Version History

**v1.0 (Current)**
- Basic signature verification
- Nonce-based replay protection
- Admin-managed signer list
- Event logging

**Future v2.0**
- Weighted multisig support
- Time-based authorization windows
- Per-signer limits
- Batch verification

## License

Part of the SkillSync smart contract suite.
