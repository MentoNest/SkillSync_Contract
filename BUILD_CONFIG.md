# Build & Release Configuration Guide

## Development Dependencies

### Required for Building and Releasing

```bash
# Core Rust (stable, with wasm32 target)
# Already configured in rust-toolchain.toml
rustup default stable
rustup target add wasm32-unknown-unknown
rustup component add rustfmt clippy
```

### WASM Optimization Tools

```bash
# WASM optimizer (binaryen)
# Required for: wasm-opt -Oz step
cargo install wasm-opt --version 0.116

# WASM utilities (wabt)
# Required for: wasm-strip and introspection
cargo install wasm-tools --version 1.209
```

### Verification (Optional)

```bash
# For checksum verification
# Built into most Unix/Linux systems and Windows (WSL2)
which sha256sum

# macOS alternative
which shasum  # Use: shasum -a 256
```

---

## Cargo Configuration: Determinism Settings

### Workspace Root: `Cargo.toml`

The release profile is **already optimally configured**:

```toml
[profile.release]
opt-level = "z"           # ⚙️  Optimize for size (critical for smart contracts)
overflow-checks = true    # 🔒 Safety checks (catch bugs in release builds)
debug = 0                 # 📊 Strip debug symbols
strip = true              # 🗑️  Remove all symbols
debug-assertions = false  # ⚡ Allow assertions to be compiled out
panic = "abort"           # 💥 Fast, deterministic panic handling
codegen-units = 1         # 🔐 CRITICAL: Single-threaded codegen = determinism
lto = true                # ⚡ Link-time optimization
```

**Why `codegen-units = 1`?**
- Default is 16 (parallel compilation)
- Parallel compilation introduces non-determinism
- Single-threaded forces deterministic ordering
- Small performance cost, massive reproducibility gain

### Contract Package: `crates/contracts/core/Cargo.toml`

```toml
[lib]
crate-type = ["cdylib"]  # ✓ Builds C-compatible dynamic library (.wasm)
```

This MUST remain `cdylib` for Soroban smart contracts.

---

## Environment Variables for Deterministic Builds

These are automatically set in the CI/CD workflow but you should use them locally:

```bash
# DISABLE INCREMENTAL COMPILATION
# Incremental builds cache intermediate artifacts non-deterministically
export CARGO_INCREMENTAL=0

# STRIP BITCODE FROM RLIB FILES
# Bitcode can differ between compilation sessions
export RUSTFLAGS="-C embed-bitcode=no"
```

**Why these matter:**
- Incremental compilation caches are architecture-specific
- Bitcode embedding depends on LLVM version and compiler state
- Both prevent reproducible rebuilds

---

## Verified Tool Versions

For maximum compatibility, use these versions:

| Tool | Version | Install |
|------|---------|---------|
| Rust (stable) | 2024.11 (pins via rust-toolchain.toml) | `rustup default stable` |
| Cargo | Bundled with Rust | - |
| wasm-opt | 0.116+ | `cargo install wasm-opt@0.116` |
| wasm-tools | 1.209+ | `cargo install wasm-tools@1.209` |
| sha256sum | standard | macOS: use `shasum -a 256` |

Check versions:

```bash
rustc --version
cargo --version
wasm-opt --version
wasm-tools --version
```

---

## Key Flags Explained

### Build Flags (Cargo)

| Flag | Purpose | Why for Release |
|------|---------|-----------------|
| `--release` | Optimized binary | Better performance, smaller code |
| `--locked` | Respect Cargo.lock | **CRITICAL for reproducibility** |
| `-p skillsync-core` | Build specific package | Avoids building unnecessary tools |
| `--target wasm32-unknown-unknown` | Cross-compile to WASM | Target platform for smart contracts |

### Compiler Flags (RUSTFLAGS)

| Flag | Purpose |
|------|---------|
| `-C embed-bitcode=no` | Don't embed LLVM bitcode in rlib files |
| `-C codegen-units=1` | Single-threaded codegen (deterministic) |

The `codegen-units=1` is already in `Cargo.toml`, but `-C embed-bitcode=no` is crucial to set via environment.

### WASM Optimization Flags

| Tool | Flag | Effect |
|------|------|--------|
| `wasm-tools strip` | - | Remove all symbols (debug info, function names) |
| `wasm-opt` | `-Oz` | Optimize for size (many rounds of aggressive optimizations) |

---

## Cargo.lock Importance

✅ **MUST be committed to git**

The `Cargo.lock` file:
- Locks all transitive dependencies to specific versions
- Ensures identical builds across machines
- Required for reproducible smart contract builds
- Checked by `cargo build --locked`

