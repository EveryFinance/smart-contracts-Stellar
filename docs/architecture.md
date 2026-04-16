# Architecture

## System Overview

The protocol is a modular, non-custodial fund management system built on Soroban.

Core design:
- `vault` is the central accounting and policy engine.
- `share_token` represents depositor ownership as SEP-41 fungible shares.
- `strategies` hold and manage external protocol positions.
- `trade_guards` enforce pre-trade policy constraints.
- `oracle` provides asset pricing for NAV conversion.
- `factory` is a registry for deployed vaults.

## Contract Topology

- Users deposit and withdraw through `vault`.
- `vault` mints/burns `share_token` shares.
- `vault` allocates capital to strategies via `invest*` and receives funds via `unwind*`.
- `vault` optionally calls `oracle` during NAV calculation.
- `vault` calls `trade_guard` contracts before trade execution.
- `factory` tracks known vaults and supports verified registration.

## Roles

- `manager`: strategy operations, parameter updates, pause/unpause, oracle/guard wiring.
- `trader`: spot trade execution through `vault.execute_trade`.
- `oracle admin`: controls oracle admin, max age, and prices.
- `factory admin`: controls registry admin actions.

Recommended production policy:
- separate all four roles.
- use multisig/timelock for `manager`, `oracle admin`, and `factory admin`.

Private-pool mode:
- when enabled on vault, only manager and allowlisted members can deposit.

## Accounting Model

### Shares

- Deposits mint shares proportional to NAV/share.
- Withdrawals burn shares and return proportional base-asset value.
- Entry fee is charged in shares (minted to manager).
- Exit fee stays in vault (benefits remaining LPs).

### NAV

Vault NAV is computed as:
- vault base-asset cash,
- plus strategy values,
- converted to base units where needed via oracle price tokens.

For LP strategies:
- valuation is handled inside strategy contracts,
- vault enforces oracle presence for LP valuation safety.

## Fee Architecture

- Entry fee cap: 500 bps.
- Exit fee cap: 500 bps.
- Management fee cap: 300 bps annualized.
- Performance fee cap: 3000 bps.

Vault accrues fees before deposit and withdrawal flows.

Fee-increase hardening:
- direct fee setters are decrease-only for safety,
- increases use `announce_fee_increase` and delayed `commit_fee_increase`.

## Risk and Control Layers

1. Auth checks (`require_auth`, role equality checks).
2. Strategy whitelist checks.
3. Trade guard checks for path/whitelist/slippage.
4. Concentration controls (`max_concentration_bps`).
5. TVL/NAV loss guard (`max_loss_bps`) around manager/trader operations.
6. Oracle freshness checks (`max_age_ledgers`) at read time.
7. Exit cooldown to reduce atomic deposit/withdraw extraction risk.
8. Same-ledger operation/value checkpoint guard (`set_value_guard_enabled`).

## Data Lifetime / TTL

Contracts use instance/persistent TTL bumping patterns to avoid data archiving during normal operation. TTL constants are defined per contract storage module.
