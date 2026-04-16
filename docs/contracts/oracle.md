# Oracle Contract

Path: `contracts/oracle`

## Purpose

Admin-updated price oracle used by vault/strategies for NAV conversion.

## Data Model

For each asset, oracle stores:
- `price` (precision-scaled i128)
- `updated_ledger` (u32)

Global config:
- `max_age_ledgers` for freshness enforcement.

## Public Methods

Admin:
- `initialize(admin)`
- `set_admin(new_admin)`
- `set_max_age_ledgers(max_age_ledgers)`

Price ops:
- `set_price(asset, price)`
- `get_price(asset) -> i128`
- `get_prices(assets) -> Map<Address, i128>`

Views:
- `get_admin`, `get_max_age_ledgers`

## Freshness Behavior

- Reads enforce `current_ledger - updated_ledger <= max_age_ledgers` when `max_age_ledgers > 0`.
- violation raises `StalePrice`.
- `max_age_ledgers = 0` disables staleness checks.

## Security Notes

- Oracle is governance-sensitive; compromised admin can bias NAV.
- production should use multisig/timelock and monitoring.
