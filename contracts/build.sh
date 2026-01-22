#!/bin/bash

# Build script for SkillSync contracts
echo "Building SessionGate contract..."

# Build the session gate contract
cargo contract build --package session_gate

echo "Build complete!"
echo "WASM file location: target/ink/session_gate.wasm"
echo "Metadata file location: target/ink/metadata.json"
