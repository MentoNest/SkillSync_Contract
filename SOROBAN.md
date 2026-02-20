# Soroban Multi-Network Configuration Guide

This document explains how to manage multi-network configuration for deploying SkillSync contracts to Soroban.

## Overview

The configuration system supports three networks:

| Network | Purpose | RPC URL |
|---------|---------|---------|
| **testnet** | Testing before mainnet deployment | `https://soroban-testnet.stellar.org` |
| **mainnet** | Production Stellar network | `https://mainnet.sorobanrpc.com` |
| **sandbox** | Local development environment | `http://localhost:8000` |

---

## Configuration Files

### `soroban.toml` - Profile Definitions

Located at the workspace root, defines network profiles:

```toml
[profile.testnet]
network = "testnet"
rpc_url = "https://soroban-testnet.stellar.org"
network_passphrase = "Test SDF Network ; September 2015"
description = "Stellar Testnet - for testing before mainnet deployment"
```

**Never modify this directly.** Use environment variables to override specific values.

### `.env` - Environment-Specific Configuration

Copy `.env.example` to `.env` and configure your local environment:

```bash
# Select network
SOROBAN_NETWORK=testnet

# Optional: Override RPC URL
SOROBAN_RPC_URL=https://soroban-testnet.stellar.org

# Optional: Provide contract details
SOROBAN_CONTRACT_ID=CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4
SOROBAN_ACCOUNT=GBRPYHIL2CI3WHZDTOOQFC6EB4KCYFFWXHEUNUME34PJF7RDOFB6HXY
```

**Important:** Never commit `.env` with real values. Add it to `.gitignore`:

```bash
echo ".env" >> .gitignore
```

---

## Configuration Resolution

The system resolves configuration in this priority order:

1. **Environment Variables** (highest priority)
   - `SOROBAN_NETWORK`
   - `SOROBAN_RPC_URL`
   - `SOROBAN_NETWORK_PASSPHRASE`
   - etc.

2. **soroban.toml Profile** (if env var not set)
   - Profile name from `SOROBAN_NETWORK` or default
   - Looks up values in `[profile.<name>]` section

3. **Network Defaults** (if neither above exists)
   - Each `Network` enum variant has built-in defaults
   - Used only if TOML is missing

4. **Error** (if required fields cannot be resolved)
   - Missing `rpc_url` or `network_passphrase`

**Example resolution for testnet:**

```
SOROBAN_RPC_URL env var is set?
  ├─ Yes → Use it
  └─ No → Check soroban.toml [profile.testnet]
       ├─ Found → Use it
       └─ Not found → Use Network::Testnet.default_rpc_url()
```

---

## Using the Configuration System

### 1. CLI: Show Active Network

```bash
# Display resolved configuration
cargo run -p skillsync-tools -- network show

# Output:
# ╔════════════════════════════════════════════════════════════════╗
# ║         SOROBAN NETWORK CONFIGURATION RESOLVED                 ║
# ╚════════════════════════════════════════════════════════════════╝
#   Network:             testnet
#   RPC URL:             https://soroban-testnet.stellar.org
#   Network Passphrase:  Test SDF Network ; September 2015
#   Contract ID:         (not configured)
#   Account:             (not configured)
#   RPC Timeout:         30000ms
```

### 2. CLI: List Available Networks

```bash
cargo run -p skillsync-tools -- network list

# Output:
# ╔════════════════════════════════════════════════════════════════╗
# ║            AVAILABLE SOROBAN NETWORKS                          ║
# ╚════════════════════════════════════════════════════════════════╝
#   testnet   - Stellar Testnet (for testing)
#   mainnet   - Stellar Mainnet (production)
#   sandbox   - Local Soroban Sandbox (localhost:8000)
```

### 3. CLI: Deploy with Network Override

```bash
# Deploy to mainnet (overrides SOROBAN_NETWORK env var)
cargo run -p skillsync-tools -- deploy \
  --network mainnet \
  --wasm target/wasm32-unknown-unknown/release/skillsync_core.wasm
```

### 4. CLI: Show Configuration as JSON

```bash
cargo run -p skillsync-tools -- config --json

# Output:
# {
#   "network": "testnet",
#   "rpc_url": "https://soroban-testnet.stellar.org",
#   "network_passphrase": "Test SDF Network ; September 2015",
#   "contract_id": null,
#   "account": null,
#   "rpc_timeout_ms": 30000,
#   "debug": false
# }
```

### 5. Rust Library: Load Configuration Programmatically

**In your Rust code:**

