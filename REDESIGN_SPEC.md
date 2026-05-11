# Protocol Redesign Specification — Multi-Asset Vault v2

**Date:** 2026-05-11  
**Branch:** audit_fixes_Almanax  
**Status:** Approved for implementation

---

## 1. Motivation

The current vault design has three critical limitations:

1. **Single-asset NAV**: Only the idle `base_asset` balance is counted in NAV; USDT, XLM, or any other token held directly by the vault is invisible to share-price accounting.  
2. **Single-asset withdrawal**: Users always receive `base_asset` only, ignoring the true multi-asset composition of the portfolio.  
3. **Inflation attack**: On first deposit `total_supply == 0`, which allows a share-price manipulation attack.

This specification defines the redesigned protocol that solves all three problems using a model inspired by **dHedge V2** on EVM chains.

---

## 2. Role Model

### 2.1 Factory Admin

Controls the global protocol-level whitelists. Only the admin can:

| Action | Description |
|---|---|
| `add_authorized_asset(asset)` | Add an asset to the global whitelist |
| `remove_authorized_asset(asset)` | Remove an asset (fails if any vault holds it) |
| `add_authorized_guard(guard)` | Whitelist a strategy guard contract |
| `remove_authorized_guard(guard)` | Remove a guard (fails if any vault uses it) |
| `create_vault(params, manager, seed_amount)` | Deploy + initialize a vault with a seed deposit |
| `set_vault_manager(vault, new_manager)` | Reassign the manager of a vault |
| `set_pending_admin(new_admin)` | Begin two-step admin transfer |
| `accept_admin()` | Complete two-step admin transfer |

### 2.2 Vault Admin

Controls emergency and access settings for the vault. Admin is set at construction time. Admin can:

| Action | Description |
|---|---|
| `pause_deposits()` / `unpause_deposits()` | Block/unblock user deposits and withdrawals |
| `pause_operations()` / `unpause_operations()` | Block/unblock manager execute_op calls |
| `set_private_pool(is_private)` | Toggle member-only deposit mode |
| `add_member(addr)` / `remove_member(addr)` | Manage allowlist for private-pool deposits |
| `set_manager(new_manager)` | Transfer manager role |
| `set_treasury(addr)` | Change fee recipient |

### 2.3 Vault Manager

Controls vault-level configuration. Manager is assigned by the admin. Manager can:

| Action | Constraint |
|---|---|
| `set_fees(deposit_bps, withdraw_bps, mgmt_bps, perf_bps)` | Within protocol caps |
| `add_portfolio_asset(asset)` | `asset ∈ factory.AuthorizedAssets` |
| `remove_portfolio_asset(asset)` | balance==0 AND no active guard uses it |
| `add_deposit_asset(asset)` | `asset ∈ PortfolioAssets` |
| `remove_deposit_asset(asset)` | No constraint (does not affect holdings) |
| `add_guard(guard)` | `guard ∈ factory.AuthorizedGuards` |
| `remove_guard(guard)` | `guard.get_total_value(vault) == 0` |
| `set_authorized_ops(guard, ops)` | Further restrict trader per guard |
| `set_vault_oracle(oracle)` | Must price all portfolio assets |

### 2.4 Vault Trader

Executes trades through guard contracts via `vault.execute_op(caller, guard, fn_name, args)`.

- `fn_name` must appear in `AuthorizedOps(guard)` — manager controls which functions are permitted per guard
- The vault **injects its own address** as the first argument before calling the guard; the trader cannot substitute a different source address — funds can only move from the vault
- Args are typed `Vec<Val>` (Soroban native encoding) — analogous to ABI-encoded calldata in dHedge V2

**DEX guard functions:** `swap`, `add_liquidity`, `remove_liquidity`  
**Lending guard functions:** `supply`, `withdraw_from_lending`

---

## 3. Asset Governance — Three-Tier Model

