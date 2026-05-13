# Vault Data — Developer Reference

This document describes every on-chain read you can perform against a vault contract, what the return value means, and how to convert raw integers to human-readable amounts.

All examples use the Beta vault on testnet:
```
VAULT=CBR6GCCBB73UMQOHUXWFS4L5BL7X7GUR3EPM5YF4OWU6LRNBTARPQWTR
```

---

## Decimal conventions

| Value type | Scale | Example raw | Human |
|---|---|---|---|
| Token balance / NAV | 1e7 (7 decimals) | `10000000` | 1.0 USDC |
| Share price | 1e7 (`PRICE_PRECISION`) | `10000000` | 1.0 USDC per share |
| Fee / bps | basis points | `200` | 2% |

**Formula to convert any raw i128 to human:**
```
human = raw / 10_000_000
```

---

## 1. Share token balance of a user

**What it gives you:** how many vault shares a specific address holds.

**Contract:** Share token (not the vault itself)  
**Function:** `balance(id: Address) → i128`

```bash
stellar contract invoke \
  --network testnet \
  --source-account <any-key> \
  --id <SHARE_TOKEN_ADDRESS> \
  -- balance \
  --id <USER_G_ADDRESS>
```

**Return:** raw share balance in 1e7 scale.

**Convert to base asset (USDC):**
```
user_value_usdc = shares × share_price / 10_000_000
```

Where `share_price` comes from `get_share_price` (section 3).

**Example:**
```
shares        = 1_000_000_000   (100 shares)
share_price   = 10_050_000      (1.005 USDC/share)
user_value    = 1_000_000_000 × 10_050_000 / 10_000_000
              = 1_005_000_000   (100.5 USDC)
```

---

## 2. Total share supply

**What it gives you:** total number of shares currently in circulation across all holders.

**Contract:** Share token  
**Function:** `total_supply() → i128`

```bash
stellar contract invoke \
  --network testnet \
  --source-account <any-key> \
  --id <SHARE_TOKEN_ADDRESS> \
  -- total_supply
```

**Return:** raw total supply in 1e7 scale.  
**Convert:** divide by `10_000_000` → number of shares in human units.

---

## 3. Share token price

**What it gives you:** current price of one share expressed in the base asset (USDC), at 1e7 scale.

**Contract:** Vault  
**Function:** `get_share_price() → i128`

```bash
stellar contract invoke \
  --network testnet \
  --source-account <any-key> \
  --id <VAULT_ADDRESS> \
  -- get_share_price
```

**Return:** `NAV × 10_000_000 / total_supply`  
At launch the price is exactly `10_000_000` (1.00 USDC). As yield accrues the price rises.

**Convert to human:**
```
price_usdc = raw / 10_000_000
```

**Example:**
```
raw = 10_050_000  →  1.005 USDC per share
```

---

## 4. NAV (Net Asset Value)

**What it gives you:** total value of everything the vault controls — idle token balances plus all active strategy positions — expressed in the base asset (USDC).

**Contract:** Vault  
**Function:** `get_nav() → i128`

```bash
stellar contract invoke \
  --network testnet \
  --source-account <any-key> \
  --id <VAULT_ADDRESS> \
  -- get_nav
```

**How it is computed internally:**
```
NAV = Σ (idle_balance[asset] × oracle_price[asset] / 1e7)
    + Σ strategy.get_total_value(vault)
```

All strategy values are already denominated in USDC by the strategy contract before being summed.

**Convert to human USDC:**
```
nav_usdc = raw / 10_000_000
```

**Example:**
```
raw = 4_650_000_841  →  465.00 USDC
```

---

## 5. Investor PnL

**What it gives you:** full profit-and-loss breakdown for a single user — cost basis, current value, unrealized PnL, realized PnL, and total PnL — all in USDC at 1e7 scale.

**Contract:** Vault  
**Function:** `get_user_pnl(user: Address) → UserPnLReport`

```bash
stellar contract invoke \
  --network testnet \
  --source-account <any-key> \
  --id <VAULT_ADDRESS> \
  -- get_user_pnl \
  --user <G...USER_ADDRESS>
```

**Return fields (all i128, 1e7 scale):**

