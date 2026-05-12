# Security Documentation

## Security Objectives

1. Preserve depositor fairness in share accounting.
2. Preserve NAV correctness under adversarial conditions.
3. Restrict privileged operations to intended roles.
4. Bound economic loss per operation.
5. Prevent unsafe trade paths and token exposures.

## Threat Model

Adversarial assumptions include compromise of:
- `manager`,
- `trader`,
- `oracle admin`,
- `factory admin`.

Security design assumes code-level controls plus operational governance controls.

## Key Controls

### Access Control

- Manager-only mutators live on the vault/factory/oracle configuration layer; strategy guards do not store their own manager.
- Trader-only trade execution in vault.
- Vault-only validation caller in guards.
- Admin-only mint/admin transfer in share token.
- Optional private-pool mode with member allowlist for deposit gating.

### Share Control Hardening

- Vault initialization requires deterministic share-token admin takeover.
- Mismatch between expected and actual share-token admin causes hard revert (`ShareTokenAdminMismatch`).
- Shares are non-transferable by default.
- `transfer` and `transfer_from` are blocked until transferability is explicitly
  enabled by the vault admin.
- Share burns are admin/vault-only and delegated `burn_from` is disabled, so
  redemption stays on the vault withdrawal path.
- Default non-transferability preserves account-based exit cooldown and accurate
  per-user PnL tracking.
- If share transfers are enabled, cooldown is no longer a hard control and PnL
  is approximate / informational only. Integrators can check
  `exit_cooldown_is_hard_control()` and `pnl_tracking_is_accurate()`.

### Oracle Freshness

- Price entries include `updated_ledger`.
- Oracle reads fail with `StalePrice` when older than `max_age_ledgers`.

### Trade Risk Controls

- Per-strategy guard required for `execute_trade`.
- Soroswap guard enforces exact-in and exact-out quote-relative slippage.
- Phoenix guard enforces slippage, whitelist, and hop/operations limits.

### Portfolio Risk Controls

- `max_concentration_bps` caps single-strategy NAV share.
- `max_loss_bps` NAV guard limits loss per manager/trader operation.
- deposit cap optional via `set_deposit_cap`.
- exit cooldown optional via `set_exit_cooldown_secs`.
- same-ledger value guard optional via `set_value_guard_enabled`.

## Fee and Economic Safety

- Fee caps enforced at initialize and setter calls:
  - entry/exit <= 500 bps,
  - management <= 300 bps,
  - performance <= 3000 bps.
- Fee increases are timelocked:
  - `announce_fee_increase` -> wait delay -> `commit_fee_increase`.
  - direct setters allow immediate decreases only.
- arithmetic checks use safe math + explicit overflow paths.

## Known Residual Risks

1. Oracle governance risk:
   - admin can set arbitrary prices,
   - `max_age_ledgers = 0` can disable freshness checks.
2. LP liquidity operational risk:
   - LP-heavy portfolios may require manual unwind before some withdrawals.
3. Role concentration risk:
   - if one key controls multiple roles, blast radius expands.
4. Transferable-share mode risk:
   - if a vault admin enables share transfers, users can move shares away from
     the original depositor account, so cooldown becomes same-address friction
     and PnL reporting is no longer exact.

## Production Governance Requirements

1. Separate manager/trader/oracle-admin/factory-admin roles.
2. Put privileged roles behind multisig + timelock.
3. Keep non-zero `max_loss_bps` and non-zero oracle max age.
4. Require guard wiring before enabling live trading.
5. Maintain emergency pause and key-rotation playbooks.
6. Keep shares non-transferable for vaults that rely on cooldown as a hard
   security control or exact user-level PnL reporting.

## Security Testing Coverage

Current test coverage includes:
- unit tests per contract,
- integration tests for vault lifecycle, strategy flows, and trade guards,
- stale-price behavior tests,
- slippage rejection tests (including exact-out on Soroswap).
- private-pool/member gating tests.
- cooldown enforcement tests.
- fee-increase timelock tests.
- management, performance, entry, and exit fee accounting tests.
- permissionless `collect_pending_fees()` tests.
- mixed-position deposit/withdraw tests across idle balances, Blend lending,
  and Phoenix LP positions.
- same-ledger operation/value manipulation guard tests.

Recommended continuous verification:
- run `cargo test --workspace --lib` in CI,
- keep `docs/coverage_report.md` updated after material test-suite changes,
- preserve regression tests for guard slippage and vault-init admin handoff,
- run periodic governance-drill simulations (oracle/admin compromise scenarios).
