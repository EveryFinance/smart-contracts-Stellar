# Interaction Flows

## 1) Deployment and Initialization

Recommended order:
1. Deploy `share_token`.
2. Deploy `vault`.
3. Initialize `share_token` with temporary admin (`manager`/deployer).
4. Initialize `vault` with `share_token_admin` set to current token admin.
5. During vault initialize, vault takes share-token admin ownership atomically.
6. Deploy and initialize `oracle`.
7. Set oracle on vault.
8. Deploy and initialize `factory`.
9. Register vault in factory via `verify_and_register_vault`.
10. Deploy guards and strategies; set strategy list and guard wiring.

Reference script:
- `scripts/deploy_testnet.sh`

## 2) Deposit Flow

1. User calls `vault.deposit(amount, from)`.
2. Vault checks paused state, amount validity, and private-pool membership (if enabled).
3. Vault collects mgmt/perf fees first.
4. Vault computes shares from pre-deposit NAV/share.
5. Vault mints user shares and manager fee shares.
6. Vault pulls base asset into vault and updates per-user `last_deposit_ts`.

Output:
- minted user shares.

## 3) Withdraw Flow

1. User calls `vault.withdraw(share_amount, from, to)`.
2. Vault enforces exit cooldown (`last_deposit_ts + exit_cooldown_secs`).
3. Vault collects fees first.
3. Vault computes gross/net base amount from share proportion.
4. If vault cash is insufficient, vault auto-unwinds non-LP strategies.
5. Vault burns user shares.
6. Vault transfers net base asset to recipient.

Notes:
- LP strategies are not auto-unwound in the proportional loop.
- if liquidity remains trapped in LP-only exposure, withdrawal can revert until manager unwinds.

## 3.1) Fee-Increase Timelock Flow (Manager)

1. Manager calls `announce_fee_increase(entry, exit, mgmt, perf)`.
2. Vault stores pending fees and `activation_ts = now + 86400`.
3. Before activation, `commit_fee_increase` reverts.
4. After activation, manager calls `commit_fee_increase` to apply and clear pending values.
5. `renounce_fee_increase` cancels pending values.

Direct fee setters remain available for decreases; increases must use the timelock flow.

## 4) Invest / Unwind (Manager)

- `invest` and `invest_lp` move vault capital into strategies.
- `unwind` and `unwind_lp` pull units/liquidity back.
- manager-only, strategy-whitelist enforced.
- optional strategy guard checks are executed.
- post-operation NAV loss guard applies (`max_loss_bps`).

## 5) Spot Trade Execution (Trader)

1. Trader calls `vault.execute_trade(caller, strategy, amount_in, min_out, path)`.
2. Vault enforces trader role and strategy whitelist.
3. Vault requires per-strategy trade guard configured.
4. Vault gets quote from strategy (`quote_exact_in` path).
5. Guard validates policy using quoted values.
6. Strategy executes trade.
7. Vault checks `amount_out >= min_out` and NAV loss guard.
8. If value guard is enabled, same-ledger operation/value checkpoints are enforced.

## 6) Oracle Interaction

- Oracle admin sets prices via `set_price(asset, price)`.
- Each stored price includes `updated_ledger`.
- Reads (`get_price`, `get_prices`) enforce staleness by `max_age_ledgers`.
- `max_age_ledgers = 0` disables staleness checks (allowed by code, discouraged by policy).

## 7) Factory Interaction

- Use `verify_and_register_vault` to ensure target vault is initialized.
- `register_vault` also exists for compatibility but depends on caller-provided manager argument.
