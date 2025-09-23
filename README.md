 SkillSync Smart Contract (Rust / ink!)
   A decentralized smart contract system for managing freelance work agreements and payments using Rust (ink!) on Substrate smart-contracts-compatible chains.

## Prerequisites
   Before you begin, ensure you have:
      1. Rust toolchain (stable) + rustup (>= 1.56 recommended)
      2. cargo and rustc installed
      3. cargo-contract (for building & deploying ink! contracts)
      4. substrate-contracts-node or Parity Canvas node for a local dev chain
      5. wasm32-unknown-unknown target
      6. cargo-make (optional, for helper tasks)

## Installation

* Install rustup (if needed)
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   rustup update stable
   rustup target add wasm32-unknown-unknown

* Install cargo-contract (ink! tool)
   cargo install cargo-contract --vers ^2.0.0

* (Optional) Install cargo-make
   cargo install cargo-make
  
# To run a local dev node:
   * Get a node for local dev (one-off)
      git clone https://github.com/paritytech/substrate-contracts-node
      cd substrate-contracts-node
      cargo build --release
   * Run node in a separate terminal:
      ./target/release/substrate-contracts-node --dev



## Project Structure

```
contracts/
├── Cargo.toml           # workspace or contract top-level (if multiple modules)
├── src/
│   ├── lib.rs           # main ink! contract entry (SkillSync)
│   ├── modules/         # modular contract components (separate Rust modules)
│   │   ├── user.rs
│   │   ├── escrow.rs
│   │   ├── reputation.rs
│   │   └── dispute.rs
│   └── utils.rs
├── tests/
│   └── unit_tests.rs    # Rust unit tests (ink! off-chain tests)
├── README.md            # This file
└── .env.example         # Environment / deployment template (RPC endpoints, accounts)

```

## Development BUILD & TEST
   # Build the contract to wasm + metadata
   cargo +stable contract build
   
   # Run unit tests (off-chain/in-memory)
   cargo test
   
   # If using cargo-make, optionally:
   # cargo make build



### DEPLOYMENT

# deploy command (example)
cargo +stable contract instantiate \
  --constructor new \
  --suri "//Alice" \
  --endowment 1000000000000000 \
  --salt 0x00 \
  --manifest-path target/ink/metadata.json \
  --wasm target/ink/skill_sync.wasm