**To regenerate after dependency changes:**

```bash
rm Cargo.lock
cargo update
git add Cargo.lock
git commit -m "Update cargo dependencies"
```

---

## Build Output Artifacts

### Location After `cargo build`

```
target/wasm32-unknown-unknown/release/skillsync_core.wasm
  ↓ (strip + optimize)
  → core-<VERSION>.wasm (release artifact)
```

### Size Progression

| Stage | Expected Size | Notes |
|-------|---------------|-------|
| After cargo build | ~100-200KB | Includes debug symbols |
| After wasm-tools strip | ~80-150KB | Symbols removed |
| After wasm-opt -Oz | ~40-100KB | Aggressively optimized |

(Exact sizes depend on contract source)

---

## Common Issues & Solutions

### Issue: "Build not reproducible"

**Check:**
```bash
# 1. Git is clean
git status

# 2. Correct tag
git describe --tags

# 3. Exact Rust version
rustup show

# 4. Environment variables
echo $CARGO_INCREMENTAL  # should be 0
echo $RUSTFLAGS          # should include -C embed-bitcode=no

# 5. Tool versions
wasm-opt --version
wasm-tools --version
```

**Fix:**
```bash
cargo clean
export CARGO_INCREMENTAL=0
export RUSTFLAGS="-C embed-bitcode=no"
cargo build -p skillsync-core \
  --target wasm32-unknown-unknown \
  --release \
  --locked
```

### Issue: "wasm-opt command not found"

```bash
cargo install wasm-opt
export PATH="$HOME/.cargo/bin:$PATH"
which wasm-opt
```

### Issue: "Checksum mismatch between CI and local"

**Possible causes:**
- Different wasm-opt or wasm-tools versions
- `Cargo.lock` not committed or not used with `--locked`
- Different Rust toolchain (check `rustup default`)
- Incremental compilation interfering (set `CARGO_INCREMENTAL=0`)

**Fix:**
```bash
# Ensure you're on the exact commit
git checkout <tag>

# Clean previous builds
cargo clean

# Set environment
export CARGO_INCREMENTAL=0
export RUSTFLAGS="-C embed-bitcode=no"

# Rebuild
cargo build -p skillsync-core \
  --target wasm32-unknown-unknown \
  --release \
  --locked
```

---

## Advanced: Custom Profiles

If you need additional profiles (not recommended for releases):

```toml
[profile.release-with-logs]
inherits = "release"
debug-assertions = true  # Keep debug assertions but use release optimizations
```

Build with:
```bash
cargo build -p skillsync-core \
  --target wasm32-unknown-unknown \
  --profile release-with-logs \
  --locked
```

---

## CI/CD Integration

The GitHub Actions workflow automatically:

1. ✓ Installs `dtolnay/rust-toolchain@stable` (pins stable channel)
2. ✓ Adds `wasm32-unknown-unknown` target
3. ✓ Uses `Swatinem/rust-cache` for dependency caching
4. ✓ Sets `CARGO_INCREMENTAL=0` and `RUSTFLAGS="-C embed-bitcode=no"`
5. ✓ Installs `wasm-opt` and `wasm-tools`
6. ✓ Builds with `--locked`
7. ✓ Strips and optimizes WASM
8. ✓ Verifies reproducibility by rebuilding

**You should not need to change any of these settings.**

---

## Checklist Before Release

- [ ] All tests pass: `cargo test --all`
- [ ] Code compiles: `cargo build --all`
- [ ] WASM builds: `cargo build -p skillsync-core --target wasm32-unknown-unknown --release`
- [ ] Git is clean: `git status` shows no uncommitted changes
- [ ] You're on main: `git branch` shows `*main`
- [ ] Latest main is pulled: `git pull origin main`
- [ ] Decide version number (semantic versioning)
- [ ] Tag is created and pushed: `git tag -a v0.2.0 -m "..."` then `git push origin v0.2.0`
- [ ] Watch CI/CD: https://github.com/<owner>/<repo>/actions
- [ ] Verify checksums match: download artifacts and run `sha256sum -c checksums.txt`

---

## References

- [Reproducible Builds](https://reproducible-builds.org/)
- [Rust Book: Profiles](https://doc.rust-lang.org/cargo/reference/profiles.html)
- [Binaryen/wasm-opt](https://github.com/binaryen/binaryen)
- [Soroban Contracts](https://soroban.stellar.org/)
- [Cargo.lock article](https://doc.rust-lang.org/nightly/cargo/guide/cargo-lock.html)
