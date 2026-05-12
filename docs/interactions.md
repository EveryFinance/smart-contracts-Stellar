# Interaction Flows

## 1) Deployment and Initialization

Recommended order:

1. Deploy `share_token`.
2. Deploy `vault` with `VaultParams` (includes `admin`, `manager`, `trader`, `is_private`, fees, `factory`).
3. During `__constructor`, vault atomically takes share-token admin ownership.
4. Deploy `ReflectorAdapter` pointing to the Reflector contract address.
5. Deploy `DIAAdapter` pointing to the DIA contract address.
6. Call `dia_adapter.set_asset_key(admin, asset, "ASSET/USD")` for each asset.
7. Deploy `AssetHandler`; call `set_primary_oracle(reflector_adapter)` and `set_fallback_oracle(dia_adapter)`.
8. Register assets in AssetHandler: `add_asset(admin, asset)` for each.
9. Deploy and initialize `factory`.
10. Deploy strategy contracts and call `vault.add_active_guard(manager, strategy)` + `vault.set_authorized_ops(manager, strategy, ops)` for each.
11. Call `vault.add_portfolio_asset` and `vault.add_deposit_asset` for each asset.

Reference script: `scripts/deploy_testnet.sh`

---

## 2) Deposit Flow

1. User calls `vault.deposit(amount, from, asset, min_shares_out)`.
2. Vault checks: `DepositsPaused` (admin-controlled), `DepositCap`, asset ∈ `DepositAssets`, private-pool membership (if enabled), amount > 0.
3. Vault collects mgmt/perf fees.
4. Vault prices the deposit asset via oracle and computes shares from pre-deposit NAV/share.
5. Vault mints user shares and treasury fee shares.
6. Vault pulls asset from user into vault; updates `LastDepositTs`.

By default, the minted shares cannot be transferred. This keeps
`LastDepositTs(user)` and `UserPosition(user)` aligned with the account that
owns and later withdraws the shares.

---

## 3) Withdraw Flow

1. User calls `vault.withdraw(share_amount, from, to, min_base_out)`.
2. Vault enforces exit cooldown (`LastDepositTs + exit_cooldown_secs`).
3. Vault checks `DepositsPaused` (same flag as deposits).
4. Vault collects fees.
5. Vault burns user shares proportionally to `share_amount / total_supply`.
6. For each asset ∈ `PortfolioAssets`: transfers `floor(balance × fraction)` to `to`.
7. For each guard ∈ `ActiveGuards`: calls `guard.withdraw_fraction(vault, num, denom, to)`.
8. Checks `min_base_out` slippage guard.

The cooldown is a hard account-level control only while share transfers are
disabled. If a vault admin enables share transfers, withdrawals still check the
`from` account's cooldown, but transferred shares may carry economic exposure
without carrying the original depositor's cooldown or cost-basis history.

---

## 3.1) Share Transfer Mode (Admin)

Default:
1. `share_transfers_enabled() == false`.
2. `exit_cooldown_is_hard_control() == true`.
3. `pnl_tracking_is_accurate() == true`.
4. `share_token.transfer` and `share_token.transfer_from` revert with
   `TransfersDisabled`.

Optional transfer mode:
1. Vault admin calls `vault.set_share_transfers_enabled(admin, true)`.
2. Share transfers become possible.
3. `exit_cooldown_is_hard_control() == false`.
4. `pnl_tracking_is_accurate() == false`.

This mode is suitable only when the vault accepts cooldown as same-address
friction and PnL as approximate / informational reporting.

---

## 3.2) Fee-Increase Timelock Flow (Manager)

1. Manager calls `announce_fee_increase(entry, exit, mgmt, perf)`.
2. Vault stores pending fees and `activation_ts = now + 86400`.
3. Before activation, `commit_fee_increase` reverts with `FeeIncreaseDelayActive`.
4. After activation, manager calls `commit_fee_increase` to apply and clear pending values.
5. `renounce_fee_increase` cancels at any time.

Direct fee setters (`set_entry_fee_bps`, etc.) are decrease-only.

---

## 4) execute_op (Manager / Trader)

```
caller calls:  vault.execute_op(caller, guard, "swap", [from_asset, to_asset, amount, min_out])
vault checks:  OpsPaused == false
               caller ∈ {manager, trader}
               guard ∈ ActiveGuards
               "swap" ∈ AuthorizedOps(guard)
vault builds:  full_args = [vault_addr, from_asset, to_asset, amount, min_out]
vault calls:   guard.swap(vault_addr, from_asset, to_asset, amount, min_out)
vault asserts: NAV after ≥ NAV before × (1 − max_loss_bps / 10_000)
```

The vault always injects its own address as the first argument — traders cannot route funds to arbitrary sources.

---

## 5) Oracle Price Resolution

```
AssetHandler::get_price(asset)
  1. Per-asset oracle override (if set via set_asset_oracle)
     → try_invoke_contract; catches reverts gracefully
  2. Primary oracle (ReflectorAdapter)
     → try_invoke_contract; catches reverts gracefully
  3. Fallback oracle (DIAAdapter)
     → invoke_contract; panics if also fails
```

ReflectorAdapter calls Reflector's `lastprice(Asset::Stellar(asset))`.  
DIAAdapter calls DIA's `read_oracle_value(key)` using the admin-registered `asset → "PAIR/USD"` mapping.

---

## 6) Emergency Controls (Admin)

```
admin calls: vault.pause_deposits(admin)
             → blocks deposit() and withdraw()

admin calls: vault.pause_operations(admin)
             → blocks execute_op()
```

Each pause is independently toggleable. Admin can pause operations while leaving withdrawals open, or vice versa.

---

## 7) Private-Pool Mode (Admin)

```
admin calls: vault.set_private_pool(admin, true)
             → only admin, manager, and allowlisted members can deposit

admin calls: vault.add_member(admin, user_address)
             → adds user to allowlist

admin calls: vault.remove_member(admin, user_address)
             → removes user from allowlist
```

Private-pool mode can be configured at construction (`is_private: true` in `VaultParams`) or toggled later.

---

## 8) Factory Interaction

- `verify_and_register_vault(vault)` — validates vault is initialized and registers it.
- `add_authorized_asset(asset)` / `add_authorized_guard(guard)` — add to global whitelist.
- `create_vault(params, seed_amount)` — atomically deploys and seeds a vault (prevents first-depositor attack).
