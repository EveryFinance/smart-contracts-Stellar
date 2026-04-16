# Phoenix Trade Guard Contract

Path: `contracts/trade_guards/phoenix`

## Purpose

Policy enforcement for Phoenix multi-hop swap parameters.

## Constants

- `MAX_OPERATIONS = 4`
- `MAX_SLIPPAGE_BPS = 1000` (10%)

## Public Methods

- `initialize(vault, manager, tokens)`
- `set_whitelist(caller, tokens)`
- `validate_swap(caller, amount_in, min_out, operations, quoted_out)`
- `validate_swap_exact_in(caller, amount_in, min_out, path, quoted_out)`
- `validate_invest(caller, amount)`
- `validate_unwind(caller, units)`
- `get_whitelist()`, `get_vault()`, `get_manager()`

## Validation Rules

- vault-only caller.
- amount positivity.
- operation/path count bounds.
- whitelist checks across all offered/asked tokens.
- slippage check versus quoted output.
