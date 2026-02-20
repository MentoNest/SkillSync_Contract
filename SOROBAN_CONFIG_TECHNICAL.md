# Soroban Configuration System - Technical Documentation

## Architecture Overview

The multi-network configuration system is built with strong typing, error handling, and environment-driven resolution.

### Component Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                   CLI Interface (main.rs)                   │
│  Commands: network show, network list, deploy, config       │
└────────────────────┬────────────────────────────────────────┘
                     │
┌────────────────────▼────────────────────────────────────────┐
│               Config Module (config.rs)                      │
│  Owns: Network enum, Config struct, error types             │
│  Handles: Loading, validation, resolution                   │
└────────────────────┬────────────────────────────────────────┘
                     │
        ┌────────────┼────────────┐
        │            │            │
        ▼            ▼            ▼
   Environment   soroban.toml   Network Defaults
   Variables     (TOML parsing)  (fallback)
```

---

## Design Principles

### 1. Strong Typing

```rust
pub enum Network {
    Testnet,
    Mainnet,
    Sandbox,
}
```

**Benefits:**
- Compile-time guarantee of valid networks
- No string-based network names after parsing
- IDE autocompletion for network values

### 2. Typed Errors

```rust
pub enum ConfigError {
    MissingField(String),
    InvalidNetwork(String),
    ValidationError(String),
    // ...
}
```

**Benefits:**
- Errors are first-class values
- No `unwrap()` or `expect()` in config loading
- Callers can handle errors appropriately
- Clear error messages for debugging

### 3. Configuration Resolution Pipeline

```rust
Config::load()
  ├─ Load environment variables
  ├─ Load soroban.toml (if exists)
  ├─ Overlay env vars on TOML values
  ├─ Apply fallback defaults
  └─ Validate and return Config
```

**Benefits:**
- Predictable resolution order
- Environment variables always take precedence
- Graceful degradation
- Clear validation for invalid data

---

## File Reference

### soroban.toml

**Purpose**: Define named network profiles

**Structure**:
```toml
[profile.<name>]
network = "<network-type>"
rpc_url = "<endpoint>"
network_passphrase = "<passphrase>"
description = "<optional>"

[default]
network = "<default-profile>"
```

**Example**:
```toml
[profile.testnet]
network = "testnet"
rpc_url = "https://soroban-testnet.stellar.org"
network_passphrase = "Test SDF Network ; September 2015"

[default]
network = "testnet"
```

**Notes:**
- One profile section per network
- Profile name usually matches network name
- `[default]` section specifies fallback network
- Can be committed to git (no secrets)

### .env

**Purpose**: Override configuration for local development

**Format**:
```bash
SOROBAN_NETWORK=testnet
SOROBAN_RPC_URL=https://...
SOROBAN_CONTRACT_ID=C...
SOROBAN_ACCOUNT=G...
```

**Loading**:
- Automatically loaded from workspace root if exists
- Can be `.env`, `.env.local`, or `.env.<stage>`
- Add to `.gitignore` (contains potentially sensitive data)

**Example .env**:
```bash
# .env - local development
SOROBAN_NETWORK=testnet
SOROBAN_ACCOUNT=GBRPYHIL2CI3WHZDTOOQFC6EB4KCYFFWXHEUNUME34PJF7RDOFB6HXY
```

---

## Code Reference

### Network Enum

```rust
pub enum Network {
    Testnet,
    Mainnet,
    Sandbox,
}
```

**Methods:**

| Method | Returns | Purpose |
|--------|---------|---------|
| `as_str()` | `&'static str` | Get network as string ("testnet") |
| `from_str(s)` | `Result<Self>` | Parse network from string |
| `default_rpc_url()` | `&'static str` | Get hardcoded RPC endpoint |
| `passphrase()` | `&'static str` | Get network passphrase for signing |

**Example**:
```rust
let net = Network::from_str("testnet")?;
println!("URL: {}", net.default_rpc_url());
// URL: https://soroban-testnet.stellar.org
```

### Config Struct

```rust
pub struct Config {
    pub network: Network,
    pub rpc_url: String,
    pub network_passphrase: String,
    pub contract_id: Option<String>,
    pub account: Option<String>,
    pub rpc_timeout_ms: u64,
    pub debug: bool,
}
```

**Methods:**

| Method | Returns | Purpose |
|--------|---------|---------|
| `load()` | `Result<Config>` | Load from env + soroban.toml |
| `print_summary()` | `()` | Print formatted configuration |
| `to_json()` | `Result<String>` | Serialize to JSON |

**Example**:
```rust
let config = Config::load()?;
config.print_summary();

if let Some(contract) = config.contract_id {
    println!("Using contract: {}", contract);
}
```

### ConfigError Enum

```rust
pub enum ConfigError {
    Io(std::io::Error),
    TomlError(toml::de::Error),
    MissingField(String),
    InvalidNetwork(String),
    MissingNetworkConfig,
    EnvVar(std::env::VarError),
    ValidationError(String),
}
```

