# Soroswap LP Strategy Contract

Path: `contracts/strategies/soroswap_lp`

## Purpose

Two-asset LP strategy adapter for Soroswap pools. Implements both the **guard interface** (called by vault internals during NAV/withdrawal) and the **trader-callable operations** (dispatched via `vault.execute_op`).

## Guard Interface (called by vault)

```rust
fn get_total_value(vault: Address) -> i128
fn withdraw_fraction(vault: Address, numerator: i128, denominator: i128, to: Address)
fn asset_in_use(vault: Address, asset: Address) -> bool
```

## Trader-Callable Operations (via vault.execute_op)

```rust
fn add_liquidity(vault: Address, amount_a: i128, amount_b: i128, min_a: i128, min_b: i128)
fn remove_liquidity(vault: Address, lp_amount: i128, min_a: i128, min_b: i128)
fn swap(vault: Address, from_asset: Address, to_asset: Address, amount_in: i128, min_out: i128)
```

These are dispatched by the vault after checking the function name is in `AuthorizedOps(guard)`. The vault injects its own address as the first argument.

## Admin Methods

- `initialize(vault, asset_a, asset_b, lp_token, router, manager, name)`
- `set_oracle(caller, oracle)`
- `pause(caller)` / `unpause(caller)`

## Views

- `get_lp_balance() -> i128`
- `get_value(vault) -> i128`
- `asset_a()`, `asset_b()`, `lp_token()`, `get_router()`, `get_name()`
- `is_paused()`, `has_oracle()`

## Access Control

- Trader-callable ops are vault-only (vault address injected as first arg).
- Admin ops are manager-only.

## Valuation

- Without oracle: uses LP balance proportional to pool reserves.
- With oracle: decomposes reserves and prices each token via oracle.
