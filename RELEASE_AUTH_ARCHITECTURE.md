# Release Authorization Contract - Architecture & Usage Guide

## Architecture Overview

```
┌────────────────────────────────────────────────────────┐
│                  ReleaseAuth Contract                   │
├────────────────────────────────────────────────────────┤
│                                                         │
│  Storage:                                               │
│  ├─ admin: AccountId                                   │
│  ├─ signers: Map<[u8;32], bool>                        │
│  ├─ used_nonces: Map<u64, bool>                        │
│  └─ signer_count: u32                                  │
│                                                         │
│  Public Messages:                                       │
│  ├─ add_signer(pubkey)         [Admin only]            │
│  ├─ remove_signer(pubkey)      [Admin only]            │
│  ├─ authorize(...)             [Public verification]   │
│  ├─ is_nonce_used(nonce)       [Query]                │
│  └─ is_signer_authorized(pk)   [Query]                │
│                                                         │
│  Events:                                                │
│  ├─ SignerAdded { pubkey }                             │
│  ├─ SignerRemoved { pubkey }                           │
│  └─ ReleaseAuthorized { booking_id, mentor, signer }   │
│                                                         │
└────────────────────────────────────────────────────────┘
```

## Data Flow

### Authorization Flow

```
Backend System
     │
     ├─ Generate ReleasePayload {
     │    booking_id, mentee, mentor,
     │    amount, token, nonce
     │  }
     │
     ├─ Encode payload (SCALE)
     │
     ├─ Hash payload (Blake2x256)
     │
     ├─ Sign hash (ECDSA/Ed25519)
     │   → 65-byte signature
     │
     └─ Call authorize(payload_fields, signature)
            │
            ▼
        ┌─────────────────────┐
        │ ReleaseAuth.authorize│
        └──────────┬──────────┘
                   │
         ┌─────────┼─────────┐
         │         │         │
         ▼         ▼         ▼
    Check    Check      Recover
    Nonce    Signer     Pubkey
     │         │          │
     └─────────┴──────────┘
            │
            ▼
      Mark Nonce Used
      Emit Event
      Return true/false
            │
            ▼
    Payment/Escrow Contract
    (Calls authorize before release)
```

## Signature Verification Flow

```
┌──────────────────────────────┐
│  Backend Generates Payload    │
├──────────────────────────────┤
│ booking_id: 12345            │
│ mentee: 0x123...             │
│ mentor: 0x456...             │
│ amount: 5000                 │
│ token: 0x789...              │
│ nonce: 1                     │
└────────────┬─────────────────┘
             │
             ▼
   ┌─────────────────────┐
   │  SCALE Encode       │
   │ [0xAA, 0xBB, ...]   │
   └────────────┬────────┘
                │
                ▼
   ┌──────────────────────┐
   │ Blake2x256 Hash      │
   │ 0x1234567890abcdef...│
   └────────────┬─────────┘
                │
                ▼
   ┌──────────────────────┐
   │ Sign with Backend Key│
   │ 65 bytes: r || s || v│
   └────────────┬─────────┘
                │
                ▼
   ┌──────────────────────────────┐
   │ Call Contract.authorize(      │
   │   booking_id,                │
   │   mentee, mentor, amount,    │
   │   token, nonce, signature    │
   │ )                            │
   └────────────┬─────────────────┘
                │
                ▼
   ┌──────────────────────────────┐
   │ Contract Verification:        │
   │                              │
   │ 1. Check nonce not used ✓    │
   │ 2. Re-encode payload         │
   │ 3. Re-hash payload           │
   │ 4. Recover pubkey from sig   │
   │ 5. Check pubkey authorized ✓ │
   │ 6. Mark nonce used ✓         │
   └────────────┬─────────────────┘
                │
         ┌──────┴──────┐
         │             │
      Success        Error
      (return true)  (return Err)
```

## Nonce Replay Prevention

