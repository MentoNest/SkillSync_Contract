//! Multi-network Soroban configuration management
//!
//! This module provides typed, strongly-validated configuration for Soroban networks.
//! Configuration is resolved in priority order:
//!
//! 1. Environment variables (SOROBAN_*)
//! 2. soroban.toml profile selection
//! 3. Error if required fields are missing
//!
//! # Examples
//!
//! ```rust,no_run
//! use skillsync_tools::config::{Config, Network};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let config = Config::load()?;
//! println!("Network: {}", config.network);
//! println!("RPC URL: {}", config.rpc_url);
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;
use thiserror::Error;

/// Configuration error types
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    TomlError(#[from] toml::de::Error),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid network: {0}. Must be: testnet, mainnet, or sandbox")]
    InvalidNetwork(String),

    #[error("Missing SOROBAN_NETWORK environment variable and soroban.toml not found")]
    MissingNetworkConfig,

    #[error("Env var error: {0}")]
    EnvVar(#[from] std::env::VarError),

    #[error("Configuration validation failed: {0}")]
    ValidationError(String),
}

/// Soroban supported networks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    /// Stellar Testnet - for testing before mainnet
    Testnet,
    /// Stellar Mainnet - production
    Mainnet,
    /// Local Soroban Sandbox - for local development
    Sandbox,
}

impl Network {
    /// Get network as string
    pub fn as_str(&self) -> &'static str {
        match self {
            Network::Testnet => "testnet",
            Network::Mainnet => "mainnet",
            Network::Sandbox => "sandbox",
        }
    }

    /// Parse network from string
    pub fn from_str(s: &str) -> Result<Self, ConfigError> {
        match s.to_lowercase().as_str() {
            "testnet" => Ok(Network::Testnet),
            "mainnet" => Ok(Network::Mainnet),
            "sandbox" => Ok(Network::Sandbox),
            other => Err(ConfigError::InvalidNetwork(other.to_string())),
        }
    }

    /// Get default RPC URL for this network
    pub fn default_rpc_url(&self) -> &'static str {
        match self {
            Network::Testnet => "https://soroban-testnet.stellar.org",
            Network::Mainnet => "https://mainnet.sorobanrpc.com",
            Network::Sandbox => "http://localhost:8000",
        }
    }

    /// Get network passphrase for transaction signing
    pub fn passphrase(&self) -> &'static str {
        match self {
            Network::Testnet => "Test SDF Network ; September 2015",
            Network::Mainnet => "Public Global Stellar Network ; September 2015",
            Network::Sandbox => "Standalone Network ; February 2017",
        }
    }
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// TOML profile definition from soroban.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkProfile {
    pub network: String,
    pub rpc_url: String,
    pub network_passphrase: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// Complete Soroban configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SorobanToml {
    #[serde(default)]
    pub default: Option<DefaultProfile>,
    #[serde(default)]
    pub profile: std::collections::HashMap<String, NetworkProfile>,
}

/// Default profile section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultProfile {
    pub network: Option<String>,
}

/// Resolved runtime configuration with all required fields
#[derive(Debug, Clone)]
pub struct Config {
    /// Active network
    pub network: Network,
    /// RPC endpoint URL
    pub rpc_url: String,
    /// Network passphrase for signing
    pub network_passphrase: String,
    /// Contract ID (optional)
    pub contract_id: Option<String>,
    /// Account address (optional)
    pub account: Option<String>,
    /// RPC timeout in milliseconds
    pub rpc_timeout_ms: u64,
    /// Debug mode
    pub debug: bool,
}

