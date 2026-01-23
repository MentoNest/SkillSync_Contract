# Upgradable Contract Registry

A Soroban smart contract that maps logical names to current contract addresses, enabling versioned upgrades (e.g., `escrow:v2`) while keeping a stable lookup interface for consumers.

## Overview

The registry stores a pointer from a `Symbol` name to a contract `Address`. An admin can update pointers, and consumers can resolve the active address by name.

## Contract Interface

### Entrypoints

#### `init(admin: Address)`
Initializes the contract with an admin address.

#### `set(name: Symbol, addr: Address)`
Admin-only update of a registry pointer. Emits `RegistryUpdated`.

#### `get(name: Symbol) -> Address`
Returns the current address for the name.

#### `all() -> Vec<(Symbol, Address)>`
Returns all registry entries in insertion order.

#### `get_admin() -> Address`
Returns the current admin address.

## Storage

- `Admin()` -> `Address` (instance storage)
- `Registry(name)` -> `Address` (persistent storage)
- `RegistryKeys()` -> `Vec<Symbol>` (instance storage)

## Events

### RegistryUpdated
Emitted when a pointer is set or updated.
- Topics: `("RegistryUpdated")`
- Data: `{ name, addr }`

## Usage Example

```bash
# Initialize
soroban contract invoke \
  --id $CONTRACT_ID \
  -- init \
  --admin $ADMIN_ADDRESS

# Set a pointer (admin)
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  -- set \
  --name escrow_v2 \
  --addr $ESCROW_V2_ADDRESS

# Resolve a pointer
soroban contract invoke \
  --id $CONTRACT_ID \
  -- get \
  --name escrow_v2
```

## Security Considerations

- **Authorization**: Only the admin can update pointers.
- **Deterministic reads**: Consumers resolve by name with no mutable side effects.

## Building

```bash
cd skillsync_app
cargo build --release --target wasm32-unknown-unknown -p registry
```

## Testing

```bash
cd skillsync_app
cargo test -p registry
```
