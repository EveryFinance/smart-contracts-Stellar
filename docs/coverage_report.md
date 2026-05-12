# Test Coverage Report

Generated on: 2026-05-12

Tool:

```bash
cargo-llvm-cov 0.8.6
```

Primary command for production smart-contract coverage:

```bash
cargo llvm-cov report --summary-only --ignore-filename-regex '(/test\.rs$|contracts/integration_tests/|contracts/mock_blend_pool/)'
```

Scope:

- Includes production smart-contract/library source files.
- Excludes unit test source files ending in `test.rs`.
- Excludes the `contracts/integration_tests` harness.
- Excludes `contracts/mock_blend_pool`, which is a mock-only test helper crate.

## Summary

| Metric | Coverage |
|---|---:|
| Line coverage | 88.78% |
| Region coverage | 86.99% |
| Function coverage | 81.49% |

Additional reference runs:

| Scope | Line coverage |
|---|---:|
| Production contracts, excluding mock Blend pool | 88.78% |
| Production contracts, including mock Blend pool | 86.36% |
| Full workspace, including tests | 90.13% |

## Production Coverage By File

| File | Region Coverage | Function Coverage | Line Coverage |
|---|---:|---:|---:|
| `asset_handler/src/lib.rs` | 85.40% | 90.00% | 87.06% |
| `asset_handler/src/storage.rs` | 97.63% | 90.00% | 97.94% |
| `factory/src/events.rs` | 100.00% | 100.00% | 100.00% |
| `factory/src/lib.rs` | 92.90% | 92.59% | 92.80% |
| `factory/src/storage.rs` | 93.62% | 90.62% | 93.63% |
| `oracle/src/lib.rs` | 93.08% | 90.00% | 91.57% |
| `oracle/src/storage.rs` | 89.19% | 63.64% | 92.45% |
| `oracle_adapters/dia/src/lib.rs` | 90.55% | 93.75% | 94.66% |
| `oracle_adapters/dia/src/storage.rs` | 92.31% | 78.95% | 94.74% |
| `oracle_adapters/reflector/src/lib.rs` | 82.02% | 82.35% | 85.61% |
| `oracle_adapters/reflector/src/storage.rs` | 91.53% | 77.78% | 94.59% |
| `share_token/src/events.rs` | 100.00% | 100.00% | 100.00% |
| `share_token/src/lib.rs` | 90.64% | 69.44% | 93.97% |
| `share_token/src/storage.rs` | 88.83% | 72.00% | 93.33% |
| `strategies/blend/src/lib.rs` | 92.65% | 89.47% | 94.79% |
| `strategies/blend/src/storage.rs` | 93.64% | 83.33% | 95.71% |
| `strategies/phoenix_lp/src/interfaces.rs` | 69.81% | 70.00% | 67.02% |
| `strategies/phoenix_lp/src/lib.rs` | 82.84% | 71.43% | 86.32% |
| `strategies/phoenix_lp/src/storage.rs` | 95.45% | 85.71% | 96.49% |
| `strategies/soroswap_lp/src/interfaces.rs` | 77.31% | 72.73% | 86.73% |
| `strategies/soroswap_lp/src/lib.rs` | 78.22% | 73.08% | 80.24% |
| `strategies/soroswap_lp/src/storage.rs` | 98.55% | 90.91% | 97.78% |
| `vault/src/events.rs` | 75.61% | 75.00% | 70.37% |
| `vault/src/lib.rs` | 83.66% | 75.89% | 84.28% |
| `vault/src/storage.rs` | 95.53% | 89.71% | 96.35% |
| **Total** | **86.99%** | **81.49%** | **88.78%** |

## Notes

- Branch coverage is not reported by this run because the generated report shows no branch counters for these Rust/Soroban targets.
- Coverage percentage is not a security guarantee. The strongest coverage remains the scenario coverage around vault accounting, fees, strategy positions, oracle fallbacks, share transfer controls, cooldown, and PnL behavior.
