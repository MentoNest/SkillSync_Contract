# Release Process Documentation

This document explains how to cut releases for SkillSync Contract and verify artifact integrity.

## Overview

This project uses a deterministic, reproducible build process for WASM smart contract artifacts:

- **Pinned Rust toolchain** ensures consistency across machines
- **Locked dependencies** (`Cargo.lock`) guarantee identical builds
- **Disabled incremental compilation** prevents stale artifacts  
- **SHA256 checksums** enable integrity verification
- **Reproducibility verification** in CI prevents supply chain compromise

---

## Prerequisites for Local Development

Install the required tools for building and verifying releases:

```bash
# Install Rust (uses rust-toolchain.toml for version pinning)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -t stable -c rustfmt,clippy

# Explicitly add wasm32 target
rustup target add wasm32-unknown-unknown

# Install WASM optimization tools
cargo install wasm-opt
cargo install wasm-tools
```

**Verify installation:**

```bash
rustc --version
cargo --version
wasm-opt --version
wasm-tools --version
```

---

## How to Cut a Release

### Step 1: Prepare the Code

Ensure all commits are on `main` (or your release branch):

```bash
git checkout main
git pull origin main
```

### Step 2: Verify Tests Pass

```bash
cargo test --all
cargo build --all --release
```

### Step 3: Determine the Version

