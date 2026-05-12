# Share Token Contract (SEP-41)

Path: `contracts/share_token`

## Purpose

Vault ownership token implementing SEP-41-style accounting with controlled mint/burn admin.

Vault shares are non-transferable by default. This is intentional: the vault's
exit cooldown and per-user PnL accounting are keyed to the account that deposits
and later withdraws. If shares could move freely by default, a user could move
shares to another address and bypass same-address cooldown/PnL assumptions.

## Initialization

`initialize(admin, name, symbol, decimals)`

The vault is expected to become token admin during vault initialization handoff.

## Public Methods

Metadata and state:
- `name`, `symbol`, `decimals`, `total_supply`, `get_admin`
- `transfers_enabled() -> bool`

SEP-41:
- `balance`, `allowance`
- `approve(from, spender, amount, expiration_ledger)`
- `transfer(from, to, amount)` - reverts unless transfers are enabled
- `transfer_from(spender, from, to, amount)` - reverts unless transfers are enabled
- `burn(from, amount)` - admin/vault-only while transfers are disabled; holder burn is allowed only when transfers are enabled
- `burn_from(spender, from, amount)` - reverts unless transfers are enabled

Admin:
- `mint(to, amount)`
- `set_admin(new_admin)`
- `set_transfers_enabled(enabled)`

The vault is expected to be the share token admin after initialization, so
transferability should be changed through the vault-level admin method rather
than by direct operational access to the share token.

## Security Notes

- `mint` requires current admin auth.
- `set_admin` requires current admin auth.
- `set_transfers_enabled` requires current admin auth.
- `transfer` and `transfer_from` revert with `TransfersDisabled` while
  `transfers_enabled() == false`.
- Direct holder burns are blocked in default mode; default-mode burns should
  happen through vault withdrawal so cooldown and PnL state stay consistent.
- Delegated `burn_from` is blocked while transfers are disabled.
- allowance expiration is enforced at use-time.
- amount positivity and arithmetic overflow checks are explicit.

## Error Highlights

- `InsufficientAllowance`, `InsufficientBalance`
- `NegativeAmount`, `ZeroAmount`, `Overflow`
- `TransfersDisabled`
