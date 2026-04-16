# Soroswap Trade Guard Contract

Path: `contracts/trade_guards/soroswap`

## Purpose

Policy enforcement for Soroswap swap parameters before vault strategy trade execution.

## Constants

- `MAX_PATH_LEN = 5`
- `MAX_SLIPPAGE_BPS = 1000` (10%)

## Public Methods

- `initialize(vault, manager, tokens)`
- `set_whitelist(caller, tokens)`
- `validate_swap_exact_in(caller, amount_in, min_out, path, quoted_out)`
- `validate_swap_exact_out(caller, amount_out, max_in, path, quoted_in)`
- `validate_invest(caller, amount)`
- `validate_unwind(caller, units)`
- `get_whitelist()`, `get_vault()`, `get_manager()`

## Validation Rules

Exact-in:
- caller must be vault,
- amount must be positive,
- path length 2..MAX_PATH_LEN,
- all path tokens whitelisted,
- slippage check on `(quoted_out - min_out)/quoted_out`.

Exact-out:
- caller must be vault,
- amount/max_in positive,
- path/whitelist checks,
- slippage check on `(max_in - quoted_in)/quoted_in`.

## Notes

- Exact-out slippage enforcement was added in the latest remediation.