Use [Semantic Versioning](https://semver.org/):

- `v1.2.3` - patch release (bug fixes)
- `v1.2.0` - minor release (new features, backward compatible)
- `v2.0.0` - major release (breaking changes)

### Step 4: Create an Annotated Git Tag

**Option A: Latest commit**

```bash
git tag -a v0.2.0 -m "Release v0.2.0: Description of changes"
```

**Option B: Specific commit**

```bash
git tag -a v0.2.0 <commit-hash> -m "Release v0.2.0: Description of changes"
```

### Step 5: Push the Tag to Trigger Workflow

```bash
git push origin v0.2.0
```

This triggers the `.github/workflows/release.yml` workflow in GitHub Actions.

### Step 6: Monitor the Workflow

1. Go to: `https://github.com/<owner>/<repo>/actions`
2. Select the "Release WASM Artifacts" workflow
3. Monitor the build:
   - ✓ **Build WASM artifact** - compiles with optimizations  
   - ✓ **Strip WASM binary** - removes debug info
   - ✓ **Optimize WASM** - applies wasm-opt -Oz
   - ✓ **Verify build reproducibility** - rebuilds and checks SHA256
   - ✓ **Upload artifacts** - publishes to GitHub Release

The workflow **fails immediately if reproducibility check fails**.

### Step 7: Verify Release

1. Go to: `https://github.com/<owner>/<repo>/releases`  
2. Confirm artifacts are present:
   - `core-v0.2.0.wasm` - optimized smart contract WASM binary
   - `checksums.txt` - SHA256 checksums

---

## How to Verify Checksums Locally

After downloading artifacts from a GitHub Release:

### Verify Single Artifact

```bash
sha256sum core-v0.2.0.wasm
# Compare against the value in checksums.txt
```

### Verify All Artifacts (Recommended)

```bash
# Download checksums.txt from the release
# Place it in the same directory as the artifacts

sha256sum -c checksums.txt

# Expected output:
# core-v0.2.0.wasm: OK
```

### Fail on Verification Error

```bash
sha256sum -c checksums.txt || exit 1
```

---

## How to Reproduce the Build Locally

To verify you can rebuild the exact same binary:

### Step 1: Clone the Repository at the Release Tag

```bash
git clone https://github.com/<owner>/<repo>.git skillsync-contract
cd skillsync-contract
git checkout v0.2.0  # Replace with your release version
```

### Step 2: Verify Toolchain is Pinned

```bash
cat rust-toolchain.toml
# Should output:
# [toolchain]
# channel = "stable"
# targets = ["wasm32-unknown-unknown"]
# components = ["rustfmt", "clippy"]
```

### Step 3: Build with Deterministic Settings

```bash
# Disable incremental compilation
export CARGO_INCREMENTAL=0
export RUSTFLAGS="-C embed-bitcode=no"

# Build (--locked ensures Cargo.lock is respected)
cargo build -p skillsync-core \
  --target wasm32-unknown-unknown \
  --release \
  --locked
```

### Step 4: Process the WASM Binary

Strip debug information:

```bash
wasm-tools strip \
  target/wasm32-unknown-unknown/release/skillsync_core.wasm \
  -o skillsync_core_stripped.wasm

mv skillsync_core_stripped.wasm \
   target/wasm32-unknown-unknown/release/skillsync_core.wasm
```

Optimize:

```bash
wasm-opt -Oz \
  target/wasm32-unknown-unknown/release/skillsync_core.wasm \
  -o skillsync_core_opt.wasm

mv skillsync_core_opt.wasm \
   target/wasm32-unknown-unknown/release/skillsync_core.wasm
```

### Step 5: Compute Checksum

```bash
sha256sum target/wasm32-unknown-unknown/release/skillsync_core.wasm
```

### Step 6: Compare Against Release Checksum

```bash
# From the GitHub Release, get the checksum value
# For example: a1b2c3d4...

EXPECTED="a1b2c3d4..."
COMPUTED=$(sha256sum target/wasm32-unknown-unknown/release/skillsync_core.wasm | cut -d' ' -f1)

if [ "$EXPECTED" = "$COMPUTED" ]; then
  echo "✓ Build reproducibility verified!"
else
  echo "✗ Checksums DO NOT match!"
  echo "Expected: $EXPECTED"
  echo "Got:      $COMPUTED"
  exit 1
fi
```

### Quick Reproduction Script

Save as `verify-release.sh`:

```bash
#!/bin/bash
set -euo pipefail

VERSION="${1:?Usage: $0 <version> [artifacts_dir]}"
ARTIFACTS_DIR="${2:-.}"

echo "Verifying release $VERSION"

# Clone and checkout
REPO_DIR="/tmp/skillsync-verify-$VERSION"
rm -rf "$REPO_DIR"
git clone https://github.com/<owner>/<repo>.git "$REPO_DIR"
cd "$REPO_DIR"
git checkout "$VERSION"

# Build
export CARGO_INCREMENTAL=0
export RUSTFLAGS="-C embed-bitcode=no"
cargo build -p skillsync-core \
  --target wasm32-unknown-unknown \
  --release \
  --locked

# Process
wasm-tools strip \
  target/wasm32-unknown-unknown/release/skillsync_core.wasm \
  -o /tmp/wasm_stripped.wasm
mv /tmp/wasm_stripped.wasm \
   target/wasm32-unknown-unknown/release/skillsync_core.wasm

wasm-opt -Oz \
  target/wasm32-unknown-unknown/release/skillsync_core.wasm \
  -o /tmp/wasm_opt.wasm
mv /tmp/wasm_opt.wasm \
   target/wasm32-unknown-unknown/release/skillsync_core.wasm

# Verify
COMPUTED=$(sha256sum target/wasm32-unknown-unknown/release/skillsync_core.wasm | cut -d' ' -f1)
EXPECTED=$(cat "$ARTIFACTS_DIR/checksums.txt" | grep core | cut -d' ' -f1)

echo "Expected checksum: $EXPECTED"
echo "Computed checksum: $COMPUTED"

if [ "$EXPECTED" = "$COMPUTED" ]; then
  echo "✓ Release $VERSION verified successfully!"
  exit 0
else
  echo "✗ Verification FAILED: checksums do not match"
  exit 1
fi
```

Usage:

```bash
chmod +x verify-release.sh
./verify-release.sh v0.2.0 ./downloaded-artifacts
```

---

## Minimal Cargo Configuration for Reproducibility

The workspace already includes optimal settings in `Cargo.toml`:

```toml
[profile.release]
opt-level = "z"           # Optimize for size (ideal for WASM)
overflow-checks = true    # Safety checks even in release
debug = 0                 # Strip debug info
strip = true              # Strip symbols
debug-assertions = false  # Disable debug assertions
panic = "abort"           # Faster panic handling
codegen-units = 1         # Single-threaded codegen (deterministic)
lto = true                # Link-time optimization
```

**DO NOT modify these settings** unless you understand the reproducibility implications.

---

## Troubleshooting

### Checksums Don't Match

**Cause**: Different build environments or settings.

**Solution**:
1. Verify you're on the correct git tag: `git describe --tags`
2. Verify toolchain: `rustup show` (must match tag's `rust-toolchain.toml`)
3. Verify WASM tools versions: `wasm-opt --version`, `wasm-tools --version`
4. Check you're using `--locked`: `cargo +stable build -p skillsync-core --target wasm32-unknown-unknown --release --locked`
5. Ensure `CARGO_INCREMENTAL=0` and `RUSTFLAGS` are set

### Build Fails in CI

Check the GitHub Actions logs at: `https://github.com/<owner>/<repo>/actions`

Common issues:
- Rust toolchain install failed: check internet connectivity
- WASM optimization failed: check wasm-opt installation
- Reproducibility verification failed: see logs for checksum mismatch

---

## Security Considerations

1. **Artifact Signing**: Consider adding GPG signatures to checksums.txt for production:
   ```bash
   gpg --clearsign checksums.txt
   ```

2. **Build Machine Integrity**: CI uses fresh Ubuntu container (guaranteed clean environment)

3. **Dependency Pinning**: `Cargo.lock` is committed and `--locked` enforces its use

4. **Supply Chain Security**: Reproducible builds enable detection of tampering

---

## CI/CD Pipeline Details

The workflow (.github/workflows/release.yml) performs:

1. **Checkout** - fetch source at tag
2. **Install Toolchain** - uses dtolnay/rust-toolchain@stable with pinned targets
3. **Cache Dependencies** - Swatinem/rust-cache for faster rebuilds
4. **Install Tools** - wasm-opt, wasm-tools
5. **Build** - with CARGO_INCREMENTAL=0, --locked, and deterministic flags
6. **Strip** - remove all symbols and debug info
7. **Optimize** - wasm-opt -Oz for size
8. **Checksum** - generate SHA256
9. **Verify Reproducibility** - rebuild and compare checksums (exits 1 on mismatch)
10. **Upload** - softprops/action-gh-release with checksums.txt and WASM binary

---

## FAQ

**Q: Can I release from any branch?**  
A: No. The workflow only triggers on tags (`push: tags: v*`). Create and push a tag to trigger.

**Q: How do I release from a non-main branch?**  
A: Just push the tag from any branch. The tag points to a specific commit regardless of branch.

**Q: What if I need to re-release the same version?**  
A: Delete the old tag locally and remotely, then recreate:
```bash
git tag -d v0.2.0
git push origin :v0.2.0  # Delete remote tag
git tag -a v0.2.0 -m "Release v0.2.0"
git push origin v0.2.0
```

**Q: Are builds truly reproducible?**  
A: Yes, the workflow verifies by: rebuilding locally in same CI, recomputing checksum, comparing checksums, failing if they differ.

**Q: Can I modify compiler settings?**  
A: Only if you update Cargo.toml and understand the reproducibility impact. All users must use identical settings.

---

## Related Documentation

- [Cargo Book - Profiles](https://doc.rust-lang.org/cargo/reference/profiles.html)
- [Soroban Documentation](https://soroban.stellar.org/)
- [wasm-opt Documentation](https://github.com/binaryen/binaryen)
- [Reproducible Builds](https://reproducible-builds.org/)