```
factory.AuthorizedAssets   ⊇   vault.PortfolioAssets   ⊇   vault.DepositAssets
```

**Invariants enforced by contract:**
- `DepositAssets ⊆ PortfolioAssets` — enforced on `add_deposit_asset`
- `PortfolioAssets ⊆ factory.AuthorizedAssets` — enforced on `add_portfolio_asset`
- Cannot remove from `PortfolioAssets` if any guard has `asset_in_use(vault, asset) == true`
- Cannot remove from `PortfolioAssets` if `token_balance(vault, asset) > 0`
- Removing from `PortfolioAssets` automatically removes from `DepositAssets`

---

## 4. NAV Formula (Updated)

```
NAV = Σ oracle.get_price(asset) × token_balance(vault, asset)
                                    for asset ∈ PortfolioAssets
    + Σ guard.get_total_value(vault)
                                    for guard ∈ ActiveGuards
```

- `oracle.get_price(asset)` returns price in `base_asset` terms (e.g., USDC)
- `base_asset` is itself a PortfolioAsset; its price is `PRICE_PRECISION` (1.0)
- `guard.get_total_value(vault)` sums ALL positions of the vault in that protocol

**Share price:**
```
share_price = NAV × PRICE_PRECISION / total_supply
```

---

## 5. Deposit Flow (Multi-Asset)

```
1. Verify asset ∈ vault.DepositAssets
2. Compute NAV before transfer (snapshot)
3. Transfer asset from user to vault
4. deposit_value = oracle.get_price(asset) × amount_after_fee (in base_asset terms)
5. if total_supply == 0: shares_minted = deposit_value   [only if seed deposit exists]
   else: shares_minted = deposit_value × total_supply / nav_before
6. share_token.mint(user, shares_minted)
7. Update user.cost_basis += deposit_value
8. Emit DepositEvent
```

**Note:** `total_supply` is never 0 after factory creation (seed deposit ensures this).

---

## 6. Withdrawal Flow (Proportional Multi-Asset — dHedge V2 Model)

```
1. shares_burned  = requested_amount
2. total_supply   = share_token.total_supply()
3. numerator      = shares_burned
4. denominator    = total_supply

5. BURN SHARES FIRST (reentrancy protection)
   share_token.burn(user, shares_burned)

6. Compute realized PnL before transfer:
   avg_cost_per_share = user.cost_basis / (total_supply + shares_burned)
   withdrawal_cost    = avg_cost_per_share × shares_burned
   current_share_price = NAV_before × PRICE_PRECISION / total_supply
   withdrawal_value   = shares_burned × current_share_price / PRICE_PRECISION
   realized_gain      = withdrawal_value - withdrawal_cost
   user.realized_pnl += realized_gain
   user.cost_basis   -= withdrawal_cost

7. For each asset ∈ PortfolioAssets:
      amount = floor(token_balance(vault, asset) × numerator / denominator)
      if amount > 0: token_transfer(vault → user, amount)

8. For each guard ∈ ActiveGuards:
      guard.withdraw_fraction(vault, numerator, denominator, user)
      // guard sends underlying tokens directly to user

9. Apply exit fee (deducted from each asset proportionally or in base_asset)

10. Emit WithdrawEvent
```

**No swaps. No auto-unwind. User receives native tokens from every position.**

---

## 7. Strategy Guard Interface

### 7.1 Mandatory interface (called by vault internals)

Every guard contract MUST implement these functions.  The vault always passes itself as the first argument; guards must accept it.

```rust
/// Total value of all vault positions in this protocol, in base_asset terms.
fn get_total_value(vault: Address) -> i128

/// Withdraw `numerator/denominator` fraction of all positions.
/// Sends underlying tokens directly to `to` (not back to vault).
fn withdraw_fraction(vault: Address, numerator: i128, denominator: i128, to: Address)

/// Returns true if any position of `vault` in this protocol involves `asset`.
/// Used by vault to gate remove_portfolio_asset.
fn asset_in_use(vault: Address, asset: Address) -> bool

/// Position breakdown for UI/analytics.
fn get_positions(vault: Address) -> Vec<StrategyPosition>
```