**All variants implement `std::error::Error`**, enabling robust error handling:

```rust
match Config::load() {
    Ok(config) => println!("Network: {}", config.network),
    Err(ConfigError::InvalidNetwork(net)) => {
        eprintln!("Unknown network: {}", net);
    }
    Err(e) => eprintln!("Config error: {}", e),
}
```

---

## Resolution Algorithm

### Step 1: Determine Network

```rust
let network_name = std::env::var("SOROBAN_NETWORK")
    .ok()
    .or_else(|| load_from_soroban_toml_default())
    .unwrap_or_else(|| "testnet".to_string());

let network = Network::from_str(&network_name)?;
```

**Flow**:
1. Check `SOROBAN_NETWORK` env var
2. If not set, check `[default] network` in soroban.toml
3. If nothing found, use "testnet"
4. Parse string to `Network` enum

### Step 2: Load TOML Profile (Optional)

```rust
let toml_config = load_soroban_toml()?;
let profile = toml_config.profile.get(&network_name);
```

**Details**:
- Search for `soroban.toml` in current directory
- Parse TOML into `SorobanToml` struct
- Look up profile matching network name
- If profile not found, use fallback values

### Step 3: Resolve RPC URL

```rust
let rpc_url = std::env::var("SOROBAN_RPC_URL")
    .ok()
    .or_else(|| profile.map(|p| p.rpc_url.clone()))
    .unwrap_or_else(|_| network.default_rpc_url().to_string());
```

**Priority**:
1. `SOROBAN_RPC_URL` environment variable
2. `rpc_url` from soroban.toml profile
3. Network default (Network::Testnet.default_rpc_url())

### Step 4: Resolve Passphrase

```rust
let network_passphrase = std::env::var("SOROBAN_NETWORK_PASSPHRASE")
    .ok()
    .or_else(|| profile.map(|p| p.network_passphrase.clone()))
    .unwrap_or_else(|_| network.passphrase().to_string());
```

**Priority**: Same as RPC URL

### Step 5: Load Metadata

```rust
let contract_id = std::env::var("SOROBAN_CONTRACT_ID").ok();
let account = std::env::var("SOROBAN_ACCOUNT").ok();
```

**Details**:
- `contract_id` and `account` are optional
- Loaded only from environment variables
- Other fields have TOML and network defaults

### Step 6: Validate

```rust
fn validate(network: &Network, rpc_url: &str, passphrase: &str) -> Result<()> {
    if rpc_url.is_empty() {
        return Err(MissingField("rpc_url"));
    }
    if !rpc_url.starts_with("http://") && !rpc_url.starts_with("https://") {
        return Err(ValidationError("Invalid RPC URL format"));
    }
    Ok(())
}
```

**Checks**:
- RPC URL is present
- RPC URL starts with http:// or https://
- Network passphrase is present

---

## Usage Patterns

### Pattern 1: Web Application

```rust
use skillsync_tools::Config;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load()?;
    
    // Create RPC client
    let rpc_client = create_soroban_client(&config.rpc_url)?;
    
    // Use in your application
    let response = rpc_client.call_contract().await?;
    
    Ok(())
}
```

### Pattern 2: CLI Tool

```rust
use skillsync_tools::{Config, Network};

fn main() -> Result<()> {
    let config = Config::load()?;
    
    // Warn if testnet (production check)
    if config.network == Network::Mainnet {
        eprintln!("⚠️  WARNING: Operating on MAINNET");
    }
    
    println!("Network: {}", config.network);
    println!("RPC: {}", config.rpc_url);
    
    Ok(())
}
```

### Pattern 3: Configuration Validation

```rust
use skillsync_tools::ConfigError;

fn validate_deployment() -> Result<()> {
    let config = match Config::load() {
        Ok(c) => c,
        Err(ConfigError::InvalidNetwork(net)) => {
            return Err(format!("Invalid network: {}", net).into());
        }
        Err(e) => return Err(e.into()),
    };
    
    if config.contract_id.is_none() {
        return Err("SOROBAN_CONTRACT_ID not configured".into());
    }
    
    Ok(())
}
```

---

## Testing Strategy

### Unit Tests

**Test Network Enum**:
```rust
#[test]
fn test_network_from_str() {
    assert_eq!(Network::from_str("testnet")?, Network::Testnet);
}

#[test]
fn test_network_defaults() {
    assert_eq!(
        Network::Testnet.default_rpc_url(),
        "https://soroban-testnet.stellar.org"
    );
}
```

**Test Validation**:
```rust
#[test]
fn test_validate_invalid_rpc_url() {
    assert!(Config::validate(
        &Network::Testnet,
        "ftp://example.com",
        "Test SDF Network ; September 2015"
    ).is_err());
}
```

**Test Configuration**:
```rust
#[test]
fn test_resolve_from_env() {
    // Set up env vars
    std::env::set_var("SOROBAN_NETWORK", "testnet");
    let config = Config::load()?;
    assert_eq!(config.network, Network::Testnet);
}
```

