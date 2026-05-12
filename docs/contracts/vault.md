# Vault Contract

Path: `contracts/vault`

## Purpose

Main protocol engine for:
- deposits and withdrawals,
- share accounting,
- strategy allocation via `execute_op`,
- fee accrual,
- NAV computation and risk guardrails.

## Roles

| Role | Who | Responsibilities |
|------|-----|-----------------|
| **Admin** | Set at construction | Emergency controls, private-pool management, share-transfer mode, admin transfer |
| **Manager** | Assigned by admin | Strategy config, fee config, oracle/guard wiring, trader assignment |
| **Trader** | Assigned by manager | Executes operations via `execute_op` |

## Initialization

`__constructor(params: VaultParams)` where `VaultParams` includes:

```rust
pub struct VaultParams {
    pub admin: Address,
    pub manager: Address,
    pub manager_name: Option<String>,
    pub trader: Address,
    pub base_asset: Address,
    pub share_token: Address,
    pub share_token_admin: Address,
    pub treasury: Address,
    pub entry_fee_bps: u32,       // 0–500
    pub exit_fee_bps: u32,        // 0–500
    pub mgmt_fee_bps: u32,        // 0–300
    pub perf_fee_bps: u32,        // 0–3000
    pub factory: Option<Address>,
    pub is_private: bool,         // start in private-pool mode
}
```

Security-critical behavior:
- Vault checks current share-token admin.
- If not already vault, it must equal `share_token_admin` then vault calls `share_token.set_admin(vault)`.

## Public Methods

### Core user operations

- `deposit(amount, from, asset, min_shares_out) -> i128`
- `withdraw(share_amount, from, to, min_base_out) -> i128`
- `seed_deposit(from, amount)` — one-time seed to prevent first-depositor attack

### Strategy / guard execution (manager or trader)

- `execute_op(caller, guard, fn_name, args) -> Val`
  - Vault injects its own address as first arg before calling the guard
  - `fn_name` must be in `AuthorizedOps(guard)`
  - Strategy lifecycle/view/rescue functions are reserved and cannot be authorized through `execute_op`
  - Blocked when `OpsPaused` is set

### Multi-asset portfolio management (manager)

- `add_portfolio_asset(caller, asset)`
- `remove_portfolio_asset(caller, asset)`
- `add_deposit_asset(caller, asset)`
- `remove_deposit_asset(caller, asset)`
- `add_active_guard(caller, guard)`
- `remove_active_guard(caller, guard)`
- `set_authorized_ops(caller, guard, ops)`

### Fee configuration (manager)

- `set_entry_fee_bps`, `set_exit_fee_bps`, `set_mgmt_fee_bps`, `set_perf_fee_bps` — decrease-only direct setters
- `announce_fee_increase(entry, exit, mgmt, perf)`, `commit_fee_increase`, `renounce_fee_increase` — timelock flow for increases

### Fee settlement (permissionless)

- `collect_pending_fees() -> i128`
  - Settles accrued management and performance fees without requiring a deposit or withdrawal.
  - Mints deterministic fee shares to treasury only.
  - Caller does not provide NAV, fee amounts, recipient, timestamp, or price inputs.
  - Returns the number of fee shares minted.

### Risk / config (manager)

- `set_oracle(caller, oracle)`
- `set_max_loss_bps(caller, bps)`
- `set_deposit_cap(caller, cap)`
- `set_exit_cooldown_secs(caller, secs)`
- `set_value_guard_enabled(caller, enabled)`
- `set_manager(caller, new_manager)`
- `set_trader(caller, new_trader)`

### Emergency controls (admin only)

- `pause_deposits(caller)` / `unpause_deposits(caller)` — blocks `deposit` and `withdraw`
- `pause_operations(caller)` / `unpause_operations(caller)` — blocks `execute_op`
- `set_share_transfers_enabled(caller, enabled)` — opts vault shares into or out of transferability

### Private-pool management (admin only)

- `set_private_pool(caller, is_private)` — toggle member-only deposit mode
- `add_member(caller, member)` / `remove_member(caller, member)` — manage allowlist

### Admin transfer (two-step, admin only)

- `set_pending_admin(caller, new_admin)`
- `accept_admin(caller)`

### Views

