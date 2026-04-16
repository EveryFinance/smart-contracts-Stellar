# Share Token Contract (SEP-41)

Path: `contracts/share_token`

## Purpose

Vault ownership token implementing SEP-41 behaviors with controlled mint/burn admin.

## Initialization

`initialize(admin, name, symbol, decimals)`

The vault is expected to become token admin during vault initialization handoff.

## Public Methods

Metadata and state:
- `name`, `symbol`, `decimals`, `total_supply`, `get_admin`

SEP-41:
- `balance`, `allowance`
- `approve(from, spender, amount, expiration_ledger)`
- `transfer(from, to, amount)`
- `transfer_from(spender, from, to, amount)`
- `burn(from, amount)`
- `burn_from(spender, from, amount)`

Admin:
- `mint(to, amount)`
- `set_admin(new_admin)`

## Security Notes

- `mint` requires current admin auth.
- `set_admin` requires current admin auth.
- allowance expiration is enforced at use-time.
- amount positivity and arithmetic overflow checks are explicit.

## Error Highlights

- `InsufficientAllowance`, `InsufficientBalance`
- `NegativeAmount`, `ZeroAmount`, `Overflow`