### 7.2 Trader-callable functions (dispatched via vault.execute_op)

Guards expose named Soroban functions.  The manager whitelists their names in `AuthorizedOps(guard)`.  The vault injects its own address as the first argument before calling — the trader supplies only the remaining args.

**Dispatch flow:**
```
manager/trader calls:   vault.execute_op(caller, guard, "supply", [asset, amount])
vault checks:           "supply" ∈ AuthorizedOps(guard)
vault builds full_args: [vault_addr, asset, amount]
vault calls:            guard.supply(vault_addr, asset, amount)
```

The manager/trader **cannot** substitute a different `vault_addr` — it is always injected by the vault contract itself.  This prevents funds from being routed to arbitrary external addresses.

### 7.3 DEX Guard (Soroswap, Phoenix)

**Internal state:**
```
DataKey::LPPositions(vault)  →  Vec<(pair: Address, lp_balance: i128)>
```

**get_total_value:** Σ value of each LP position using reserve decomposition + oracle  
**withdraw_fraction:** For each position → `remove_liquidity(fraction × lp_amount)` → tokens to `to`  
**asset_in_use:** Returns true if any LP position pair contains `asset`

**Authorized trader functions:**
```rust
fn swap(vault: Address, from_asset: Address, to_asset: Address, amount_in: i128, min_out: i128)
fn add_liquidity(vault: Address, token_a: Address, token_b: Address, amount_a: i128, amount_b: i128, min_lp: i128)
fn remove_liquidity(vault: Address, pair: Address, lp_amount: i128, min_a: i128, min_b: i128)
```

### 7.4 Lending Guard (Blend)

**Internal state:**
```
DataKey::LendingPositions(vault)  →  Vec<(reserve: Address, supplied: i128)>
```

**get_total_value:** Σ `oracle.price(asset) × supplied_amount` for each position  
**withdraw_fraction:** For each position → `pool.withdraw(fraction × supplied)` → token to `to`  
**asset_in_use:** Returns true if any lending position involves `asset`

**Authorized trader functions:**
```rust
fn supply(vault: Address, asset: Address, amount: i128)
fn withdraw_from_lending(vault: Address, asset: Address, amount: i128)
```

---

## 8. PnL Tracking

**Storage per user per vault:**
```rust
DataKey::UserPosition(user: Address)  →  UserPosition {
    cost_basis:   i128,   // current holdings' total cost in base_asset terms
    realized_pnl: i128,   // accumulated realized gains/losses from withdrawals
}
```

**On deposit:**
```
user.cost_basis += oracle.price(deposit_asset) × net_amount
```

**On withdrawal:**
```
avg_cost = user.cost_basis / shares_held_before
realized = (current_share_value - avg_cost) × shares_burned
user.realized_pnl += realized
user.cost_basis   -= avg_cost × shares_burned
```

**View function:**
```rust
fn get_user_pnl(user: Address) -> UserPnLReport {
    cost_basis,
    current_value,    // shares × current_share_price
    unrealized_pnl,   // current_value - cost_basis
    realized_pnl,
    total_pnl,
}
```

---

## 9. Inflation Attack Prevention (Seed Deposit)

Factory `create_vault()` atomically:
1. Deploys the vault
2. Transfers `seed_amount` of `base_asset` from factory to vault
3. Mints `seed_amount` shares to `BURN_ADDRESS` (a fixed all-zeros address)

Result: `total_supply > 0` from day one. The "first depositor" attack vector is eliminated.

`seed_amount` should be at least `10^(base_asset_decimals)` (e.g., 1 USDC = 1_000_000 stroops).

---

## 10. Factory Storage Layout (Updated)