impl Config {
    /// Load configuration from environment and soroban.toml
    ///
    /// # Resolution Order
    ///
    /// 1. Load SOROBAN_NETWORK from env
    /// 2. Load individual env vars (SOROBAN_RPC_URL, etc)
    /// 3. Load soroban.toml if it exists
    /// 4. Overlay env vars on top of TOML values
    /// 5. Validate required fields
    ///
    /// # Errors
    ///
    /// Returns `ConfigError` if:
    /// - No network is configured
    /// - Invalid network name is provided
    /// - Required fields are missing
    pub fn load() -> Result<Self, ConfigError> {
        // Load .env file if it exists (non-fatal)
        let _ = dotenvy::dotenv();

        // Determine active network
        let network_name = std::env::var("SOROBAN_NETWORK")
            .ok()
            .or_else(|| {
                // Try to get from soroban.toml default
                Self::load_toml()
                    .ok()
                    .and_then(|toml| {
                        toml.default
                            .and_then(|d| d.network)
                    })
            })
            .unwrap_or_else(|| "testnet".to_string());

        let network = Network::from_str(&network_name)?;

        // Load TOML profile as baseline
        let toml_config = Self::load_toml().ok();
        let profile = toml_config
            .as_ref()
            .and_then(|t| t.profile.get(network_name.as_str()));

        // Build config with env overrides
        let rpc_url = std::env::var("SOROBAN_RPC_URL")
            .ok()
            .or_else(|| profile.map(|p| p.rpc_url.clone()))
            .unwrap_or_else(|| network.default_rpc_url().to_string());

        let network_passphrase = std::env::var("SOROBAN_NETWORK_PASSPHRASE")
            .ok()
            .or_else(|| profile.map(|p| p.network_passphrase.clone()))
            .unwrap_or_else(|| network.passphrase().to_string());

        let contract_id = std::env::var("SOROBAN_CONTRACT_ID").ok();
        let account = std::env::var("SOROBAN_ACCOUNT").ok();
        let rpc_timeout_ms = std::env::var("SOROBAN_RPC_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30000);
        let debug = std::env::var("SOROBAN_DEBUG")
            .ok()
            .map(|s| s.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        // Validate
        Self::validate(&network, &rpc_url, &network_passphrase)?;

        Ok(Config {
            network,
            rpc_url,
            network_passphrase,
            contract_id,
            account,
            rpc_timeout_ms,
            debug,
        })
    }

    /// Load soroban.toml from workspace root
    fn load_toml() -> Result<SorobanToml, ConfigError> {
        let paths = [
            Path::new("soroban.toml"),
            Path::new("./soroban.toml"),
        ];

        for path in paths {
            if path.exists() {
                let content = std::fs::read_to_string(path)?;
                return toml::from_str(&content).map_err(ConfigError::TomlError);
            }
        }

        Err(ConfigError::MissingNetworkConfig)
    }

    /// Validate configuration values
    fn validate(
        _network: &Network,
        rpc_url: &str,
        passphrase: &str,
    ) -> Result<(), ConfigError> {
        if rpc_url.is_empty() {
            return Err(ConfigError::MissingField("rpc_url".to_string()));
        }

        if passphrase.is_empty() {
            return Err(ConfigError::MissingField("network_passphrase".to_string()));
        }

        // Validate RPC URL format
        if !rpc_url.starts_with("http://") && !rpc_url.starts_with("https://") {
            return Err(ConfigError::ValidationError(
                format!("RPC URL must start with http:// or https://: {}", rpc_url),
            ));
        }

        Ok(())
    }

    /// Print the resolved configuration
    pub fn print_summary(&self) {
        println!("╔════════════════════════════════════════════════════════════════╗");
        println!("║         SOROBAN NETWORK CONFIGURATION RESOLVED                 ║");
        println!("╚════════════════════════════════════════════════════════════════╝");
        println!("  Network:             {}", self.network);
        println!("  RPC URL:             {}", self.rpc_url);
        println!("  Network Passphrase:  {}", self.network_passphrase);

        if let Some(ref contract) = self.contract_id {
            println!("  Contract ID:         {}", contract);
        } else {
            println!("  Contract ID:         (not configured)");
        }

        if let Some(ref account) = self.account {
            println!("  Account:             {}", account);
        } else {
            println!("  Account:             (not configured)");
        }

        println!("  RPC Timeout:         {}ms", self.rpc_timeout_ms);

        if self.debug {
            println!("  Debug Mode:          ENABLED");
        }

        println!("╚════════════════════════════════════════════════════════════════╝");
    }

    /// Get configuration as JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

// Manual Serialize impl for Config since we want custom serialization
impl Serialize for Config {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(7))?;
        map.serialize_entry("network", &self.network.to_string())?;
        map.serialize_entry("rpc_url", &self.rpc_url)?;
        map.serialize_entry("network_passphrase", &self.network_passphrase)?;
        map.serialize_entry("contract_id", &self.contract_id)?;
        map.serialize_entry("account", &self.account)?;
        map.serialize_entry("rpc_timeout_ms", &self.rpc_timeout_ms)?;
        map.serialize_entry("debug", &self.debug)?;
        map.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn setup_test_env() {
        env::remove_var("SOROBAN_NETWORK");
        env::remove_var("SOROBAN_RPC_URL");
        env::remove_var("SOROBAN_NETWORK_PASSPHRASE");
        env::remove_var("SOROBAN_CONTRACT_ID");
        env::remove_var("SOROBAN_ACCOUNT");
    }

    #[test]
    fn test_network_from_str() {
        assert_eq!(Network::from_str("testnet").unwrap(), Network::Testnet);
        assert_eq!(Network::from_str("mainnet").unwrap(), Network::Mainnet);
        assert_eq!(Network::from_str("sandbox").unwrap(), Network::Sandbox);
        assert_eq!(
            Network::from_str("TESTNET").unwrap(),
            Network::Testnet
        );
    }

    #[test]
    fn test_network_invalid() {
        assert!(Network::from_str("invalid").is_err());
    }

    #[test]
    fn test_network_display() {
        assert_eq!(Network::Testnet.to_string(), "testnet");
        assert_eq!(Network::Mainnet.to_string(), "mainnet");
        assert_eq!(Network::Sandbox.to_string(), "sandbox");
    }

    #[test]
    fn test_network_default_rpc_urls() {
        assert_eq!(
            Network::Testnet.default_rpc_url(),
            "https://soroban-testnet.stellar.org"
        );
        assert_eq!(
            Network::Mainnet.default_rpc_url(),
            "https://mainnet.sorobanrpc.com"
        );
        assert_eq!(
            Network::Sandbox.default_rpc_url(),
            "http://localhost:8000"
        );
    }

    #[test]
    fn test_network_passphrases() {
        assert_eq!(
            Network::Testnet.passphrase(),
            "Test SDF Network ; September 2015"
        );
        assert_eq!(
            Network::Mainnet.passphrase(),
            "Public Global Stellar Network ; September 2015"
        );
        assert_eq!(
            Network::Sandbox.passphrase(),
            "Standalone Network ; February 2017"
        );
    }

    #[test]
    fn test_validate_missing_rpc_url() {
        let result = Config::validate(&Network::Testnet, "", "Test SDF Network ; September 2015");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_missing_passphrase() {
        let result = Config::validate(&Network::Testnet, "https://example.com", "");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_invalid_rpc_url() {
        let result =
            Config::validate(&Network::Testnet, "ftp://example.com", "Test SDF Network ; September 2015");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_success() {
        let result = Config::validate(
            &Network::Testnet,
            "https://soroban-testnet.stellar.org",
            "Test SDF Network ; September 2015",
        );
        assert!(result.is_ok());
    }
}
