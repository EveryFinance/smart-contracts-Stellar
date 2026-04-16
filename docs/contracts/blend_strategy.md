# Blend Strategy Contract

Path: `contracts/strategies/blend`

## Purpose

Single-asset strategy adapter for Blend protocol.

## Public Methods

- `initialize(vault, asset, protocol, manager, name)`
- `deposit(amount, from) -> i128`
- `withdraw(amount, from, to) -> i128`
- `get_value(_vault) -> i128`
- `sync_position(caller, actual_position)`
- `asset()`, `get_protocol_address()`, `get_name()`, `is_paused()`
- `pause(caller)`, `unpause(caller)`

## Access Control

- `deposit`/`withdraw`: vault-only (`from == vault`, auth required).
- `pause`/`unpause`/`sync_position`: manager-only.

## Notes

- Strategy receives funds from vault and interacts with Blend.
- Value is exposed via `get_value` for vault NAV.