```
DataKey::Admin                  instance   Address
DataKey::PendingAdmin           instance   Address
DataKey::VaultCount             instance   u32
DataKey::VaultByIndex(u32)      persistent Address
DataKey::VaultPosition(Address) persistent u32
DataKey::IsRegistered(Address)  persistent bool
DataKey::VaultManager(Address)  persistent Address         [NEW]
DataKey::AuthorizedAssets       persistent Vec<Address>    [NEW]
DataKey::AuthorizedGuards       persistent Vec<Address>    [NEW]
DataKey::Initialized            persistent bool
```

---

## 11. Vault Storage Layout (Updated)

```
DataKey::Admin                  instance   Address
DataKey::Manager                instance   Address
DataKey::Trader                 instance   Address
DataKey::BaseAsset              instance   Address
DataKey::ShareToken             instance   Address
DataKey::Paused                 instance   bool             [deposits+withdrawals; admin-controlled]
DataKey::OpsPaused              instance   bool             [execute_op; admin-controlled]
DataKey::PrivatePool            instance   bool             [member-only deposits; admin-controlled]
DataKey::Member(Address)        instance   bool             [per-address allowlist; admin-controlled]
DataKey::EntryFeeBps            instance   u32
DataKey::ExitFeeBps             instance   u32
DataKey::MgmtFeeBps             instance   u32
DataKey::PerfFeeBps             instance   u32
DataKey::LastMgmtFeeTs          instance   u64
DataKey::HighWaterMark          instance   i128
DataKey::DepositCap             instance   i128
DataKey::MaxLossBps             instance   u32
DataKey::Oracle                 instance   Address          [UPDATED: now required for multi-asset NAV]
DataKey::PortfolioAssets        instance   Vec<Address>     [NEW]
DataKey::DepositAssets          instance   Vec<Address>     [NEW]
DataKey::ActiveGuards           instance   Vec<Address>     [NEW — replaces Strategies]
DataKey::AuthorizedOps(guard)   instance   Vec<Symbol>      [NEW — whitelisted function names per guard]
DataKey::Factory                instance   Address          [NEW — to verify asset whitelist]
DataKey::UserPosition(Address)  persistent UserPosition     [NEW — PnL tracking]
```

---

## 12. Events (Updated)

```
DepositEvent    { user, asset, amount, shares_minted, share_price, nav, ledger }
WithdrawEvent   { user, shares_burned, realized_pnl, share_price, nav, ledger }
ExecuteOpEvent  { guard, fn_name, args_len }   — guard addr + function dispatched + arg count
PortfolioAssetAdded   { asset }
PortfolioAssetRemoved { asset }
DepositAssetAdded     { asset }
DepositAssetRemoved   { asset }
GuardAdded            { guard }
GuardRemoved          { guard }
AssetAuthorized       { asset }          [factory]
GuardAuthorized       { guard }          [factory]
VaultCreated          { vault, manager, seed_amount }  [factory]
```

---

## 13. Security Invariants

| Invariant | Enforcement |
|---|---|
| Shares burned before any transfer | Withdraw burns shares in step 5, transfers in steps 7-8 |
| NAV snapshot before deposit | NAV computed before tokens received |
| Funds only leave vault through vault itself | Vault injects own address as first arg in execute_op — trader cannot substitute a different source |
| No unauthorized guard functions | fn_name must be in AuthorizedOps(guard) before dispatch |
| No unknown assets acquired | Guard validates output asset ∈ PortfolioAssets |
| No unauthorized guards | Guard must be in factory.AuthorizedGuards |
| No unauthorized assets | Asset must be in factory.AuthorizedAssets |
| Oracle required for non-base deposits | Checked at deposit time |
| LP asset cannot be silently removed | asset_in_use() blocks removal |
| Seed deposit prevents first-depositor attack | factory.create_vault() ensures total_supply > 0 |
| Max strategies / guards bounded | MAX_GUARDS = 10 to bound NAV cost |

---

## 14. AssetHandler Oracle Design

