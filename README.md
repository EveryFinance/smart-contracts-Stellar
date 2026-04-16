# Every Finance Protocol (Rebranded to Elyx Finance)

Dapp: https://www.elyx.finance/

An on-chain, non-custodial asset management protocol built on **Stellar Soroban**. A professional fund manager accepts deposits in a configurable vault base asset, issues pro-rata **share tokens** to depositors, and deploys capital across multiple DeFi protocols — all governed by transparent, immutable on-chain rules with no privileged backdoors.

---

## Table of Contents

1. [Design Philosophy](#1-design-philosophy)
2. [Architecture](#2-architecture)
3. [Contract Inventory](#3-contract-inventory)
4. [Core Flows](#4-core-flows)
5. [NAV & Share Price Accounting](#5-nav--share-price-accounting)
6. [Fee Model](#6-fee-model)
7. [Security Model](#7-security-model)
8. [Strategy Interface](#8-strategy-interface)
9. [Trade Guard Policy](#9-trade-guard-policy)
10. [Oracle Integration](#10-oracle-integration)
11. [Storage & TTL](#11-storage--ttl)
12. [Adding a New Protocol](#12-adding-a-new-protocol)
13. [Building & Testing](#13-building--testing)
14. [Testnet Deployment](#14-testnet-deployment)
15. [CI/CD Pipeline](#15-cicd-pipeline)

---

## 1. Design Philosophy

### Non-custodial fund management

Depositors retain economic ownership at all times through share tokens. The manager can invest capital and execute trades, but can never withdraw funds to an arbitrary address — only `withdraw()` callers who hold shares can redeem base assets, and then only proportional to their share.

### On-chain policy enforcement

Every manager action passes through a layered validation stack before execution:

```
Manager action
  │
  ├─ Auth check          (caller.require_auth)
  ├─ Whitelist check     (strategy must be in vault's approved list)
  ├─ Pre-execution guard (trade guard validates swap path, slippage, tokens)
  ├─ Execute action      (cross-contract call to strategy or DEX)
  ├─ Concentration check (single strategy NAV share ≤ max_concentration_bps)
  └─ Post-execution NAV guard (nav_after ≥ nav_before × (1 − max_loss_bps/10_000))
```

If any layer fails, the entire transaction reverts.

### Aligned incentives

The manager's entry fee is charged as **share tokens**, not base asset. This means the manager is immediately subject to the same NAV risk as every other depositor — they are incentivised to grow the fund value rather than collect fees and exit.

### Composable strategies

The Vault has no knowledge of specific protocols. It interacts with strategies through a minimal interface (`deposit`, `withdraw`, `get_value`). Adding support for a new protocol requires no vault code changes — only deploying a new strategy contract.

---

## 2. Architecture

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                              Factory (Registry)                              │
│                                                                              │
│  register_vault(caller, vault, manager)                                      │
│  verify_and_register_vault(caller, vault)   ← cross-contract verified       │
│  remove_vault / get_vaults / get_vault_count / is_registered                 │
└──────────────────────────────────┬───────────────────────────────────────────┘
                                   │ tracks vault addresses
                                   ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                                  Vault                                       │
│                                                                              │
│  Depositors                                                                  │
│    deposit(amount, from)  → shares_minted                                    │
│    withdraw(shares, from, to) → base_returned                                │
│                                                                              │
│  Manager (capital deployment)                                                │
│    invest(manager, strategy, amount)                                         │
│    unwind(manager, strategy, units)                                          │
│    invest_lp(manager, strategy, amount_a, amount_b, min_a, min_b)            │
│    unwind_lp(manager, strategy, lp_amount, min_a, min_b)                     │
│                                                                              │
│  Trader (spot execution)                                                     │
│    execute_trade(trader, strategy, amount_in, min_out, path)                 │
│                                                                              │
│  Configuration (manager only)                                                │
│    set_strategies / set_trade_guard / set_deposit_cap                        │
│    set_oracle / set_strategy_price_token / set_lp_strategy                   │
│    set_max_concentration_bps / set_max_loss_bps                              │
│    set_entry_fee_bps / set_exit_fee_bps / set_mgmt_fee_bps / set_perf_fee_bps│
│    pause / unpause / set_manager / set_trader                                │
│                                                                              │
│  Views (public)                                                              │
│    get_nav / get_share_price / get_manager / get_strategies / is_paused …    │
└──┬────────────────────┬───────────────────────────────────┬──────────────────┘
   │ mint / burn        │ invest / unwind                   │ validate before trade
   ▼                    ▼                                   ▼
┌──────────────┐  ┌─────────────────────────────┐  ┌──────────────────────────┐
│  ShareToken  │  │        Strategies            │  │      Trade Guards        │
│  (SEP-41)    │  │                             │  │                          │
│              │  │  BlendStrategy              │  │  SoroswapTradeGuard      │
│  mint(to,    │  │    deposit / withdraw /     │  │    validate_swap_exact_in │
│    amount)   │  │    get_value / sync_position│  │    validate_swap_exact_out│
│  burn(from,  │  │                             │  │                          │
│    amount)   │  │  SoroswapLpStrategy         │  │  PhoenixTradeGuard       │
│  transfer /  │  │    deposit_liquidity /      │  │    validate_swap          │
│  approve /   │  │    withdraw / get_value /   │  │    validate_swap_exact_in │
│  balance …   │  │    set_oracle               │  │                          │
└──────────────┘  │                             │  │  Rules enforced:         │
                  │  PhoenixLpStrategy          │  │  • token whitelist       │
                  │    deposit_liquidity /      │  │  • slippage ≤ 10 %       │
                  │    withdraw / get_value     │  │  • path length bounds    │
                  └───────────┬─────────────────┘  └──────────────────────────┘
                              │ cross-contract calls
                              ▼
               ┌──────────────────────────────────┐
               │         External Protocols        │
               │                                  │
               │  Blend Protocol  (lending pool)  │
               │  Soroswap        (AMM / router)  │
               │  Phoenix Protocol (AMM / pool)   │
               └──────────────────────────────────┘

┌──────────────────────────┐
│          Oracle          │
│                          │
│  set_price(asset, price) │
│  get_price(asset) → i128 │
│  get_prices(assets) → Map│
│                          │
│  Used by vault nav() and │
│  SoroswapLP get_value()  │
└──────────────────────────┘
```

### Component roles at a glance

| Component | Who interacts with it | Primary responsibility |
|---|---|---|
| **Factory** | Protocol admin, off-chain tooling | Registry of vault addresses |
| **Vault** | Depositors, manager, trader | NAV accounting, fee management, capital routing |
| **ShareToken** | Vault (mint/burn), depositors (transfer) | SEP-41 fungible proof-of-deposit |
| **Oracle** | Vault `nav()`, Soroswap LP `get_value()` | PRICE_PRECISION-scaled asset prices |
| **Strategies** | Vault | Abstract interface to DeFi protocols |
| **Trade Guards** | Vault (before each trade) | Pre-execution policy enforcement |

---

## 3. Contract Inventory

| Contract | Path | Role |
|---|---|---|
| **Factory** | `contracts/factory` | Append-only registry of deployed and verified vaults |
| **Vault** | `contracts/vault` | Central NAV engine: deposit, withdraw, invest, trade |
| **ShareToken** | `contracts/share_token` | SEP-41 fungible token representing vault shares |
| **Oracle** | `contracts/oracle` | Admin-controlled on-chain price feed |
| **BlendStrategy** | `contracts/strategies/blend` | Single-asset supply to Blend Protocol lending pool |
| **SoroswapLpStrategy** | `contracts/strategies/soroswap_lp` | Two-asset LP position on Soroswap AMM |
| **PhoenixLpStrategy** | `contracts/strategies/phoenix_lp` | Two-asset LP position on Phoenix Protocol AMM |
| **SoroswapTradeGuard** | `contracts/trade_guards/soroswap` | Policy validation for Soroswap swaps |
| **PhoenixTradeGuard** | `contracts/trade_guards/phoenix` | Policy validation for Phoenix multi-hop swaps |

---

## 4. Core Flows

### 4.1 Deposit

A user transfers base asset to the vault and receives share tokens proportional to their contribution at the current NAV.

```
User
  │
  ├─ 1. base_asset.transfer(user → vault, amount)
  │
  └─ vault.deposit(amount, from=user)
         │
         ├─ 2. Collect streaming mgmt fee + perf fee (mint shares to manager)
         │
         ├─ 3. Compute share_price = NAV / total_supply
         │      (first depositor: price = 1.0, minted 1:1)
         │
         ├─ 4. total_shares = amount / share_price
         │      fee_shares  = total_shares × entry_fee_bps / 10_000
         │      user_shares = total_shares − fee_shares
         │
         ├─ 5. Check deposit cap (if set): NAV + amount ≤ cap
         │
         ├─ 6. ShareToken.mint(to=user,    amount=user_shares)
         └─ 7. ShareToken.mint(to=manager, amount=fee_shares)  ← entry fee as shares
```

**Key design decision:** The entry fee is charged as shares minted to the manager, not as a base-asset transfer out of the vault. The full deposit stays in the vault. This aligns manager incentives — they hold the same risk as depositors and benefit only if the NAV grows.

### 4.2 Withdraw

A share-holder burns shares and receives back the proportional base asset.

```
User
  │
  └─ vault.withdraw(share_amount, from=user, to=recipient)
         │
         ├─ 1. Collect streaming mgmt fee + perf fee
         │
         ├─ 2. Check share balance: user holds ≥ share_amount
         │
         ├─ 3. base_gross = share_amount × (NAV / total_supply)
         │      base_net  = base_gross × (1 − exit_fee_bps / 10_000)
         │
         ├─ 4. Auto-unwind if vault cash < base_net:
         │      For each non-LP strategy (proportionally):
         │        strategy.withdraw(shortfall_portion, vault, vault)
         │
         ├─ 5. ShareToken.burn(from=user, amount=share_amount)
         │
         └─ 6. base_asset.transfer(vault → recipient, base_net)
                (exit fee fraction stays in vault — benefits remaining shareholders)
```

**Key design decision:** The exit fee is NOT transferred to the manager. The fee fraction stays inside the vault, increasing the NAV per share for remaining depositors. This prevents a manager from extracting value on every withdrawal.

### 4.3 Invest (manager deploys capital)

```
Manager
  │
  └─ vault.invest(manager, strategy, amount)
         │
         ├─ 1. Auth: caller must be manager
         ├─ 2. Whitelist: strategy must be in vault's approved list
         ├─ 3. If trade guard set: guard.validate_invest(vault, amount)
         ├─ 4. Snapshot NAV before (if TVL guard enabled)
         ├─ 5. base_asset.approve(vault → strategy, amount)
         ├─ 6. strategy.deposit(amount, from=vault)
         │       └─ strategy pulls tokens, invests in protocol
         ├─ 7. Concentration check:
         │      strategy_value / nav_after ≤ max_concentration_bps / 10_000
         └─ 8. TVL guard:
                nav_after ≥ nav_before × (1 − max_loss_bps / 10_000)
```

### 4.4 Unwind (manager reclaims capital)

```
Manager
  │
  └─ vault.unwind(manager, strategy, units)
         │
         ├─ 1. Auth + whitelist checks
         ├─ 2. If trade guard set: guard.validate_unwind(vault, units)
         ├─ 3. Snapshot NAV before (if TVL guard enabled)
         ├─ 4. strategy.withdraw(units, from=vault, to=vault)
         └─ 5. TVL guard: nav_after ≥ nav_before × (1 − max_loss_bps / 10_000)
```

### 4.5 Spot Trade (trader executes swap)

```
Trader
  │
  └─ vault.execute_trade(trader, strategy, amount_in, min_out, path)
         │
         ├─ 1. Auth: caller must be trader
         ├─ 2. Whitelist: strategy must be in approved list
         ├─ 3. Guard must be set: get_trade_guard(strategy) must exist
         ├─ 4. guard.validate_swap_exact_in(vault, amount_in, min_out, path)
         │       ├─ whitelist: all tokens in path must be approved
         │       ├─ slippage: (amount_in − min_out)/amount_in ≤ MAX_SLIPPAGE_BPS
         │       └─ path length: 2–5 tokens (Soroswap) / 1–4 ops (Phoenix)
         ├─ 5. Snapshot NAV before (if TVL guard enabled)
         ├─ 6. strategy.execute_trade(amount_in, min_out, path, vault)
         │       └─ strategy calls DEX router
         └─ 7. TVL guard: nav_after ≥ nav_before × (1 − max_loss_bps / 10_000)
```

### 4.6 Auto-Unwind on Withdrawal

When a user withdraws more than the vault's available cash balance, the vault automatically partially unwinds single-asset strategies in proportion to their NAV share to cover the shortfall. LP strategies (Soroswap, Phoenix) are **skipped** during auto-unwind because they hold two assets and cannot accept a single-asset partial redemption cleanly.

```
Shortfall = base_net − vault_cash_balance

For each strategy S (where is_lp_strategy(S) == false):
  S_value   = strategy_get_value(S, vault)
  portion   = shortfall × S_value / total_single_asset_value
  strategy.withdraw(portion, vault, vault)
```

---

## 5. NAV & Share Price Accounting

### Net Asset Value

```
NAV = vault_base_balance
    + Σ strategy_contribution(S)   for S in strategies
```

Where `strategy_contribution(S)` is:

| Oracle configured? | Price token set for S? | Contribution formula |
|---|---|---|
| No | — | `S.get_value(vault)` (raw, in base-asset units) |
| Yes | No | `S.get_value(vault)` (assumed already base-denominated) |
| Yes | Yes | `S.get_value(vault) × oracle.get_price(price_token) / PRICE_PRECISION` |

For **Soroswap LP** positions with an oracle set, `get_value()` uses **reserve decomposition**:

```
lp_share     = lp_balance / pair.total_supply()
pool_value   = pair.get_reserves().0 × oracle.get_price(asset_a) / PRICE_PRECISION
             + pair.get_reserves().1 × oracle.get_price(asset_b) / PRICE_PRECISION
get_value()  = pool_value × lp_share
```

This correctly accounts for impermanent loss and does not require an oracle for the LP token itself — only for the two underlying assets.

### Share Price

```
share_price = NAV × PRICE_PRECISION / total_supply
```

`PRICE_PRECISION = 10_000_000` (7 decimal places, matching Stellar native asset precision).

### Bootstrap (first deposit)

When `total_supply == 0` (no prior deposits), the first depositor receives 1 share per base-asset unit and the share price initialises at `PRICE_PRECISION` (= 1.0).

### Shares minted on deposit

```
total_shares = amount × PRICE_PRECISION / share_price
fee_shares   = total_shares × entry_fee_bps / 10_000
user_shares  = total_shares − fee_shares
```

### Base returned on withdrawal

```
base_gross = share_amount × NAV / total_supply
base_net   = base_gross × (10_000 − exit_fee_bps) / 10_000
```

---

## 6. Fee Model

All fees are expressed in **basis points** (`1 bps = 0.01%`). Hard caps are enforced at `initialize` time and cannot be changed after deployment.

| Fee | Cap | Trigger | Mechanism | Recipient |
|---|---|---|---|---|
| **Entry fee** | 500 bps (5 %) | `deposit` | Shares minted to manager at deposit share price | Manager (as shares) |
| **Exit fee** | 500 bps (5 %) | `withdraw` | Fee fraction stays in vault; user receives `base_net` only | Remaining shareholders (NAV increase) |
| **Management fee** | 300 bps (3 %/yr) | Every `deposit` / `withdraw` | Continuously streamed: `NAV × mgmt_bps × elapsed_seconds / (10_000 × SECONDS_PER_YEAR)` | Manager (as minted shares) |
| **Performance fee** | 3 000 bps (30 %) | Every `deposit` / `withdraw` | Charged on gain above high-water mark per share: `gain × total_supply × perf_bps / (PRICE_PRECISION × 10_000)` | Manager (as minted shares) |

### Entry fee — shares, not base asset

The entry fee model mints share tokens to the manager instead of transferring deposited base assets out of the vault. This has two consequences:

1. The **full deposit amount enters the vault** and is immediately deployed for all shareholders — no "fee leakage" reduces the vault's investable capital.
2. The manager's fee shares are subject to the **same NAV risk** as every other shareholder. If the fund loses value, so does the manager's fee position.

### Exit fee — stays in vault

The exit fee model keeps the fee fraction in the vault rather than transferring it to the manager. This increases the NAV per share for remaining depositors and creates a **natural incentive for long-term holding** — early exitors effectively donate a small NAV fraction to continuing shareholders.

### High-water mark (performance fee)

The performance fee is only charged when the current NAV per share exceeds the **highest previously recorded NAV per share**. If the fund declines and then recovers, the manager earns no performance fee until the prior peak is surpassed. This protects depositors from paying twice for the same gain.

```
hwm          = stored high-water mark (PRICE_PRECISION-scaled)
current_ps   = NAV × PRICE_PRECISION / total_supply

if current_ps > hwm:
  gain_per_share = current_ps − hwm
  perf_fee_shares = gain_per_share × total_supply × perf_fee_bps
                    / (PRICE_PRECISION × FEE_DENOMINATOR)
  mint perf_fee_shares to manager
  set hwm = current_ps
```

---

## 7. Security Model

### 7.1 Authorization

Every state-mutating function begins with `caller.require_auth()` enforced by the Soroban host. The role hierarchy is:

| Role | Capabilities |
|---|---|
| **Depositor** | `deposit`, `withdraw` (own shares) |
| **Share-holder** | `withdraw` (own shares only) |
| **Trader** | `execute_trade` |
| **Manager** | Everything trader can do + invest / unwind / configuration / pause |

The manager cannot withdraw other users' shares and cannot redirect vault funds to arbitrary addresses.

### 7.2 TVL Guard (post-execution NAV check)

After every manager operation the vault optionally verifies that NAV did not drop by more than `max_loss_bps`:

```
nav_after ≥ nav_before × (1 − max_loss_bps / 10_000)
```

If this check fails the entire transaction reverts with `TvlGuardTripped (#17)`. This prevents a compromised manager or a malicious strategy from silently draining vault value through slippage abuse or oracle manipulation.

Configure via:
```
vault.set_max_loss_bps(manager, 100)   // 1% max loss per operation
```

### 7.3 Concentration Limit

After any `invest` call the vault checks that no single strategy holds more than `max_concentration_bps` of total NAV:

```
strategy_value / nav_after ≤ max_concentration_bps / 10_000
```

Violations revert with `ConcentrationLimitExceeded (#16)`. Set `max_concentration_bps = 0` to disable.

### 7.4 Trade Guards (pre-execution policy)

Both trade guards enforce on-chain policy before any swap executes:

| Rule | Soroswap | Phoenix |
|---|---|---|
| Token whitelist | all path tokens | all `offer_asset` + `ask_asset` |
| Slippage cap | ≤ 10 % (1 000 bps) | ≤ 10 % (1 000 bps) |
| Path / op bounds | 2–5 tokens | 1–4 `SwapOperation`s |
| Caller | vault only | vault only |

The whitelist is maintained by the manager and cannot be changed mid-transaction.

### 7.5 Deposit Cap

The manager can set a maximum total NAV the vault will accept:

```
vault.set_deposit_cap(manager, 1_000_000_0000000)  // example deposit cap
```

Deposits that would push NAV above the cap revert with `DepositCapExceeded (#15)`. Set to `0` to disable.

### 7.6 Emergency Pause

```
vault.pause(manager)    // blocks deposit + withdraw
vault.unpause(manager)  // restores normal operation
```

Pausing does **not** prevent the manager from unwinding positions — capital recovery remains possible even during a pause.

### 7.7 Fee Caps

Hard-coded on initialization; cannot be raised after deployment:

```
entry / exit fee  ≤ 500 bps  (5 %)
management fee    ≤ 300 bps  (3 % per year)
performance fee   ≤ 3 000 bps (30 %)
```

### 7.8 Soroban Execution Model

- **Reentrancy**: Soroban's single-threaded execution prevents classic reentrancy attacks.
- **Auth propagation**: All cross-contract calls use `env.invoke_contract`; auth is validated by the host at each level.
- **Overflow**: All arithmetic uses `i128` with `checked_add` / `checked_sub`; overflows revert with `Overflow (#10)`.
- **Storage TTL**: All instance-storage entries are bumped on every call; archival during active use is not possible.

---

## 8. Strategy Interface

All strategies expose this minimal interface to the vault:

### Single-asset strategies (e.g. Blend)

| Function | Caller | Description |
|---|---|---|
| `initialize(vault, asset, protocol, manager, name)` | Deployer | One-time setup |
| `deposit(amount, from) → units` | Vault | Pull `amount` from `from`, invest in protocol, return position |
| `withdraw(units, from, to) → amount` | Vault | Redeem `units`, send asset directly to `to` |
| `get_value(vault) → i128` | Vault | Return current position value in base-asset units |
| `pause(caller)` / `unpause(caller)` | Manager | Emergency halt |
| `is_paused() → bool` | Anyone | Read pause state |
| `get_name() → String` | Anyone | Strategy label |

### Two-asset LP strategies (e.g. Soroswap, Phoenix)

| Function | Caller | Description |
|---|---|---|
| `initialize(vault, asset_a, asset_b, …, manager, name)` | Deployer | One-time setup |
| `deposit_liquidity(amount_a, amount_b, min_a, min_b, from) → lp_units` | Vault | Provide liquidity to pool |
| `withdraw(lp_units, min_a, min_b, from, to) → (amount_a, amount_b)` | Vault | Remove liquidity, send tokens directly to `to` |
| `get_value(vault) → i128` | Vault | Return LP position value (raw or oracle-priced) |
| `set_oracle(caller, oracle)` | Manager | Configure reserve-decomposition oracle |

### Blend strategy specifics

Blend is a **supply-only** strategy. Request types 4 (Borrow) and 5 (Repay) are explicitly not implemented — the strategy cannot take on debt. The strategy contract is the account holder inside Blend, not the vault, so all Blend operations use the strategy's own authorization.

### Soroswap LP NAV (oracle mode)

When an oracle is set, `get_value()` uses reserve decomposition rather than relying on an LP token price oracle:

```
lp_share    = get_lp_balance() / pair_contract.total_supply()
reserve_a   = pair_contract.get_reserves().0
reserve_b   = pair_contract.get_reserves().1
pool_value  = reserve_a × oracle.get_price(asset_a) / PRICE_PRECISION
            + reserve_b × oracle.get_price(asset_b) / PRICE_PRECISION
get_value() = pool_value × lp_share
```

In Soroswap the pair contract and the LP token contract share the same address, so `get_reserves()` and `total_supply()` are called on the same contract.

---

## 9. Trade Guard Policy

Trade guards are separate contracts deployed alongside the vault. The vault calls the guard's `validate_*` function **before** forwarding any trade to the DEX. If validation fails the guard reverts and the entire vault transaction rolls back.

### Policy rules

| Rule | Soroswap | Phoenix |
|---|---|---|
| Vault-only access | `caller == vault` | `caller == vault` |
| Positive amount | `amount_in > 0` | `amount_in > 0` |
| Path / ops bounds | path length: 2–5 | ops: 1–4 |
| Token whitelist | every path token | every `offer_asset` and `ask_asset` |
| Slippage cap | `(in − min_out)/in ≤ 10%` | `(in − min_out)/in ≤ 10%` |

### Slippage check (integer arithmetic)

To avoid floating-point, the slippage condition is rewritten as:

```
(amount_in − min_out) × 10_000 ≤ amount_in × MAX_SLIPPAGE_BPS
```

### Guard configuration

```
// Deploy and initialize a guard
soroswap_guard.initialize(vault_address, manager_address, initial_whitelist)

// Register it with the vault
vault.set_trade_guard(manager, strategy_address, guard_address)

// Update whitelist
soroswap_guard.set_whitelist(manager, [usdc, xlm, btc])
```

---

## 10. Oracle Integration

The Oracle contract exposes a simple `get_price(asset) → i128` interface, where all prices are `PRICE_PRECISION`-scaled (7 decimal places):

```
price = 10_000_000  →  1 unit of asset = 1.0 base currency
price = 5_000_000   →  1 unit = 0.5 base currency
price = 650_000_000_000  →  1 unit = 65 000 base currency (example)
```

### Wiring oracle pricing to a strategy

```
// 1. Deploy oracle and set prices
oracle.initialize(admin)
oracle.set_price(admin, lp_token_address, price_in_usdc)

// 2. Tell the vault to use the oracle
vault.set_oracle(manager, oracle_address)

// 3. Tell the vault which token to use when pricing this strategy
vault.set_strategy_price_token(manager, strategy_address, lp_token_address)
```

When both `oracle` and `strategy_price_token` are set for a strategy, `nav()` computes:

```
contribution = strategy.get_value(vault) × oracle.get_price(price_token) / PRICE_PRECISION
```

### Production upgrade path

Replace the admin-controlled `set_price` with calls to the [Reflector oracle network](https://reflector.network/) or any other on-chain aggregator while keeping the same `get_price(asset) → i128` interface. No vault or strategy code changes are required.

---

## 11. Storage & TTL

Soroban entries expire if not refreshed. The protocol uses two storage tiers:

| Tier | Used for | Bump amount | Threshold |
|---|---|---|---|
| **Instance** | All contract configuration | 34 560 ledgers (≈ 2.4 days) | 17 280 ledgers (≈ 1.2 days) |
| **Persistent** | Share token balances & allowances, oracle prices | 518 400 ledgers (≈ 360 days) | 259 200 ledgers (≈ 180 days) |

Every entry-point call bumps the relevant TTL before reading or writing state, ensuring that data is never archived during normal operation.

---

## 12. Adding a New Protocol

### New single-asset strategy

1. Create `contracts/strategies/<name>/src/lib.rs` implementing:
   - `initialize(vault, asset, protocol, manager, name)`
   - `deposit(amount, from) → i128`
   - `withdraw(amount, from, to) → i128`
   - `get_value(vault) → i128`
   - `pause(caller)` / `unpause(caller)` / `is_paused() → bool`

2. Add the crate to `Cargo.toml` workspace members.

3. Deploy, initialize, then:
   ```
   vault.set_strategies(manager, [...existing..., new_strategy])
   ```

No vault code changes required.

### New LP strategy

Follow the same steps but implement:
- `deposit_liquidity(amount_a, amount_b, min_a, min_b, from) → i128`
- `withdraw(lp_amount, min_a, min_b, from, to) → (i128, i128)`
- `get_value(vault) → i128`

Then mark it as an LP strategy so auto-unwind skips it:
```
vault.set_lp_strategy(manager, new_strategy, true)
```

### New trade guard

1. Create `contracts/trade_guards/<name>/` implementing:
   - `initialize(vault, manager, tokens)`
   - `set_whitelist(caller, tokens)`
   - `validate_swap_exact_in(caller, amount_in, min_out, path)`

2. Deploy, initialize, then:
   ```
   vault.set_trade_guard(manager, strategy_address, guard_address)
   ```

---

## 13. Building & Testing

## 14. Testnet Deployment

### Prerequisites

1. Install Stellar CLI (`stellar`) and ensure version `>= 26`.
2. Create and fund a testnet identity (example alias: `deployer`):

```bash
stellar keys generate deployer --network testnet --fund --overwrite
```

### Deploy core protocol (vault/share/oracle/factory + guards)

```bash
cd scripts
cp deploy.env.example .env
set -a; source .env; set +a
./deploy_testnet.sh
```

The script writes deployment outputs to:

- `deployments/testnet-<timestamp>.env`

including:

- `SHARE_TOKEN_ID`
- `VAULT_ID`
- `ORACLE_ID`
- `FACTORY_ID`
- `SOROSWAP_GUARD_ID`
- `PHOENIX_GUARD_ID`

Latest redeployment (April 16, 2026, 15:05:21 testnet):

- `SHARE_TOKEN_ID=CBD2NZHYBNTN4ECUWVUT7KBTCQOXAK2D6TFDMHZPJIWL2MP56KPQWAN7`
- `VAULT_ID=CBVK3IC7ALAA5PRWKIRELDHOT6ZXSM6DZHSXDT5Y3Z5WHHVU46EIDJHE`
- `ORACLE_ID=CCFACGTFLN3CV4LRLRTPK73VUAS2VXDOPMJAZVE4HNVNVNAMREF3QMD2`
- `FACTORY_ID=CCER4YYGW2GEYAYHC7E2ULUQPV5OLTYXV3GUTBQ5IG62CV5YNWHDSJKV`
- `SOROSWAP_GUARD_ID=CAC5D67PTM7E7W7GZO7J4ENNBTMA443MQZTWHD3YTFRBZWVVMEFVPPPB`
- `PHOENIX_GUARD_ID=CBH4GEYHKXU54D3434M7OKJODM3LNT7ED5P6KYFYS2UG4Q46UJPCGXG3`

Explorer:

- https://stellar.expert/explorer/testnet/contract/CBD2NZHYBNTN4ECUWVUT7KBTCQOXAK2D6TFDMHZPJIWL2MP56KPQWAN7
- https://stellar.expert/explorer/testnet/contract/CBVK3IC7ALAA5PRWKIRELDHOT6ZXSM6DZHSXDT5Y3Z5WHHVU46EIDJHE
- https://stellar.expert/explorer/testnet/contract/CCFACGTFLN3CV4LRLRTPK73VUAS2VXDOPMJAZVE4HNVNVNAMREF3QMD2
- https://stellar.expert/explorer/testnet/contract/CCER4YYGW2GEYAYHC7E2ULUQPV5OLTYXV3GUTBQ5IG62CV5YNWHDSJKV
- https://stellar.expert/explorer/testnet/contract/CAC5D67PTM7E7W7GZO7J4ENNBTMA443MQZTWHD3YTFRBZWVVMEFVPPPB
- https://stellar.expert/explorer/testnet/contract/CBH4GEYHKXU54D3434M7OKJODM3LNT7ED5P6KYFYS2UG4Q46UJPCGXG3

### Optional strategy deployments

Set these flags and required addresses in `.env` before running:

- `DEPLOY_BLEND_STRATEGY=true` with `BLEND_PROTOCOL_ID`
- `DEPLOY_SOROSWAP_LP_STRATEGY=true` with `SOROSWAP_ASSET_A`, `SOROSWAP_ASSET_B`, `SOROSWAP_LP_TOKEN`, `SOROSWAP_ROUTER`
- `DEPLOY_PHOENIX_LP_STRATEGY=true` with `PHOENIX_ASSET_A`, `PHOENIX_ASSET_B`, `PHOENIX_POOL`

### Prerequisites

```bash
# Install Rust with the wasm32 target
rustup target add wasm32-unknown-unknown

# Install Soroban CLI (recommended: prebuilt Stellar CLI)
# The `stellar` binary also provides the `soroban` command.
curl -L -o stellar-cli.tar.gz \
  https://github.com/stellar/stellar-cli/releases/download/v26.0.0/stellar-cli-26.0.0-x86_64-unknown-linux-gnu.tar.gz
tar -xzf stellar-cli.tar.gz
install -m 0755 stellar /usr/local/bin/stellar
ln -sf /usr/local/bin/stellar /usr/local/bin/soroban

# Or build from source (slower, higher system deps)
cargo install --locked soroban-cli
```

### Build

```bash
cd smart-contracts-Stellar

# Build all contracts as WASM
cargo build --workspace --target wasm32-unknown-unknown --release

# Build for native testing (no WASM target needed)
cargo build --workspace
```

### Test

```bash
# Run the full test suite (348 tests across 10 crates)
cargo test --workspace

# Run tests for a single contract
cargo test -p vault
cargo test -p soroswap-lp-strategy
cargo test -p factory

# Run a specific test by name
cargo test -p vault test_tvl_guard_trips_when_loss_exceeds_tolerance
```

### Test coverage by crate

| Crate | Tests | What is covered |
|---|---|---|
| `vault` | 76 | Deposit/withdraw, fees, invest/unwind, oracle NAV, concentration limits, TVL guard, auto-unwind, guards, pause |
| `integration_tests` | 58 | End-to-end Vault ↔ ShareToken lifecycle with real contract implementations |
| `soroswap_trade_guard` | 25 | Whitelist, slippage, path validation, auth |
| `factory` | 30 | Registry CRUD, verify-and-register, pagination, admin transfer |
| `phoenix_lp` | 29 | Deposit/withdraw LP, share tracking, pause |
| `soroswap_lp` | 29 | Deposit/withdraw LP, LP balance tracking, reserve decomposition NAV |
| `phoenix_trade_guard` | 24 | Whitelist, slippage, multi-hop ops validation, auth |
| `share_token` | 34 | SEP-41: mint, burn, transfer, approve, transfer_from, burn_from |
| `blend` | 24 | Supply/withdraw, position tracking, sync_position, pause |
| `oracle` | 19 | Set/get price, batch get, admin management |

---

## 15. CI/CD Pipeline

This repository uses GitHub Actions workflows under `.github/workflows`:

- `ci.yml`
  - Triggers: pull requests and pushes to `main`
  - Runs: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace`
- `cd.yml`
  - Triggers: pushes to `main`, version tags matching `v*`, and manual `workflow_dispatch`
  - Packages and uploads a source bundle artifact
  - On version tags, publishes a GitHub Release with the source tarball attached

---

## License

MIT
