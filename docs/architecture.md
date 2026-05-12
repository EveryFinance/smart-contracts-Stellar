# Architecture

## System Overview

The protocol is a modular, non-custodial fund management system built on Soroban.

Core design:
- `vault` is the central accounting and policy engine.
- `share_token` represents depositor ownership as SEP-41-style shares. Shares
  are non-transferable by default to preserve account-based cooldown and PnL
  correctness.
- `strategies` hold and manage external protocol positions and implement the guard interface.
- `asset_handler` is the asset registry and three-tier price oracle.
- `oracle_adapters/reflector` and `oracle_adapters/dia` adapt on-chain oracle networks.
- `oracle` is a dev/test mock — not for production.
- `factory` is a registry for deployed vaults.

## Contract Topology

```
Users ──────────────────────────────→ vault
                                       │
                              ┌────────┴────────┐
                              │                 │
                         share_token      strategies (guards)
                         (mint/burn)      ├─ soroswap_lp
                                          ├─ phoenix_lp
                                          └─ blend
                                               │
                                               ↓
                                         external protocols
                                         (Soroswap, Phoenix, Blend)

vault ──── asset_handler ──── ReflectorAdapter → Reflector network
                         └─── DIAAdapter        → DIA network
```

- Users deposit and withdraw through `vault`.
- `vault` mints/burns `share_token` shares.
- `vault` dispatches manager/trader operations to strategy contracts via `execute_op`.
- Strategies implement both the guard interface (NAV, withdrawal) and trader-callable operations.
- `asset_handler` prices assets using a three-tier oracle: per-asset override → Reflector → DIA.
- `factory` tracks known vaults.

## Roles

### Vault Admin

Controls emergency and access settings. Set at construction time. Cannot be changed without a two-step transfer.

| Action | Description |
|---|---|
| `pause_deposits` / `unpause_deposits` | Block/unblock user deposits and withdrawals |
| `pause_operations` / `unpause_operations` | Block/unblock manager execute_op calls |
| `set_share_transfers_enabled` | Explicitly opt shares into/out of transferability |
| `set_private_pool` | Toggle member-only deposit mode |
| `add_member` / `remove_member` | Manage allowlist for private-pool deposits |
| `set_pending_admin` / `accept_admin` | Two-step admin transfer |

### Vault Manager

Controls vault-level configuration. Assigned by admin.

| Action | Constraint |
|---|---|
| `add_portfolio_asset(asset)` | `asset ∈ factory.AuthorizedAssets` |
| `add_deposit_asset(asset)` | `asset ∈ PortfolioAssets` |
| `add_active_guard(guard)` | `guard ∈ factory.AuthorizedGuards` |
| `set_authorized_ops(guard, ops)` | Restrict trader per guard |
| `set_oracle(oracle)` | Must price all portfolio assets |
| Fee configuration | Within protocol caps; increases via announce→commit timelock |

### Vault Trader

Executes trades through guard contracts via `vault.execute_op(caller, guard, fn_name, args)`.

- `fn_name` must be in `AuthorizedOps(guard)` — manager whitelists which functions are permitted per guard.
- The vault **injects its own address** as the first argument — the trader cannot substitute a different source.
- Operations are blocked when `OpsPaused` is set by admin.
- Strategy lifecycle/view functions are reserved and cannot be dispatched through `execute_op`.

**DEX strategy functions:** `swap`, `add_liquidity`, `remove_liquidity`  
**Lending strategy functions:** `supply`, `withdraw_from_lending`

Strategy guards do not store a manager or expose local pause/unpause controls. Pausing is centralized in the vault through `pause_deposits`/`unpause_deposits` and `pause_operations`/`unpause_operations`.

## Strategy = Guard

Each strategy contract serves a dual role:
1. **Guard interface** — called by vault for NAV computation and proportional withdrawal:
   - `get_total_value(vault) -> i128`
   - `withdraw_fraction(vault, numerator, denominator, to)`
   - `asset_in_use(vault, asset) -> bool`
2. **Trader-callable operations** — called via `vault.execute_op` after authorization checks.

There are no separate "trade guard" contracts. All validation lives inside the strategy's operation functions.

## Asset Governance — Three-Tier Model

```
factory.AuthorizedAssets  ⊇  vault.PortfolioAssets  ⊇  vault.DepositAssets
```

- `DepositAssets ⊆ PortfolioAssets` — enforced on `add_deposit_asset`
- `PortfolioAssets ⊆ factory.AuthorizedAssets` — enforced on `add_portfolio_asset`
- Cannot remove from `PortfolioAssets` if a guard has `asset_in_use(vault, asset) == true`
- Cannot remove from `PortfolioAssets` if `token_balance(vault, asset) > 0`

