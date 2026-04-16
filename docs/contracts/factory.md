# Factory Contract

Path: `contracts/factory`

## Purpose

Registry for vault discovery and lifecycle management.

## Public Methods

Admin:
- `initialize(admin)`
- `register_vault(caller, vault, manager)`
- `verify_and_register_vault(caller, vault)`
- `remove_vault(caller, vault)`
- `set_admin(caller, new_admin)`

Views:
- `get_vaults(offset, limit) -> Vec<Address>`
- `get_vault_count() -> u32`
- `get_admin() -> Address`
- `is_registered(vault) -> bool`

## Registration Paths

- `verify_and_register_vault` is preferred.
  - Reads `vault.get_manager()` on-chain and only registers initialized vaults.
- `register_vault` requires caller to pass a `manager` argument matching on-chain value.

## Security Notes

- keep admin role separate from manager/trader.
- use only verify path in runbooks.