| Field | Meaning |
|---|---|
| `cost_basis` | Total USDC deposited by the user (cumulative, net of withdrawals) |
| `current_value` | `user_shares × share_price / 1e7` — what shares are worth now |
| `unrealized_pnl` | `current_value − cost_basis` |
| `realized_pnl` | USDC profit already crystallized by past withdrawals |
| `total_pnl` | `realized_pnl + unrealized_pnl` |

**Convert each field:**
```
human_usdc = field_raw / 10_000_000
```

**Example:**
```
cost_basis      = 1_000_000_000   (100.00 USDC deposited)
current_value   = 1_050_000_000   (105.00 USDC worth of shares now)
unrealized_pnl  =    50_000_000   (+5.00 USDC gain)
realized_pnl    =    10_000_000   (+1.00 USDC from a prior partial withdrawal)
total_pnl       =    60_000_000   (+6.00 USDC total)
```

---

## 6. Deposit assets

**What it gives you:** the list of assets a user can deposit into the vault. Currently always `[USDC]` for all three vaults.

**Contract:** Vault  
**Function:** `get_deposit_assets() → Vec<Address>`

```bash
stellar contract invoke \
  --network testnet \
  --source-account <any-key> \
  --id <VAULT_ADDRESS> \
  -- get_deposit_assets
```

**Return:** a JSON array of contract addresses, e.g.:
```json
["CD37EBBWP3RY4QRIBP4D7KJGN6GEFXNHF5ZDMXSKSHVTLJMZ3CSIQMP7"]
```

No conversion needed — these are addresses, not amounts.

---

## 7. Portfolio assets

**What it gives you:** all assets the vault tracks for NAV accounting — both idle balances and strategy positions. This is the manager-configured asset universe.

**Contract:** Vault  
**Function:** `get_portfolio_assets() → Vec<Address>`

```bash
stellar contract invoke \
  --network testnet \
  --source-account <any-key> \
  --id <VAULT_ADDRESS> \
  -- get_portfolio_assets
```

**Return:** a JSON array of contract addresses.

| Vault | Portfolio assets |
|---|---|
| Beta | USDC, XLM, PYUSD, EURC, AQUA, USTRY |
| Alpha | USDC, XLM, BTC |
| Gamma | USDC, USTRY |

**Difference from deposit assets:**  
`get_deposit_assets` → what users can put in.  
`get_portfolio_assets` → what the manager can invest in (superset).

---

## 8. Active strategy guards

**What it gives you:** the list of strategy contract addresses that are currently active for this vault (i.e., authorized to hold positions on its behalf).

**Contract:** Vault  
**Function:** `get_active_guards() → Vec<Address>`

```bash
stellar contract invoke \
  --network testnet \
  --source-account <any-key> \
  --id <VAULT_ADDRESS> \
  -- get_active_guards
```

**Return:** JSON array of strategy contract addresses. For Beta and Alpha this returns three addresses (Blend, Soroswap, Phoenix). For Gamma it returns one (Blend).

To query the current value each strategy holds on behalf of the vault, call `get_total_value` on each strategy address:

```bash
stellar contract invoke \
  --network testnet \
  --source-account <any-key> \
  --id <STRATEGY_ADDRESS> \
  -- get_total_value \
  --vault <VAULT_ADDRESS>
```

**Return:** i128 in USDC at 1e7 scale. Convert: `raw / 10_000_000`.

---

## Quick reference cheat sheet

| Data | Contract | Function | Unit |
|---|---|---|---|
| User share balance | Share token | `balance(id)` | 1e7 shares |
| Total shares in circulation | Share token | `total_supply()` | 1e7 shares |
| Share price in USDC | Vault | `get_share_price()` | 1e7 USDC/share |
| Total vault value (NAV) | Vault | `get_nav()` | 1e7 USDC |
| Investor PnL breakdown | Vault | `get_user_pnl(user)` | 1e7 USDC |
| Deposit assets | Vault | `get_deposit_assets()` | `Vec<Address>` |
| Portfolio assets | Vault | `get_portfolio_assets()` | `Vec<Address>` |
| Active strategies | Vault | `get_active_guards()` | `Vec<Address>` |
| Strategy value for vault | Strategy | `get_total_value(vault)` | 1e7 USDC |

**All i128 amounts → human:** divide by `10_000_000`  
**Human amount → raw i128:** multiply by `10_000_000`