```
Timeline of Authorizations:

Time T₁:  Backend calls authorize(..., nonce=1, signature=sig1)
          │
          ├─ Contract: Check nonce 1 → unused ✓
          ├─ Contract: Verify signature → valid ✓
          ├─ Contract: Mark nonce 1 as used
          └─ Return: true

Time T₂:  Attacker replays: authorize(..., nonce=1, signature=sig1)
          │
          ├─ Contract: Check nonce 1 → ALREADY USED ✗
          └─ Return: Err(NonceAlreadyUsed)

Time T₃:  Backend calls authorize(..., nonce=2, signature=sig2)
          │
          ├─ Contract: Check nonce 2 → unused ✓
          ├─ Contract: Verify signature → valid ✓
          ├─ Contract: Mark nonce 2 as used
          └─ Return: true
```

## State Diagram

```
                    ┌──────────────┐
                    │   Contract   │
                    │  Initialized │
                    └──────┬───────┘
                           │
                 add_signer(pubkey)
                           │
                    ┌──────▼──────────┐
                    │  Signer List    │
                    │  (empty → {pk}) │
                    └──────┬──────────┘
                           │
                 add_signer(pubkey2)
                           │
                    ┌──────▼────────────┐
                    │   Signer List     │
                    │  {pk, pk2}        │
                    └──────┬────────────┘
                           │
      ┌────────────────────┤
      │                    │
 remove_signer(pk)    authorize(nonce=1, sig)
      │                    │
      ▼                    ▼
  Signer List         Nonce 1 marked used
  {pk2}               ReleaseAuthorized event
      │                    │
      │            authorize(nonce=1, sig)  ← FAILS (replay)
      │                    │
      │            authorize(nonce=2, sig2) ← OK (new nonce)
      │                    │
      └────────────────────┘
```

## Usage Examples

### Example 1: Setup Authorized Signers

```rust
// Admin initializes contract
let mut contract = ReleaseAuth::new(admin_account);

// Add backend signer
contract.add_signer([1u8; 32])?;

// Add secondary/backup signer
contract.add_signer([2u8; 32])?;

// Verify signers added
assert_eq!(contract.get_signer_count(), 2);
```

### Example 2: Authorize a Release

```rust
// Backend generates payload
let booking_id = 12345u64;
let nonce = 1u64;
let signature = backend.sign_payload(
    booking_id, mentee, mentor, amount, token, nonce
)?;

// Contract verifies signature
let authorized = contract.authorize(
    booking_id,
    mentee_addr,
    mentor_addr,
    5000,  // amount in smallest unit
    token_addr,
    nonce,
    signature,
)?;

if authorized {
    // Release funds (in separate payment contract)
    payment_contract.release(mentor_addr, amount, token_addr)?;
}
```

### Example 3: Prevent Replay Attacks

```rust
// First authorization with nonce 1
let sig = backend.sign(..., nonce=1)?;
contract.authorize(..., nonce=1, sig)? // ✓ Returns true

// Attacker tries to replay
let result = contract.authorize(..., nonce=1, sig);
// ✗ Returns Err(NonceAlreadyUsed)

// Backend uses different nonce
let sig2 = backend.sign(..., nonce=2)?;
contract.authorize(..., nonce=2, sig2)? // ✓ Returns true
```

### Example 4: Check Nonce Status

```rust
// Before authorization
assert!(!contract.is_nonce_used(100));

// Authorize with nonce 100
contract.authorize(..., nonce=100, sig)?;

// After authorization
assert!(contract.is_nonce_used(100));
```

### Example 5: Manage Signers

```rust
// Check if signer is authorized
let pubkey = [5u8; 32];
if contract.is_signer_authorized(pubkey) {
    println!("Signer is authorized");
}

// Remove compromised signer
contract.remove_signer(compromised_pubkey)?;

// Verify removal
assert!(!contract.is_signer_authorized(compromised_pubkey));
```

### Example 6: Transfer Admin

```rust
// Current admin delegates to multisig
contract.transfer_admin(multisig_address)?;

// Verify transfer
assert_eq!(contract.get_admin(), multisig_address);

// Multisig can now manage signers
set_caller(multisig_address);
contract.add_signer(new_pubkey)?;
```

## Integration with Payment Contract

