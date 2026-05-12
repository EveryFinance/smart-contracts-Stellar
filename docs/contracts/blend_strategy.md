# Blend Strategy Contract

Path: `contracts/strategies/blend`

## Purpose

Single-asset strategy adapter for Blend protocol.

## Public Methods

- `initialize(vault, asset, protocol, name)`
- `supply(vault, amount)`
- `withdraw_from_lending(vault, amount)`
- `withdraw_fraction(vault, numerator, denominator, to)`
- `get_value(_vault) -> i128`
- `get_total_value(vault) -> i128`
- `sync_position(caller, actual_position)`
- `asset()`, `get_protocol_address()`, `get_name()`

## Access Control

- Strategy execution functions are vault-only: the vault address is injected by `vault.execute_op`.
- `sync_position`: manager-only.
- Pause/unpause live only on the vault; the strategy does not store a manager or pause flag.

## Notes

- Strategy receives funds from vault and interacts with Blend.
- Value is exposed via `get_value` for vault NAV.