```rust
use skillsync_tools::Config;

fn main() -> Result<()> {
    // Load configuration from env + soroban.toml
    let config = Config::load()?;
    
    // Use configuration
    println!("Network: {}", config.network);
    println!("RPC URL: {}", config.rpc_url);
    
    if let Some(contract_id) = &config.contract_id {
        println!("Contract: {}", contract_id);
    }
    
    Ok(())
}
```

---

## Switching Networks

### Option 1: Set Environment Variable (Temporary)

```bash
# Test on testnet
SOROBAN_NETWORK=testnet cargo run -p skillsync-tools -- network show

# Deploy to mainnet
SOROBAN_NETWORK=mainnet cargo run -p skillsync-tools -- deploy \
  --wasm target/wasm32-unknown-unknown/release/skillsync_core.wasm
```

### Option 2: Update `.env` File (Persistent)

```bash
# Edit .env
nano .env

# Change SOROBAN_NETWORK=testnet to SOROBAN_NETWORK=mainnet

# Then run any command
cargo run -p skillsync-tools -- network show
```

### Option 3: Use Soroban CLI (Alternative)

The official Soroban CLI also supports network management:

```bash
# Add a custom network
soroban config network add my-network \
  --rpc-url https://custom-rpc.example.com \
  --network-passphrase "Custom Network ; 2024"

# List configured networks
soroban config network ls

# Select a network for soroban CLI
soroban config network use testnet

# Verify selection
soroban config network show
```

---

## Working with Sandbox (Local Development)

### Start Soroban Sandbox

```bash
# Install soroban CLI if you haven't
cargo install soroban-cli

# Start the sandbox (listens on localhost:8000)
soroban network start --admin-http-port 8000

# In another terminal, verify it's running
curl http://localhost:8000/api/soroban-rpc
```

### Configure for Sandbox

```bash
# Set environment variable
export SOROBAN_NETWORK=sandbox

# Or edit .env
SOROBAN_NETWORK=sandbox

# Verify configuration
cargo run -p skillsync-tools -- network show
# Should show: RPC URL: http://localhost:8000
```

### Deploy to Sandbox

```bash
# Build the contract
cargo build -p skillsync-core --target wasm32-unknown-unknown --release

# Deploy locally
cargo run -p skillsync-tools -- deploy \
  --network sandbox \
  --wasm target/wasm32-unknown-unknown/release/skillsync_core.wasm
```

---

## Network-Specific Setup

### Testnet Setup

```bash
# 1. Fund a testnet account (free)
#    Go to: https://friendbot.stellar.org/
#    Paste your public key (G...)

# 2. Set environment
export SOROBAN_NETWORK=testnet
export SOROBAN_ACCOUNT=GXXXXXXX...  # Your testnet account

# 3. Generate signing key (if you don't have one)
soroban config identity generate testnet-dev

# 4. Verify configuration
cargo run -p skillsync-tools -- config --json
```

### Mainnet Setup

```bash
# 1. Create/fund a mainnet account
#    - Use official Stellar wallet
#    - Ensure sufficient balance for deployment

# 2. Set environment (USE CAUTION - This is production!)
export SOROBAN_NETWORK=mainnet
export SOROBAN_ACCOUNT=GXXXXXXX...  # Your mainnet account

# 3. Configure signing key securely
#    - DO NOT commit signing key to git
#    - Use environment variable or secure storage
export SOROBAN_SIGNING_KEY=SXXXXXXX...

# 4. Verify configuration
cargo run -p skillsync-tools -- config --json

# 5. Double-check before deploying!
# Verify network is mainnet in the output
```

### Sandbox Setup

```bash
# In terminal 1: Start sandbox
soroban network start --admin-http-port 8000

# In terminal 2: Configure and deploy
export SOROBAN_NETWORK=sandbox
export SOROBAN_RPC_URL=http://localhost:8000

cargo run -p skillsync-tools -- deploy \
  --wasm target/wasm32-unknown-unknown/release/skillsync_core.wasm
```

---

## Configuration Validation

### CLI Validation

```bash
cargo run -p skillsync-tools -- config --validate

# Output: ✓ Configuration is valid
```

### Programmatic Validation

```rust
use skillsync_tools::Config;

fn main() -> Result<()> {
    match Config::load() {
        Ok(config) => {
            println!("✓ Configuration is valid");
            println!("Network: {}", config.network);
            Ok(())
        }
        Err(e) => {
            eprintln!("✗ Configuration error: {}", e);
            Err(e.into())
        }
    }
}
```

### Common Validation Errors

