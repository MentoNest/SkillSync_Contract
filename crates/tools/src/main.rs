mod config;

use anyhow::Result;
use clap::{Parser, Subcommand};
use config::Config;

#[derive(Parser)]
#[command(name = "skillsync")]
#[command(about = "SkillSync CLI tools for contract deployment and management")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage network configuration
    Network {
        #[command(subcommand)]
        action: Option<NetworkAction>,
    },
    /// Deploy the SkillSync contract
    Deploy {
        /// Network to deploy to (testnet/mainnet/sandbox)
        #[arg(short, long)]
        network: Option<String>,
        /// Contract WASM file path
        #[arg(short, long)]
        wasm: String,
    },
    /// Check and display configuration
    Config {
        /// Format output as JSON
        #[arg(short, long)]
        json: bool,
        /// Validate configuration files
        #[arg(short, long)]
        validate: bool,
    },
    /// Build contracts
    Build {
        /// Build target (release/debug)
        #[arg(short, long, default_value = "release")]
        profile: String,
    },
}

#[derive(Subcommand)]
enum NetworkAction {
    /// Show active network configuration
    Show,
    /// List available networks
    List,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Network { action } => match action {
            Some(NetworkAction::Show) => {
                let config = Config::load()?;
                config.print_summary();
                Ok(())
            }
            Some(NetworkAction::List) => {
                println!("╔════════════════════════════════════════════════════════════════╗");
                println!("║            AVAILABLE SOROBAN NETWORKS                          ║");
                println!("╚════════════════════════════════════════════════════════════════╝");
                println!("  testnet   - Stellar Testnet (for testing)");
                println!("  mainnet   - Stellar Mainnet (production)");
                println!("  sandbox   - Local Soroban Sandbox (localhost:8000)");
                println!();
                println!("To select a network:");
                println!("  export SOROBAN_NETWORK=testnet");
                println!("  cargo run -p skillsync-tools -- network show");
                Ok(())
            }
            None => {
                let config = Config::load()?;
                config.print_summary();
                Ok(())
            }
        },
        Commands::Deploy { network, wasm } => {
            let config = match network {
                Some(net) => {
                    // Override network from command line
                    std::env::set_var("SOROBAN_NETWORK", net.clone());
                    Config::load()?
                }
                None => Config::load()?,
            };

            println!("Deploying contract to {} network", config.network);
            println!("WASM file: {}", wasm);
            println!("RPC URL: {}", config.rpc_url);
            // TODO: Implement deployment logic
            Ok(())
        }
        Commands::Config { json, validate } => {
            let config = Config::load()?;

            if validate {
                println!("✓ Configuration is valid");
            }

            if json {
                println!("{}", config.to_json()?);
            } else {
                config.print_summary();
            }

            Ok(())
        }
        Commands::Build { profile } => {
            println!("Building contracts with {} profile", profile);
            // TODO: Implement build logic
            Ok(())
        }
    }
}
