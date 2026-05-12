# Final Smart Contract Audit Report

Date: 2026-05-12

Branch: `audit_fixes_Almanax`

Scope:

- `contracts/vault`
- `contracts/share_token`
- `contracts/factory`
- `contracts/asset_handler`
- `contracts/oracle`
- `contracts/oracle_adapters/dia`
- `contracts/oracle_adapters/reflector`
- `contracts/strategies/blend`
- `contracts/strategies/soroswap_lp`
- `contracts/strategies/phoenix_lp`
- Integration tests and protocol documentation relevant to the above contracts

## Executive Summary

The final audit pass did not identify any open Critical, High, or Medium smart-contract vulnerability.

The codebase now has stronger controls around vault accounting, role-gated operations, strategy execution, oracle pricing, fee accrual, share transferability, cooldown, and PnL reporting. The remaining risks are deployment and governance assumptions: oracle trust, privileged role management, external protocol behavior, and the explicitly accepted accounting downgrade if vault share transfers are enabled.

## Findings

| Severity | Finding | Status |
|---|---|---|
| Critical | None open | Closed / no open issue |
| High | None open | Closed / no open issue |
| Medium | None open | Closed / no open issue |
| Low / Informational | Transferable shares weaken account-level cooldown and PnL guarantees | Accepted by design and documented |
| Low / Informational | Oracle correctness remains trust-critical for NAV and share pricing | Operational control required |
| Low / Informational | Admin, manager, and trader roles are powerful | Operational control required |
| Low / Informational | External strategy protocols remain dependency risks | Operational control required |

## Final Fix Applied During This Audit

### Direct Share Burns In Transferable Mode

Issue:

When share transfers were enabled, holders could directly call share-token burn paths outside `vault.withdraw`. That was unnecessary for transferability and could desynchronize redemption, fees, cooldown, and PnL assumptions.

Resolution:

- `burn(from, amount)` is always vault/admin-only.
- `burn_from(spender, from, amount)` is disabled for vault shares.
- Share transfers, if enabled, only allow secondary transfers. Redemption remains routed through `vault.withdraw`.
- Share-token docs, vault docs, security docs, and tests were updated.

Relevant files:

- `contracts/share_token/src/lib.rs`
- `contracts/share_token/src/test.rs`
- `docs/contracts/share_token.md`
- `docs/contracts/vault.md`
- `docs/security.md`

## Key Controls Verified

### Vault Accounting

- Deposits collect pending fees before minting new user shares.
- Deposits snapshot NAV before transfer and use the same pricing path as NAV.
- Entry fees are minted as fee shares.
- Deposit caps are checked in base-asset value terms.
- User cost basis and last deposit timestamp are updated for PnL and cooldown tracking.

### Withdrawals

- Cooldown is checked before withdrawal.
- Share balance is checked before fee collection changes supply.
- Pending management and performance fees are collected before withdrawal accounting.
- Exit fee reduces the withdrawal numerator and remains in the vault.
- PnL realization is updated before burning.
- Shares are burned before asset transfers.
- Withdrawals distribute idle portfolio assets and call active guards proportionally.

### Fee Accounting

- `collect_pending_fees()` is permissionless and deterministic.
- The caller cannot choose the fee amount, recipient, NAV, or timestamp.
- Management fee accrues over elapsed time and advances the timestamp even when the rounded fee is zero.
- Performance fee uses the high-water mark and updates it when NAV per share exceeds the previous mark.
- Fee increases are capped and subject to the configured announcement/commit delay.

### Strategy Execution

- `execute_op` requires manager or trader authorization.
- Operations can be paused independently from deposits/withdrawals.
- Guard contracts must be active and, when a factory is configured, factory-authorized.
- Function names must be explicitly authorized per guard.
- Reserved lifecycle, view, withdrawal, and direct deposit functions are blocked from `execute_op`.
- The vault injects its own address as the first strategy argument, preventing caller-supplied source spoofing.
- Vault-scoped token transfer authorization is generated only for supported strategy actions.
- A NAV-loss guard checks post-operation value against `max_loss_bps`.

### Share Transferability, Cooldown, And PnL

Default mode:

- Shares are non-transferable.
- Cooldown is a hard account-level control.
- PnL tracking is accurate for the depositing/withdrawing account.
- Redemption is routed through `vault.withdraw`.

Optional transferable mode:

- The vault admin may enable share transfers.
- Cooldown becomes same-address friction, not a hard protocol control.
- PnL becomes approximate / informational.
- Direct share burns remain blocked outside the vault.

### Oracle And Asset Handling

- AssetHandler resolves prices through per-asset oracle, primary oracle, then fallback oracle.
- Unregistered assets are rejected.
- Zero or unavailable prices do not silently pass into vault pricing.
- Vault pricing rejects non-positive oracle prices.
- Asset and guard authorization is checked against the factory when configured.

### Strategy-Specific Review Notes

Blend strategy:

- Position ownership is held by the strategy contract.
- Supply and withdrawal are vault-gated.
- Withdrawals check live Blend supply.
- `get_total_value` uses the live pool position and AssetHandler pricing when configured.

Soroswap and Phoenix LP strategies:

- LP operations are vault-gated.
- Add/remove liquidity and swaps validate amounts and expected assets.
- LP valuation uses reserve decomposition and oracle pricing.
- Withdrawal fractions route underlying tokens to the withdrawing recipient.
- Guards expose NAV, withdrawal, and asset-in-use views for vault accounting.

## Residual Risks And Recommendations

### Oracle Trust

Bad or manipulated oracle prices can create incorrect NAV, share pricing, deposit shares, withdrawal value, and fee accounting.

Recommendation:

- Use only reviewed oracle adapters.
- Enforce freshness windows.
- Monitor price deviations.
- Keep emergency pause procedures ready.

### Privileged Roles

Admin, manager, and trader roles can materially affect vault operation.

Recommendation:

- Use multisig for admin and manager.
- Keep trader as a separate limited operational key.
- Monitor role changes, fee changes, guard changes, oracle changes, and transferability changes.
- Use off-chain procedures around announced fee increases.

### External Protocol Dependencies

DEX and lending integrations inherit external protocol risk.

Recommendation:

- Authorize only reviewed guards, pools, routers, and assets.
- Keep `max_loss_bps` conservative.
- Pause operations if an integrated protocol has an incident.

### Transferable Share Mode

Transferability changes the guarantees of account-based accounting.

Recommendation:

- Keep transfers disabled for vaults that need hard cooldown and accurate user-level PnL.
- Enable transfers only when the vault intentionally accepts approximate/informational PnL and same-address cooldown friction.

## Verification

Focused post-fix tests:

```bash
cargo test -p share-token -p vault
```

Result:

- `share-token`: 48 passed, 0 failed
- `vault`: 163 passed, 0 failed

Formatting and diff checks:

```bash
cargo fmt
git diff --check
```

Result:

- Clean

Production coverage command:

```bash
cargo llvm-cov --workspace --lib --summary-only --ignore-filename-regex '(/test\.rs$|contracts/integration_tests/|contracts/mock_blend_pool/)'
```

Coverage recorded in `docs/coverage_report.md`:

- Line coverage: 97.51%
- Region coverage: 96.56%
- Function coverage: 87.69%

## Final Opinion

The audited contracts are in a stronger and more coherent security posture after the fixes and test expansion. No open Critical, High, or Medium smart-contract issue remains from this final pass.

The protocol should treat oracle governance, privileged key management, guard authorization, and the optional transferable-share mode as production operating controls.