### Integration Tests

**Test Full Pipeline**:
```rust
#[test]
fn test_config_resolution_order() {
    // Set TOML, then override with env var
    // Verify env var takes precedence
}
```

---

## Error Handling Strategy

### No Panics in Config Loading

**Bad** ❌:
```rust
let config = serde_json::from_str(data).unwrap();  // Could panic
```

**Good** ✅:
```rust
let config = serde_json::from_str(data)
    .map_err(|e| ConfigError::ParseError(e.to_string()))?;
```

### Custom Error Types

**Every error is typed**:
```rust
match Config::load() {
    Err(ConfigError::InvalidNetwork(name)) => {
        eprintln!("Unknown network: {}", name);
    }
    Err(e) => eprintln!("Unexpected error: {}", e),
    Ok(config) => use_config(config),
}
```

### Error Messages

All errors include context:

```
Error: Invalid network: "custnet". Must be: testnet, mainnet, or sandbox
```

Not just:

```
Error: invalid network
```

---

## Performance Characteristics

### Load Time

- **Typical**: < 1ms (environment variables only)
- **With TOML**: < 5ms (includes file I/O)
- **Negligible** for most applications

### Memory

- `Network` enum: 8 bytes (discriminant)
- `Config` struct: ~200 bytes (including strings)
- No heap allocations beyond String data

### Caching

For long-running applications, cache the config:

```rust
// Instead of:
async fn handler() {
    let config = Config::load()?;  // Reloaded each request
}

// Do:
lazy_static::lazy_static! {
    static ref CONFIG: Config = Config::load().expect("Invalid config");
}

async fn handler() {
    use_config(&CONFIG);
}
```

---

## Security Considerations

### Secrets Management

**DO NOT**:
- Commit `.env` with signing keys
- Log complete configuration (redact secrets)
- Store signing keys in source code

**DO**:
- Use environment variables for secrets
- Validate inputs before use
- Fail safely on configuration errors
- Use separate credentials for each network

### Network Selection Safety

The system prevents common mistakes:

```rust
// This will error immediately
let config = Config::load()?;
if config.network == Network::Mainnet {
    // Explicit check instead of string comparison
}
```

### TOML Integrity

`soroban.toml` can be committed safely (no secrets):
- Network endpoints are public
- Passphrases are public knowledge
- Only environment variables contain secrets

---

## Deployment Scenarios

### Local Development

```bash
# Use sandbox or testnet
export SOROBAN_NETWORK=sandbox
cargo run -p skillsync-tools -- network show
```

### CI/CD Pipeline

```yaml
env:
  SOROBAN_NETWORK: testnet
  SOROBAN_ACCOUNT: ${{ secrets.TESTNET_ACCOUNT }}
  SOROBAN_SIGNING_KEY: ${{ secrets.TESTNET_KEY }}
```

### Production Deployment

```bash
# Use mainnet with separate secrets manager
export SOROBAN_NETWORK=mainnet
# Secrets loaded from AWS Secrets Manager, Azure Key Vault, etc.
```

---

## Extending the System

### Add a New Network

1. Add variant to `Network` enum:
```rust
pub enum Network {
    Testnet,
    Mainnet,
    Sandbox,
    MyCustomNetwork,  // New
}
```

2. Implement resolution in `from_str()`:
```rust
"mycustom" => Ok(Network::MyCustomNetwork),
```

3. Provide defaults:
```rust
pub fn default_rpc_url(&self) -> &'static str {
    match self {
        // ...
        Network::MyCustomNetwork => "https://...",
    }
}
```

4. Add profile to `soroban.toml`:
```toml
[profile.mycustom]
network = "mycustom"
rpc_url = "https://..."
network_passphrase = "..."
```

### Add Configuration Fields

1. Add to `Config` struct:
```rust
pub struct Config {
    // ... existing fields
    pub timeout_ms: u64,  // New
}
```

2. Load in `Config::load()`:
```rust
let timeout_ms = std::env::var("SOROBAN_TIMEOUT_MS")
    .ok()
    .and_then(|s| s.parse().ok())
    .unwrap_or(30000);
```

3. Add tests:
```rust
#[test]
fn test_timeout_default() {
    let config = Config::load()?;
    assert_eq!(config.timeout_ms, 30000);
}
```

---

## Related Files

- **Binary**: `crates/tools/src/main.rs`
- **Library**: `crates/tools/src/lib.rs`
- **Config Module**: `crates/tools/src/config.rs`
- **TOML Config**: `soroban.toml`
- **Env Template**: `.env.example`
- **User Guide**: `SOROBAN.md`

---

## References

- Rust error handling: https://doc.rust-lang.org/rust-by-example/error/index.html
- serde: https://serde.rs/
- thiserror: https://docs.rs/thiserror/
- toml: https://docs.rs/toml/
- dotenvy: https://docs.rs/dotenvy/
