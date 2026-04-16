# Reviewer Quickstart

This guide is for external reviewers (including Stellar ecosystem reviewers) to compile, test, and run the protocol demo quickly.

## 1. Prerequisites

- Rust toolchain installed
- `wasm32-unknown-unknown` target installed
- Stellar CLI installed (`stellar --version`)

```bash
rustup target add wasm32-unknown-unknown
```

## 2. Clone and Build

```bash
git clone https://github.com/EveryFinance/smart-contracts-Stellar.git
cd smart-contracts-Stellar

# Native build
cargo build --workspace

# WASM build for Soroban contracts
cargo build --workspace --target wasm32-unknown-unknown --release
```

## 3. Quality Checks (same as CI)

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
```

## 4. Unit + Integration Tests

```bash
cargo test --workspace
```

Optional targeted runs:

```bash
cargo test -p vault
cargo test -p integration-tests
```

## 5. Demo Execution

### Full testnet deployment + validation

```bash
./scripts/run_showcase_demo.sh deploy
```

### Report generation from latest existing deployment (no new deployment)

```bash
./scripts/run_showcase_demo.sh reuse-latest
```

Outputs:

- `SHOWCASE_ENV=...`
- `SHOWCASE_REPORT=...`

## 6. Key References

- Main protocol documentation: `docs/README.md`
- Showcase vaults: `docs/vaults_alpha_beta_gamma.md`
- Demo runbook: `docs/demo_runbook.md`
- Contribution and local quality gates: `CONTRIBUTING.md`