### 14.1 Overview

`AssetHandler` uses a **three-tier oracle resolution** for pricing any registered asset:

```
get_price(asset)
  │
  ├─ Tier 1: Per-asset oracle (optional override)
  │     Set via set_asset_oracle(asset, oracle).
  │     Useful for assets not on Reflector or needing a custom feed.
  │     → invoke_contract (hard fail on revert)
  │     → if price > 0: return price; else fall through to global oracles
  │
  ├─ Tier 2: Primary global oracle (Reflector)
  │     → try_invoke_contract (catches reverts gracefully)
  │     → if Ok(price) and price > 0: return price; else fall through
  │
  ├─ Tier 3: Fallback global oracle (DIA)
  │     → invoke_contract (hard fail on revert)
  │     → if price > 0: return price; else panic PriceNotAvailable
  │
  └─ Panic PriceNotAvailable
```

### 14.2 Oracle Adapter Interface

Every oracle — per-asset, primary, or fallback — must expose:

```rust
fn get_price(asset: Address) -> i128   // PRICE_PRECISION-scaled; 0 = unavailable
```

### 14.3 AssetHandler Storage

```
DataKey::Admin                  persistent  Address
DataKey::Initialized            persistent  bool
DataKey::RegisteredAssets       persistent  Vec<Address>
DataKey::AssetOracle(Address)   persistent  Address    // optional per-asset oracle
DataKey::PrimaryOracle          persistent  Address    // global primary (Reflector)
DataKey::FallbackOracle         persistent  Address    // global fallback (DIA)
```

### 14.4 Admin Functions

| Function | Description |
|---|---|
| `add_asset(asset)` | Register an asset (admin only) |
| `remove_asset(asset)` | Deregister an asset; also clears its per-asset oracle (admin only) |
| `set_primary_oracle(oracle)` | Set global primary oracle (admin only) |
| `set_fallback_oracle(oracle)` | Set global fallback oracle (admin only) |
| `set_asset_oracle(asset, oracle)` | Set per-asset oracle override (admin only) |
| `remove_asset_oracle(asset)` | Remove per-asset oracle override (admin only) |

### 14.5 Production vs Development Oracles

The `contracts/oracle/` contract is a **development / test mock only**. Do not deploy it in production. For production:

- **Primary oracle**: Deploy a Reflector adapter that implements `get_price(asset) -> i128` and reads from Reflector's `lastprice(Asset)` interface.
- **Fallback oracle**: Deploy a DIA adapter that implements `get_price(asset) -> i128` and reads from DIA's cross-chain oracle.

### 14.6 VaultParams Extension

`VaultParams` includes `is_private: bool` to set the private-pool mode at vault construction time. This replaces the previous hardcoded `false` default. The field can be changed post-deployment via `set_private_pool(admin, is_private)`.

---

## 15. Implementation Order

1. **Factory** — AuthorizedAssets, AuthorizedGuards, VaultManager, create_vault  
2. **Vault storage** — PortfolioAssets, DepositAssets, ActiveGuards, AuthorizedOps, Factory ref, UserPosition  
3. **Vault NAV** — multi-asset formula  
4. **Vault deposit** — multi-asset input with oracle pricing  
5. **Vault withdraw** — proportional multi-asset dHedge model  
6. **Vault PnL** — UserPosition tracking + get_user_pnl  
7. **Blend guard** — get_total_value, withdraw_fraction, asset_in_use  
8. **Soroswap LP guard** — get_total_value, withdraw_fraction, asset_in_use  
9. **Phoenix LP guard** — get_total_value, withdraw_fraction, asset_in_use  
10. **ReflectorAdapter** — `get_price(asset) -> i128` adapting Reflector's `lastprice` interface  
11. **DIAAdapter** — `get_price(asset) -> i128` adapting DIA's oracle interface  
12. **Unit tests** — all new functions  
13. **Integration tests** — full multi-asset lifecycle  
