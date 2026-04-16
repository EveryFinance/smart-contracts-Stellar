# Vault Contract

Path: `contracts/vault`

## Purpose

Main protocol engine for:
- deposits/withdrawals,
- share accounting,
- strategy allocation,
- trading control,
- fee accrual,
- NAV computation and risk guardrails.

## Key Roles

- Manager: strategy and config operations.
- Trader: spot trade execution.

## Initialization

`initialize(params: VaultParams)` where `VaultParams` includes:
- `manager`, `trader`, `base_asset`, `share_token`, `share_token_admin`
- `entry_fee_bps`, `exit_fee_bps`, `mgmt_fee_bps`, `perf_fee_bps`

Security-critical behavior:
- Vault checks current share-token admin.
- If not already vault, it must equal `share_token_admin` and then vault calls `share_token.set_admin(vault)`.

## Public Methods

Core:
- `deposit(amount, from) -> i128`
- `withdraw(share_amount, from, to) -> i128`

Strategy control:
- `set_strategies(caller, strategies)`
- `invest(caller, strategy, amount) -> i128`
- `unwind(caller, strategy, units) -> i128`
- `invest_lp(caller, strategy, amount_a, amount_b, min_a, min_b) -> i128`
- `unwind_lp(caller, strategy, lp_amount, min_a, min_b) -> (i128, i128)`

Trading:
- `set_trade_guard(caller, strategy, guard)`
- `execute_trade(caller, strategy, amount_in, min_out, path) -> i128`

Risk/config:
- `set_oracle`, `set_strategy_oracle_token`, `set_lp_strategy`
- `set_max_loss_bps`, `set_max_concentration_bps`, `set_deposit_cap`
- `set_entry_fee_bps`, `set_exit_fee_bps`, `set_mgmt_fee_bps`, `set_perf_fee_bps` (decrease-only direct setters)
- `announce_fee_increase`, `commit_fee_increase`, `renounce_fee_increase`
- `set_private_pool`, `add_member`, `remove_member`
- `set_exit_cooldown_secs`
- `set_value_guard_enabled`
- `pause`, `unpause`, `set_manager`, `set_trader`

Views:
- `get_nav`, `get_share_price`
- `get_manager`, `get_trader`, `get_base_asset`, `get_share_token`, `get_strategies`, `is_paused`
- `is_private_pool`, `is_member_allowed`
- `get_exit_cooldown_secs`, `get_exit_remaining_cooldown`
- `get_announced_fees`
- `is_value_guard_enabled`

## Important Invariants

1. Share mint/burn must remain proportional to NAV.
2. Deposits/withdrawals only in base asset.
3. Only whitelisted strategies can receive allocations.
4. Trader can only trade through configured guard.
5. NAV guard and concentration guard enforce post-action limits.
6. LP strategies need proper valuation wiring (or NAV reverts where required).
7. In private-pool mode, non-members cannot deposit.
8. Exit cooldown blocks withdrawals until `last_deposit_ts + cooldown`.
9. Fee increases require `announce -> timelock -> commit`.
10. Same-ledger operation/value guard detects suspicious sequence/value drift.

## Error Highlights

- `GuardNotSet`, `StrategyNotWhitelisted`, `NotManager`, `NotTrader`
- `TvlGuardTripped`, `ConcentrationLimitExceeded`
- `LpStrategyOracleRequired`, `InsufficientLiquidity`
- `ShareTokenAdminMismatch`
- `CooldownActive`, `NotMember`
- `FeeIncreaseDelayActive`, `NoFeeIncreaseAnnounced`
- `OperationTypeMismatch`, `ValueManipulationDetected`
