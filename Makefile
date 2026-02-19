.PHONY: help build test fmt lint wasm clean check-deps install-deps

# Default target
help:
	@echo "SkillSync Soroban Development Commands:"
	@echo ""
	@echo "  install-deps    Install all required dependencies"
	@echo "  check-deps      Verify dependencies are installed"
	@echo "  build           Build all workspace members"
	@echo "  wasm            Build contract for WASM target"
	@echo "  test            Run all tests"
	@echo "  fmt             Format code with rustfmt"
	@echo "  lint            Run clippy lints"
	@echo "  clean           Clean build artifacts"
	@echo "  help            Show this help message"

# Install dependencies
install-deps:
	@echo "Installing Rust toolchain..."
	rustup toolchain install stable
	rustup target add wasm32-unknown-unknown
	rustup component add rustfmt clippy
	@echo "Installing Soroban CLI..."
	cargo install soroban-cli
	@echo "Dependencies installed successfully!"

# Check if dependencies are installed
check-deps:
	@echo "Checking Rust toolchain..."
	@rustup --version || (echo "❌ Rust not installed. Run 'make install-deps'" && exit 1)
	@echo "Checking WASM target..."
	@rustup target list --installed | grep wasm32-unknown-unknown || (echo "❌ WASM target not installed. Run 'make install-deps'" && exit 1)
	@echo "Checking Soroban CLI..."
	@soroban --version || (echo "❌ Soroban CLI not installed. Run 'make install-deps'" && exit 1)
	@echo "✅ All dependencies are installed!"

# Build all workspace members
build:
	@echo "Building workspace..."
	cargo build --release
	@echo "✅ Build completed!"

# Build contract for WASM target
wasm:
	@echo "Building contract for WASM..."
	cargo build -p skillsync-core --target wasm32-unknown-unknown --release
	@echo "✅ WASM build completed!"
	@echo "📦 Artifact: target/wasm32-unknown-unknown/release/skillsync_core.wasm"

# Run tests
test:
	@echo "Running tests..."
	cargo test --release
	@echo "✅ Tests completed!"

# Format code
fmt:
	@echo "Formatting code..."
	cargo fmt --all
	@echo "✅ Code formatted!"

# Run lints
lint:
	@echo "Running clippy lints..."
	cargo clippy --all-targets --all-features -- -D warnings
	@echo "✅ Linting completed!"

# Clean build artifacts
clean:
	@echo "Cleaning build artifacts..."
	cargo clean
	@echo "✅ Clean completed!"
