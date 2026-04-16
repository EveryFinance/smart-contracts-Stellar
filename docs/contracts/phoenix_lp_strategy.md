# Phoenix LP Strategy Contract

Path: `contracts/strategies/phoenix_lp`

## Purpose

Two-asset LP strategy adapter for Phoenix pools.

## Public Methods

- `initialize(vault, asset_a, asset_b, phoenix_pool, manager, name)`
- `deposit_liquidity(amount_a, amount_b, min_a, min_b, from) -> i128`
- `withdraw(share_amount, min_a, min_b, from, to) -> (i128, i128)`
- `get_share_balance() -> i128`
- `get_value(_vault) -> i128`
- `set_oracle(caller, oracle)`
- `asset_a()`, `asset_b()`, `share_token()`, `get_name()`, `is_paused()`, `has_oracle()`
- `pause(caller)`, `unpause(caller)`

## Access Control

- Liquidity ops are vault-only (`from == vault`).
- Admin ops are manager-only.

## Notes

- On initialize, strategy auto-queries pool share token address.
- Value can use reserve decomposition when oracle is configured.
