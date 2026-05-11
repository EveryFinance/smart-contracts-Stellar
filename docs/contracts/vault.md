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
| **Admin** | Set at construction | Emergency controls, private-pool management, admin transfer |
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
- `get_exit_cooldown_secs`, `get_announced_fees`
- `is_value_guard_enabled`
- `get_user_pnl(user) -> UserPnLReport`

## execute_op Dispatch

```
trader/manager calls: vault.execute_op(caller, guard, "swap", [from_asset, to_asset, amount])
vault checks:         guard ∈ ActiveGuards
                      "swap" ∈ AuthorizedOps(guard)
                      OpsPaused == false
vault builds args:    [vault_addr, from_asset, to_asset, amount]
vault calls:          guard.swap(vault_addr, from_asset, to_asset, amount)
```

The vault always injects its own address as the first argument — the trader cannot substitute a different source.

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
8. Exit cooldown blocks withdrawals until `last_deposit_ts + cooldown_secs`.
9. Fee increases require announce → timelock (86 400 s) → commit.
10. Same-ledger operation/value guard detects suspicious sequence/value drift.

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