| Error | Cause | Solution |
|-------|-------|----------|
| `Missing SOROBAN_NETWORK` | No env var, soroban.toml not found | Create soroban.toml or set `SOROBAN_NETWORK` |
| `Invalid network: xyz` | Unknown network name | Use testnet, mainnet, or sandbox |
| `RPC URL must start with http` | Invalid URL format | Fix URL in env or soroban.toml |
| `Missing field: rpc_url` | RPC URL not defined anywhere | Set in soroban.toml or `SOROBAN_RPC_URL` env var |

---

## Environment Variables Reference

| Variable | Required | Default | Example |
|----------|----------|---------|---------|
| `SOROBAN_NETWORK` | No | testnet (if soroban.toml missing) | testnet, mainnet, sandbox |
| `SOROBAN_RPC_URL` | No | From soroban.toml profile | https://soroban-testnet.stellar.org |
| `SOROBAN_NETWORK_PASSPHRASE` | No | From soroban.toml profile | Test SDF Network ; September 2015 |
| `SOROBAN_CONTRACT_ID` | No | None | CAAAA... |
| `SOROBAN_ACCOUNT` | No | None | GBBB... |
| `SOROBAN_RPC_TIMEOUT_MS` | No | 30000 | 60000 |
| `SOROBAN_DEBUG` | No | false | true, false |

---

## Best Practices

### ✅ DO

- ✓ Commit `soroban.toml` to version control
- ✓ Commit `.env.example` with safe defaults
- ✓ Add `.env` to `.gitignore`
- ✓ Use environment variables for secrets
- ✓ Test on testnet before mainnet
- ✓ Use `soroban.toml` for profiles
- ✓ Validate configuration before deployment
- ✓ Keep separate containers/machines for mainnet

### ❌ DON'T

- ✗ Commit `.env` with real values
- ✗ Commit signing keys to git
- ✗ Use hardcoded network URLs
- ✗ Mix mainnet and testnet code
- ✗ Deploy without verifying network
- ✗ Modify `soroban.toml` runtime (env vars instead)
- ✗ Use root accounts for deployments
- ✗ Skip configuration validation

---

## Troubleshooting

### Configuration Not Loading

**Problem**: `Error: Missing SOROBAN_NETWORK environment variable`

**Solution**:
```bash
# Create soroban.toml in workspace root
# OR set environment variable
export SOROBAN_NETWORK=testnet
```

### Wrong Network Selected

**Problem**: Deployed to testnet instead of mainnet

**Verify**:
```bash
# Always check before deploying
cargo run -p skillsync-tools -- config --json

# Confirm network field matches where you want to deploy
```

### RPC Request Timeout

**Problem**: `Error: RPC request timed out`

**Solution**:
```bash
# Increase timeout
export SOROBAN_RPC_TIMEOUT_MS=60000

# Or check RPC endpoint is reachable
curl -s https://soroban-testnet.stellar.org/api/soroban-rpc
```

### Cannot Find soroban.toml

**Problem**: `Error: soroban.toml not found`

**Solution**: Ensure you're running from workspace root:
```bash
cd /path/to/SkillSync_Contract
cargo run -p skillsync-tools -- network show
```

---

## Integration Examples

### With GitHub Actions

```yaml
name: Deploy to Testnet

on:
  push:
    branches: [main]

jobs:
  deploy:
    runs-on: ubuntu-latest
    env:
      SOROBAN_NETWORK: testnet
      SOROBAN_ACCOUNT: ${{ secrets.TESTNET_ACCOUNT }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: wasm32-unknown-unknown
      - run: cargo run -p skillsync-tools -- config --validate
      - run: cargo build -p skillsync-core --target wasm32-unknown-unknown --release
      - run: cargo run -p skillsync-tools -- deploy --wasm target/wasm32-unknown-unknown/release/skillsync_core.wasm
```

### With Makefile

```makefile
.PHONY: config deploy-testnet deploy-mainnet

config:
	cargo run -p skillsync-tools -- config

deploy-testnet:
	SOROBAN_NETWORK=testnet cargo run -p skillsync-tools -- deploy --wasm target/wasm32-unknown-unknown/release/skillsync_core.wasm

deploy-mainnet:
	SOROBAN_NETWORK=mainnet cargo run -p skillsync-tools -- deploy --wasm target/wasm32-unknown-unknown/release/skillsync_core.wasm
```

---

## Reference

- [Soroban Documentation](https://soroban.stellar.org/)
- [Stellar Network Passphrases](https://developers.stellar.org/docs/fundamentals-and-concepts/stellar-data-structures/transactions#network-passphrase)
- [soroban-cli](https://github.com/stellar/rs-soroban-sdk/tree/master/soroban-cli)
- Configuration Module: `crates/tools/src/config.rs`
