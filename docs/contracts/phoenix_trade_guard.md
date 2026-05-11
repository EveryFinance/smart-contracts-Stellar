# Phoenix Trade Guard — Removed

> **This contract has been removed.** The `contracts/trade_guards/phoenix` directory no longer exists.

## Why it was removed

Strategy contracts are the guards. The Phoenix LP strategy (`contracts/strategies/phoenix_lp`) implements both:

1. The **guard interface** (`get_total_value`, `withdraw_fraction`, `asset_in_use`) called by vault internals.
2. The **trader-callable operations** (`swap`, `add_liquidity`, `remove_liquidity`) dispatched via `vault.execute_op`.

All validation (direction checks, slippage guards, amount checks) lives inside these operation functions. A separate trade guard contract that duplicated the same checks was redundant.

## Replacement

See [Phoenix LP Strategy](./phoenix_lp_strategy.md) for the current interface.

The vault enforces operation-level authorization via `AuthorizedOps(guard)` — the manager whitelists exactly which function names the trader may call per strategy.