## Oracle Architecture

`AssetHandler` resolves prices with a three-tier cascade:

```
Tier 1: Per-asset oracle override  → try_invoke_contract (graceful fallback)
Tier 2: Primary oracle (Reflector) → try_invoke_contract (graceful fallback)
Tier 3: Fallback oracle (DIA)      → invoke_contract (hard fail)
```

- **ReflectorAdapter** (`contracts/oracle_adapters/reflector`) — wraps Reflector's `lastprice()`, normalizes 8-decimal prices to PRICE_PRECISION (7).
- **DIAAdapter** (`contracts/oracle_adapters/dia`) — wraps DIA's `read_oracle_value()`, maps `Address → "PAIR/USD"` key, normalizes 8-decimal prices.
- **Oracle** (`contracts/oracle`) — dev/test mock only; do not deploy in production.

## NAV Formula

```
NAV = Σ oracle.get_price(asset) × token_balance(vault, asset)   for asset ∈ TrackedAssets
    + Σ guard.get_total_value(vault)                             for guard ∈ PositionGuards
```

Share price: `share_price = NAV × PRICE_PRECISION / total_supply`

`PortfolioAssets` and `ActiveGuards` are configuration lists. `TrackedAssets`
and `PositionGuards` are bounded accounting indexes that contain only live
assets/positions. This keeps deposits, withdrawals, fee collection, and NAV
views within Soroban budget when a vault supports many assets and strategies.
The vault updates the indexes during normal deposits, withdrawals, seed
deposits, and manager operations. Permissionless `sync_asset_balance(asset)` and
`sync_guard_position(guard)` entrypoints let keepers reflect external balance or
yield changes without scanning every configured asset or guard.

## Accounting Model

### Shares

- Deposits mint shares proportional to NAV/share.
- Withdrawals burn shares and return proportional value from all assets and positions.
- Entry fee is charged in shares (minted to treasury).
- Exit fee fraction remains in vault (benefits remaining LPs).

Default share-transfer policy:
- Shares are non-transferable by default.
- In default mode, exit cooldown is a hard account-level control and per-user
  PnL tracking is accurate.
- If vault admin enables share transfers, the vault explicitly reports
  `exit_cooldown_is_hard_control() == false` and
  `pnl_tracking_is_accurate() == false`.
- Transferable-share mode is therefore an opt-in operational mode where cooldown
  is same-address friction and PnL is approximate / informational only.

### Inflation Attack Prevention

Factory `create_vault()` atomically seeds the vault, ensuring `total_supply > 0` from day one and eliminating the first-depositor share-price attack.

## Fee Architecture

| Fee | Cap | Accrual |
|-----|-----|---------|
| Entry fee | 500 bps | On deposit — shares minted to treasury |
| Exit fee | 500 bps | On withdrawal — fraction stays in vault |
| Management fee | 300 bps annualized | Streamed continuously; settled as shares to treasury on deposit, withdrawal, or permissionless fee collection |
| Performance fee | 3 000 bps | On NAV-per-share exceeding high-water mark; settled on deposit, withdrawal, or permissionless fee collection |

Fee-increase hardening:
- Direct setters are decrease-only.
- Increases use `announce_fee_increase` → 86 400 s delay → `commit_fee_increase`.

Fee settlement is lazy by default and also keeper-compatible. Anyone can call
`collect_pending_fees()` to settle deterministic accrued management and
performance fee shares to treasury without waiting for a user action.

## Risk and Control Layers

1. Auth checks (`require_auth` + role identity checks).
2. Strategy whitelist (`guard ∈ ActiveGuards`).
3. Operation whitelist (`fn_name ∈ AuthorizedOps(guard)`).
4. TVL/NAV loss guard (`max_loss_bps`) around every `execute_op`.
5. Deposit cap (`deposit_cap`) on total NAV.
6. Non-transferable shares by default, preserving account-based cooldown and PnL guarantees.
7. Exit cooldown to reduce atomic deposit/withdraw extraction risk.
8. Same-ledger operation/value checkpoint guard (`set_value_guard_enabled`).
9. Admin-controlled pause splits: `pause_deposits` (user flows) / `pause_operations` (manager ops).

## Data Lifetime / TTL

Contracts use instance/persistent TTL bumping on every entry-point call to avoid data archiving during normal operation. Critical keys (Admin, Initialized) use `u32::MAX` TTL. TTL constants are defined per contract storage module.
