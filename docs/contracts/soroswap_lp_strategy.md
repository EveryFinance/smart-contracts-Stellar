# Soroswap LP Strategy Contract

Path: `contracts/strategies/soroswap_lp`

## Purpose

Two-asset LP strategy adapter for Soroswap pools.

## Public Methods

- `initialize(vault, asset_a, asset_b, lp_token, router, manager, name)`
- `deposit_liquidity(amount_a, amount_b, min_a, min_b, from) -> i128`
- `withdraw(lp_amount, min_a, min_b, from, to) -> (i128, i128)`
- `get_lp_balance() -> i128`
- `get_value(_vault) -> i128`
- `set_oracle(caller, oracle)`
- `asset_a()`, `asset_b()`, `lp_token()`, `get_router()`, `get_name()`, `is_paused()`, `has_oracle()`
- `pause(caller)`, `unpause(caller)`

## Access Control

- Liquidity ops are vault-only (`from == vault`).
- Admin ops are manager-only.

## Valuation

- Without oracle: can fall back to LP balance-based valuation.
- With oracle: decomposes reserves for base-value estimate.

## Notes

- Vault marks this strategy as LP via `set_lp_strategy`.
- LP strategies are not auto-unwound in vault proportional shortfall loop.
