# Test Coverage Report

Generated on: 2026-05-12

Tool:

```bash
cargo-llvm-cov 0.8.6
```

Primary command for production smart-contract coverage:

```bash
cargo llvm-cov --workspace --lib --summary-only --ignore-filename-regex '(/test\.rs$|contracts/integration_tests/|contracts/mock_blend_pool/)'
```

Scope:

- Includes production smart-contract/library source files.
- Excludes unit test source files ending in `test.rs`.
- Excludes the `contracts/integration_tests` harness.
- Excludes `contracts/mock_blend_pool`, which is a mock-only test helper crate.

## Summary

| Metric | Coverage |
|---|---:|
| Line coverage | 97.51% |
| Region coverage | 96.56% |
| Function coverage | 87.69% |

## Production Coverage By File

| File | Region Coverage | Function Coverage | Line Coverage |
|---|---:|---:|---:|
| `asset_handler/src/lib.rs` | 99.68% | 100.00% | 99.50% |
| `asset_handler/src/storage.rs` | 97.63% | 90.00% | 97.94% |
| `factory/src/events.rs` | 100.00% | 100.00% | 100.00% |
| `factory/src/lib.rs` | 100.00% | 100.00% | 100.00% |
| `factory/src/storage.rs` | 96.45% | 93.75% | 97.45% |
| `oracle/src/lib.rs` | 100.00% | 100.00% | 100.00% |
| `oracle/src/storage.rs` | 89.19% | 63.64% | 92.45% |
| `oracle_adapters/dia/src/lib.rs` | 99.50% | 93.75% | 99.24% |
| `oracle_adapters/dia/src/storage.rs` | 92.31% | 78.95% | 94.74% |
| `oracle_adapters/reflector/src/lib.rs` | 99.12% | 88.24% | 98.48% |
| `oracle_adapters/reflector/src/storage.rs` | 91.53% | 77.78% | 94.59% |
| `share_token/src/events.rs` | 100.00% | 100.00% | 100.00% |
| `share_token/src/lib.rs` | 95.05% | 75.00% | 97.08% |
| `share_token/src/storage.rs` | 91.62% | 72.00% | 95.33% |
| `strategies/blend/src/lib.rs` | 94.20% | 89.47% | 96.09% |
| `strategies/blend/src/storage.rs` | 93.64% | 83.33% | 95.71% |
| `strategies/phoenix_lp/src/interfaces.rs` | 100.00% | 100.00% | 100.00% |
| `strategies/phoenix_lp/src/lib.rs` | 93.62% | 75.00% | 95.52% |
| `strategies/phoenix_lp/src/storage.rs` | 95.45% | 85.71% | 96.49% |
| `strategies/soroswap_lp/src/interfaces.rs` | 100.00% | 100.00% | 100.00% |
| `strategies/soroswap_lp/src/lib.rs` | 94.67% | 80.77% | 95.87% |
| `strategies/soroswap_lp/src/storage.rs` | 98.55% | 90.91% | 97.78% |
| `vault/src/events.rs` | 100.00% | 100.00% | 100.00% |
| `vault/src/lib.rs` | 97.05% | 88.39% | 97.55% |
| `vault/src/storage.rs` | 98.45% | 92.65% | 98.60% |
| **Total** | **96.56%** | **87.69%** | **97.51%** |

## Notes

- Branch coverage is not reported by this run because the generated report shows no branch counters for these Rust/Soroban targets.
- Coverage percentage is not a security guarantee. The strongest coverage remains the scenario coverage around vault accounting, fees, strategy positions, oracle fallbacks, share transfer controls, cooldown, and PnL behavior.
- The practical target is now 98%. The strict current denominator still includes Soroban constructor re-initialization guards, `#[contracttype]`/storage metadata, and generated helper functions that are not directly callable through generated clients; remaining work should prioritize meaningful protocol branches over synthetic coverage.