```rust
#[ink::contract]
mod payment {
    use crate::release_auth::{ReleaseAuth, ReleaseAuthError};
    
    #[ink(storage)]
    pub struct Payment {
        release_auth: ReleaseAuth,
        // ... other fields
    }
    
    impl Payment {
        #[ink(message)]
        pub fn release_with_auth(
            &mut self,
            booking_id: u64,
            mentee: AccountId,
            mentor: AccountId,
            amount: Balance,
            token: AccountId,
            nonce: u64,
            signature: [u8; 65],
        ) -> Result<(), PaymentError> {
            // Verify authorization
            let authorized = self.release_auth.authorize(
                booking_id,
                mentee,
                mentor,
                amount,
                token,
                nonce,
                signature,
            )
            .map_err(|_| PaymentError::Unauthorized)?;
            
            if !authorized {
                return Err(PaymentError::Unauthorized);
            }
            
            // Execute payment
            self.transfer_token(token, mentor, amount)?;
            
            Ok(())
        }
    }
}
```

## Payload Encoding Reference

### SCALE Encoding
```
ReleasePayload fields in order:
1. booking_id (u64)       - 8 bytes, little-endian
2. mentee (AccountId)     - 32 bytes
3. mentor (AccountId)     - 32 bytes
4. amount (Balance)       - 16 bytes, little-endian
5. token (AccountId)      - 32 bytes
6. nonce (u64)            - 8 bytes, little-endian

Total: 128 bytes before hashing
```

### Hashing Process
```
1. Encode payload using SCALE codec
2. Hash with Blake2x256
3. Input to signature algorithm

Example (pseudo-code):
hash = blake2x256(scale_encode(payload))
signature = ecdsa_sign(hash, private_key)
```

## Security Best Practices

### For Backend

1. **Nonce Generation**
   - Use cryptographically secure random number generator
   - Ensure nonces are unique per release
   - Don't reuse nonces

2. **Key Management**
   - Store private keys securely (HSM, vault)
   - Rotate keys periodically
   - Use different keys for different environments

3. **Signature Generation**
   - Encode payload consistently
   - Verify signature before sending
   - Log all signing operations

### For Admin

1. **Signer Management**
   - Audit all SignerAdded/SignerRemoved events
   - Remove compromised keys immediately
   - Rotate keys on regular schedule

2. **Access Control**
   - Keep admin account in multisig
   - Require approval for signer changes
   - Monitor all admin operations

3. **Monitoring**
   - Watch ReleaseAuthorized events
   - Alert on NonceAlreadyUsed errors (replay attempts)
   - Audit signer list regularly

## Testing Scenarios

### Valid Authorization
```rust
// Setup
contract.add_signer(backend_pubkey)?;
let sig = backend.sign_payload(...)?;

// Test
let result = contract.authorize(..., sig)?;
assert!(result);
```

### Replay Prevention
```rust
// First call
contract.authorize(..., nonce=1, sig)?;

// Replay attempt
let err = contract.authorize(..., nonce=1, sig);
assert_eq!(err, Err(ReleaseAuthError::NonceAlreadyUsed));
```

### Unauthorized Signer
```rust
// Use unauthorized signer
let sig = unauthorized_signer.sign_payload(...)?;

// Test
let err = contract.authorize(..., sig);
assert_eq!(err, Err(ReleaseAuthError::SignerNotFound));
```

### Admin Authorization
```rust
// Non-admin tries to add signer
set_caller(non_admin);
let err = contract.add_signer(pubkey);
assert_eq!(err, Err(ReleaseAuthError::Unauthorized));
```

## Error Handling Reference

| Error | Cause | Fix |
|-------|-------|-----|
| `NonceAlreadyUsed` | Nonce was used before | Use a new nonce |
| `SignerNotFound` | Public key not authorized | Add signer via add_signer() |
| `SignerAlreadyExists` | Trying to add duplicate | Remove then re-add if needed |
| `Unauthorized` | Caller is not admin | Use admin account |

---

**Last Updated**: 2026-01-23
**Status**: ✅ Complete