- `get_nav() -> i128`, `get_share_price() -> i128`
- `get_admin`, `get_manager`, `get_trader`, `get_base_asset`, `get_share_token`, `get_treasury`
- `get_portfolio_assets`, `get_deposit_assets`, `get_active_guards`
- `get_authorized_ops(guard)`
- `get_factory`
- `is_paused() -> bool` — deposits/withdrawals paused
- `is_ops_paused() -> bool` — execute_op paused
- `is_private_pool() -> bool`, `is_member_allowed(member) -> bool`
- `share_transfers_enabled() -> bool`
- `exit_cooldown_is_hard_control() -> bool`
- `pnl_tracking_is_accurate() -> bool`
- `get_exit_cooldown_secs`, `get_announced_fees`
- `is_value_guard_enabled`
- `get_user_pnl(user) -> UserPnLReport`

### Share transferability, cooldown, and PnL

Vault shares are non-transferable by default. In this default mode:
- `share_transfers_enabled() == false`
- `exit_cooldown_is_hard_control() == true`
- `pnl_tracking_is_accurate() == true`

This is the secure default because cooldown and PnL tracking are account-based.
The vault records `LastDepositTs(user)` for cooldown and `UserPosition(user)` for
cost basis / realized PnL. Keeping shares non-transferable ensures the account
that receives shares through `deposit` is the same account whose cooldown and PnL
state is used during `withdraw`.

Default mode also routes share burns through vault withdrawal. Direct delegated
burns are blocked while transfers are disabled, preventing out-of-vault burns
from desynchronizing share balances from vault PnL state.

The vault admin may explicitly enable transfers with
`set_share_transfers_enabled(caller, true)`. This is an opt-in mode for vaults
that accept weaker account-level guarantees:
- cooldown becomes same-address friction only, not a hard protocol control,
- PnL becomes approximate / informational only,
- `exit_cooldown_is_hard_control() == false`,
- `pnl_tracking_is_accurate() == false`.

To keep accurate PnL and hard cooldown while allowing transfers, a future design
would need transfer-aware accounting hooks that move or recompute cost basis and
cooldown state with the transferred shares.

## execute_op Dispatch

```
trader/manager calls: vault.execute_op(caller, guard, "swap", [from_asset, to_asset, amount])
vault checks:         guard ∈ ActiveGuards
                      "swap" ∈ AuthorizedOps(guard)
                      "swap" is not a reserved lifecycle/view function
                      OpsPaused == false
vault builds args:    [vault_addr, from_asset, to_asset, amount]
vault calls:          guard.swap(vault_addr, from_asset, to_asset, amount)
```

The vault always injects its own address as the first argument — the trader cannot substitute a different source.
Reserved functions include strategy initialization, vault withdrawal helpers, direct deposit/withdraw entrypoints, pause controls, and NAV/view helpers. Manager/trader execution should use explicit trader operations such as `supply`, `withdraw_from_lending`, `add_liquidity`, `remove_liquidity`, and `swap`.

## Guard Interface (strategy contracts)

Every active guard must implement:

```rust
fn get_total_value(vault: Address) -> i128
fn withdraw_fraction(vault: Address, numerator: i128, denominator: i128, to: Address)
fn asset_in_use(vault: Address, asset: Address) -> bool
```

## Important Invariants

1. Shares burned before any transfer in withdrawals (reentrancy protection).
2. NAV snapshot taken before deposit transfer.
3. Funds only leave vault through vault itself (vault injects own address in `execute_op`).
4. Only whitelisted guards can receive `execute_op` calls.
5. Only authorized function names per guard are dispatched.
6. NAV loss guard (`max_loss_bps`) enforces post-operation limits.
7. In private-pool mode, only admin, manager, and allowlisted members can deposit.
8. Shares are non-transferable by default, preserving account-based cooldown and PnL accuracy.
9. Exit cooldown blocks withdrawals until `last_deposit_ts + cooldown_secs` while shares remain non-transferable.
10. Fee increases require announce → timelock (86 400 s) → commit.
11. Same-ledger operation/value guard detects suspicious sequence/value drift.

## Error Codes

| Code | Name | Description |
|------|------|-------------|
| #3 | `NotManager` | Caller is not the manager |
| #4 | `NotTrader` | Caller is not the manager or trader |
| #5 | `Paused` | Deposits/withdrawals are paused |
| #9 | `TradeGuardRejected` | fn_name not in AuthorizedOps |
| #17 | `TvlGuardTripped` | NAV drop exceeded max_loss_bps |
| #23 | `CooldownActive` | Withdrawal before cooldown elapsed |
| #24 | `NotMember` | Non-member deposit into private pool |
| #43 | `OperationsPaused` | execute_op is paused by admin |
| #44 | `NotAdmin` | Caller is not the admin |
