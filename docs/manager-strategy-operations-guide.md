# Manager Strategy Operations Guide

This guide explains how the manager interacts with the three deployed
strategy contracts — Blend (lending), Soroswap LP (AMM liquidity), and
Phoenix LP (AMM liquidity) — through the vault's `execute_op` gateway,
and how to use `scripts/manager_execute_op.py` to submit those operations
from the command line.

---

## Table of Contents

1. [Architecture overview](#1-architecture-overview)
2. [The `execute_op` gateway](#2-the-execute_op-gateway)
3. [Blend Strategy — lending on Blend V2](#3-blend-strategy--lending-on-blend-v2)
4. [Soroswap LP Strategy — AMM liquidity on Soroswap](#4-soroswap-lp-strategy--amm-liquidity-on-soroswap)
5. [Phoenix LP Strategy — AMM liquidity on Phoenix](#5-phoenix-lp-strategy--amm-liquidity-on-phoenix)
6. [Script reference](#6-script-reference)
7. [Mainnet contract addresses](#7-mainnet-contract-addresses)
8. [Troubleshooting](#8-troubleshooting)

---

## 1. Architecture overview

Each vault holds assets (USDC, XLM, BTC, …) and delegates yield generation
to one or more **strategy contracts** (also called guards). The three strategy
types are:

| Strategy | Protocol | What it does |
|----------|----------|--------------|
| Blend | Blend V2 (lending pool) | Supplies tokens as collateral/supply to earn lending interest |
| Soroswap LP | Soroswap (AMM) | Provides liquidity to token pairs, earns swap fees |
| Phoenix LP | Phoenix (AMM) | Provides liquidity to token pairs, earns swap fees |

The vault never calls strategy functions directly on behalf of a user.
Instead it exposes a single guarded entry point — `execute_op` — that the
**manager** (or trader) uses to dispatch authorized operations.

When a user withdraws shares, the vault calls `withdraw_fraction` directly
on every active strategy to proportionally unwind positions. That path
bypasses `execute_op` and is not manager-callable.

```
User               Manager / Trader
  │                      │
  │  deposit / withdraw  │  execute_op(guard, fn, args)
  ▼                      ▼
┌─────────────────────────────────────┐
│              Vault                  │
│  ┌─────────────────────────────┐   │
│  │  execute_op checks:         │   │
│  │  • caller = manager/trader  │   │
│  │  • guard ∈ ActiveGuards     │   │
│  │  • fn ∈ AuthorizedOps(guard)│   │
│  │  • NAV guard (max_loss_bps) │   │
│  └────────────┬────────────────┘   │
└───────────────┼─────────────────────┘
                │ guard.fn(vault, args…)
       ┌────────┼─────────────────────┐
       ▼        ▼                     ▼
  BlendStrategy  SoroswapLpStrategy  PhoenixLpStrategy
       │              │                    │
  Blend V2 pool  Soroswap router     Phoenix pool
```

---

## 2. The `execute_op` gateway

### Signature

```
vault.execute_op(caller, guard, fn_name, args) → Val
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `caller` | `Address` | Manager or trader account (must sign) |
| `guard` | `Address` | Strategy contract address (must be in `ActiveGuards`) |
| `fn_name` | `Symbol` | Name of the strategy function to call |
| `args` | `Vec<Val>` | Arguments forwarded to the strategy **after** the vault address |

### What the vault checks before dispatching

1. `caller.require_auth()` — the manager/trader must sign the transaction
2. `guard` must be in the vault's `ActiveGuards` list
3. `guard` must be authorized by the factory's global registry
4. `fn_name` must be in `AuthorizedOps(guard)` — the per-guard allowlist
5. If `max_loss_bps > 0`, the vault snapshots NAV before dispatch and reverts
   if the post-operation NAV drop exceeds the configured threshold

### Vault address injection

The vault automatically prepends its own address as the first argument before
calling the strategy. So if `args = [pool, asset, amount]`, the strategy
receives `(vault, pool, asset, amount)`. You only provide the strategy-specific
arguments in `--args`.

### Reserved functions (never callable via `execute_op`)

`withdraw_fraction` is reserved — it is called directly by the vault during
user share redemptions, not by the manager. Attempting to call it via
`execute_op` will be rejected with `TradeGuardRejected`.

### Admin operations for guard management

These are admin-only and not dispatched through `execute_op`:

| Vault function | What it does |
|----------------|-------------|
| `add_active_guard(caller, guard)` | Whitelist a strategy contract |
| `remove_active_guard(caller, guard)` | Remove a strategy from the whitelist |
| `set_authorized_ops(caller, guard, ops)` | Set the list of functions the manager can call on a guard |
| `set_max_loss_bps(caller, bps)` | Maximum NAV drop allowed per `execute_op` call (0 = disabled) |

---

## 3. Blend Strategy — lending on Blend V2

### What it does

The Blend strategy lends vault assets into a **Blend V2 lending pool** to earn
interest. One strategy contract instance manages all Blend positions for its
vault across any number of pools and assets. Each `(pool, asset)` pair is
tracked as a separate position.

Blend V2 uses **b-tokens** internally. When you supply 1,000,000 USDC, you
receive slightly fewer b-tokens (due to utilization rounding). On withdrawal,
the redeemable amount may be 1–2 stroop less than deposited. The strategy
clamps the withdrawal to `min(requested, position)` to handle this gracefully.

### Manager-callable functions

#### `supply` — lend tokens into the Blend pool

Sends `amount` of `asset` from the vault to the Blend `pool`.

```
args: [pool, asset, amount]
```

| Arg | Type | Description |
|-----|------|-------------|
| `pool` | `address` | Blend V2 pool contract address |
| `asset` | `address` | Token to lend (e.g. USDC) |
| `amount` | `i128` | Amount in token stroops |

#### `withdraw_from_lending` — redeem tokens from the Blend pool

Withdraws `amount` of `asset` from the Blend `pool` back to the vault.
Automatically clamped to the current position if rounding causes a 1–2
stroop shortfall.

```
args: [pool, asset, amount]
```

| Arg | Type | Description |
|-----|------|-------------|
| `pool` | `address` | Blend V2 pool contract address |
| `asset` | `address` | Token to redeem |
| `amount` | `i128` | Amount in token stroops |

### Not manager-callable

| Function | Who calls it |
|----------|-------------|
| `withdraw_fraction(vault, fraction_bps)` | Vault only — during user share redemption |

### Mainnet Blend strategy addresses (deployed 2026-05-18)

| Vault | Strategy address |
|-------|-----------------|
| Alpha | `CBD7QEXZP2RVIEFD4OUWRDAXKB2BM4GKUME3EZQQEBWAABL3IEGXPGKI` |
| Beta  | `CBO5XSLPO4DCJJSWWWCPHZ6JDFKPFBPMQDLJWUCJJ3PEWRYO7V6JOJ7V` |
| Gamma | `CDMPATIFU2P7JRRAQZZ3655IZSNON62V3EUZK2UZH33C7ACQF6EQ2HYM` |

**Blend V2 pool:** `CAJJZSGMMM3PD7N33TAPHGBUGTB43OC73HVIK2L2G6BNGGGYOSSYBXBD`

### Examples

**Supply 1 USDC to Blend — Alpha vault:**

```bash
python scripts/manager_execute_op.py \
    --vault  CAHHS2EFYMKKBYP3LTUFPDS2JSY4EPQLUDEUOFI5I7P7MOIBSJH5AUKQ \
    --guard  CBD7QEXZP2RVIEFD4OUWRDAXKB2BM4GKUME3EZQQEBWAABL3IEGXPGKI \
    --fn     supply \
    --args   '[{"address":"CAJJZSGMMM3PD7N33TAPHGBUGTB43OC73HVIK2L2G6BNGGGYOSSYBXBD"},
               {"address":"CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"},
               {"i128":10000000}]'
```

**Withdraw 1 USDC from Blend — Beta vault:**

```bash
python scripts/manager_execute_op.py \
    --vault  CDYB5FK54OXV36AQ2TBK6V2K6KYN6RXNIID6HMCYUP7EJ4BEV7BAVIYB \
    --guard  CBO5XSLPO4DCJJSWWWCPHZ6JDFKPFBPMQDLJWUCJJ3PEWRYO7V6JOJ7V \
    --fn     withdraw_from_lending \
    --args   '[{"address":"CAJJZSGMMM3PD7N33TAPHGBUGTB43OC73HVIK2L2G6BNGGGYOSSYBXBD"},
               {"address":"CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"},
               {"i128":10000000}]'
```

---

## 4. Soroswap LP Strategy — AMM liquidity on Soroswap

### What it does

The Soroswap LP strategy provides liquidity to **Soroswap AMM pairs** and
earns swap fees. One strategy contract manages all Soroswap positions for its
vault. Positions are keyed by **LP token address** (each Soroswap pair has a
unique LP token). Token ordering within the pair is detected automatically on
the first `add_liquidity` call and cached.

When liquidity is added, any residual tokens not consumed by the router (due to
ratio mismatch) are returned directly to the vault.

### Manager-callable functions

#### `add_liquidity` — provide liquidity to a Soroswap pair

Deposits `amount_a` and `amount_b` into the pair identified by `lp_token`.

```
args: [lp_token, asset_a, asset_b, amount_a, amount_b, min_a, min_b]
```

| Arg | Type | Description |
|-----|------|-------------|
| `lp_token` | `address` | LP token address (identifies the pair) |
| `asset_a` | `address` | First token of the pair |
| `asset_b` | `address` | Second token of the pair |
| `amount_a` | `i128` | Desired amount of asset_a in stroops |
| `amount_b` | `i128` | Desired amount of asset_b in stroops |
| `min_a` | `i128` | Minimum asset_a to deposit (slippage guard, 0 = no minimum) |
| `min_b` | `i128` | Minimum asset_b to deposit (slippage guard, 0 = no minimum) |

#### `remove_liquidity` — withdraw liquidity from a Soroswap pair

Burns `lp_amount` of LP tokens and receives the underlying assets directly in the vault.

```
args: [lp_token, asset_a, asset_b, lp_amount, min_a, min_b]
```

| Arg | Type | Description |
|-----|------|-------------|
| `lp_token` | `address` | LP token address |
| `asset_a` | `address` | First token of the pair |
| `asset_b` | `address` | Second token of the pair |
| `lp_amount` | `i128` | LP tokens to burn |
| `min_a` | `i128` | Minimum asset_a to receive (slippage guard) |
| `min_b` | `i128` | Minimum asset_b to receive (slippage guard) |

#### `swap` — swap one asset for another via Soroswap router

Swaps `amount_in` of `from_asset` for `to_asset`. Output goes directly to the vault.

```
args: [from_asset, to_asset, amount_in, min_out]
```

| Arg | Type | Description |
|-----|------|-------------|
| `from_asset` | `address` | Token to sell |
| `to_asset` | `address` | Token to receive |
| `amount_in` | `i128` | Amount to sell in stroops |
| `min_out` | `i128` | Minimum amount to receive (slippage guard) |

### Not manager-callable

| Function | Who calls it |
|----------|-------------|
| `withdraw_fraction(vault, fraction_bps)` | Vault only — during user share redemption |

### Mainnet Soroswap LP strategy addresses

| Vault | Strategy address |
|-------|-----------------|
| Alpha | `CBWQQKHFLNFXAHVMXBOFPZPPVX77JD7C7DMJ5ZCGBMRVWG7H34W3CVFU` |
| Beta  | `CAAPFJFF6IGWVZDYQC3OCXJ4ZLVTO3VVJYSVXBDVNSXSDC5U2LTKEK6R` |

### Examples

**Add liquidity USDC/XLM — Alpha vault:**

```bash
python scripts/manager_execute_op.py \
    --vault  CAHHS2EFYMKKBYP3LTUFPDS2JSY4EPQLUDEUOFI5I7P7MOIBSJH5AUKQ \
    --guard  CBWQQKHFLNFXAHVMXBOFPZPPVX77JD7C7DMJ5ZCGBMRVWG7H34W3CVFU \
    --fn     add_liquidity \
    --args   '[{"address":"<LP_TOKEN>"},
               {"address":"CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"},
               {"address":"CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA"},
               {"i128":10000000},
               {"i128":10000000},
               {"i128":0},
               {"i128":0}]'
```

**Remove liquidity — Alpha vault:**

```bash
python scripts/manager_execute_op.py \
    --vault  CAHHS2EFYMKKBYP3LTUFPDS2JSY4EPQLUDEUOFI5I7P7MOIBSJH5AUKQ \
    --guard  CBWQQKHFLNFXAHVMXBOFPZPPVX77JD7C7DMJ5ZCGBMRVWG7H34W3CVFU \
    --fn     remove_liquidity \
    --args   '[{"address":"<LP_TOKEN>"},
               {"address":"CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"},
               {"address":"CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA"},
               {"i128":5000000},
               {"i128":0},
               {"i128":0}]'
```

**Swap USDC for XLM — Alpha vault:**

```bash
python scripts/manager_execute_op.py \
    --vault  CAHHS2EFYMKKBYP3LTUFPDS2JSY4EPQLUDEUOFI5I7P7MOIBSJH5AUKQ \
    --guard  CBWQQKHFLNFXAHVMXBOFPZPPVX77JD7C7DMJ5ZCGBMRVWG7H34W3CVFU \
    --fn     swap \
    --args   '[{"address":"CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"},
               {"address":"CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA"},
               {"i128":10000000},
               {"i128":0}]'
```

---

## 5. Phoenix LP Strategy — AMM liquidity on Phoenix

### What it does

The Phoenix LP strategy provides liquidity to **Phoenix AMM pools** and earns
swap fees. One strategy contract manages all Phoenix positions for its vault.
Positions are keyed by **pool address** (each Phoenix pool manages its own
share token). The share token address and token ordering are cached on the
first `add_liquidity` call.

When liquidity is added, any residual tokens not consumed by the pool (due to
ratio mismatch) are returned directly to the vault.

Unlike Soroswap (which identifies pairs by LP token), Phoenix identifies pools
directly by pool contract address.

### Manager-callable functions

#### `add_liquidity` — provide liquidity to a Phoenix pool

Deposits `amount_a` and `amount_b` into the `pool`.

```
args: [pool, asset_a, asset_b, amount_a, amount_b, min_a, min_b]
```

| Arg | Type | Description |
|-----|------|-------------|
| `pool` | `address` | Phoenix pool contract address |
| `asset_a` | `address` | First token of the pool |
| `asset_b` | `address` | Second token of the pool |
| `amount_a` | `i128` | Desired amount of asset_a in stroops |
| `amount_b` | `i128` | Desired amount of asset_b in stroops |
| `min_a` | `i128` | Minimum asset_a to deposit (slippage guard, 0 = no minimum) |
| `min_b` | `i128` | Minimum asset_b to deposit (slippage guard, 0 = no minimum) |

#### `remove_liquidity` — withdraw liquidity from a Phoenix pool

Burns `share_amount` of LP shares and receives the underlying assets in the vault.

```
args: [pool, asset_a, asset_b, share_amount, min_a, min_b]
```

| Arg | Type | Description |
|-----|------|-------------|
| `pool` | `address` | Phoenix pool contract address |
| `asset_a` | `address` | First token of the pool |
| `asset_b` | `address` | Second token of the pool |
| `share_amount` | `i128` | LP shares to burn |
| `min_a` | `i128` | Minimum asset_a to receive (slippage guard) |
| `min_b` | `i128` | Minimum asset_b to receive (slippage guard) |

#### `swap` — swap one asset for another within a Phoenix pool

Swaps `amount_in` of `asset_in` for `asset_out` within the given `pool`.
The strategy automatically derives `sell_a` from the asset addresses. Output
goes directly to the vault.

```
args: [pool, asset_in, asset_out, amount_in, min_out]
```

| Arg | Type | Description |
|-----|------|-------------|
| `pool` | `address` | Phoenix pool contract address |
| `asset_in` | `address` | Token to sell |
| `asset_out` | `address` | Token to receive |
| `amount_in` | `i128` | Amount to sell in stroops |
| `min_out` | `i128` | Minimum amount to receive (slippage guard) |

### Not manager-callable

| Function | Who calls it |
|----------|-------------|
| `withdraw_fraction(vault, fraction_bps)` | Vault only — during user share redemption |

### Mainnet Phoenix LP strategy addresses

| Vault | Strategy address |
|-------|-----------------|
| Alpha | `CCYMGI5VYZ625TNLLLBMX2IIQHRXXXUFUKMJCRLVPRG3IQDYTWV4SUI7` |
| Beta  | `CCB6KZZ6TFAQCIWUO3C7IHTA554WKP4XBBB3SFR5APP4BE5JTROEPGTN` |

### Examples

**Add liquidity USDC/XLM — Alpha vault:**

```bash
python scripts/manager_execute_op.py \
    --vault  CAHHS2EFYMKKBYP3LTUFPDS2JSY4EPQLUDEUOFI5I7P7MOIBSJH5AUKQ \
    --guard  CCYMGI5VYZ625TNLLLBMX2IIQHRXXXUFUKMJCRLVPRG3IQDYTWV4SUI7 \
    --fn     add_liquidity \
    --args   '[{"address":"<PHOENIX_POOL>"},
               {"address":"CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"},
               {"address":"CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA"},
               {"i128":10000000},
               {"i128":10000000},
               {"i128":0},
               {"i128":0}]'
```

**Remove liquidity — Alpha vault:**

```bash
python scripts/manager_execute_op.py \
    --vault  CAHHS2EFYMKKBYP3LTUFPDS2JSY4EPQLUDEUOFI5I7P7MOIBSJH5AUKQ \
    --guard  CCYMGI5VYZ625TNLLLBMX2IIQHRXXXUFUKMJCRLVPRG3IQDYTWV4SUI7 \
    --fn     remove_liquidity \
    --args   '[{"address":"<PHOENIX_POOL>"},
               {"address":"CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"},
               {"address":"CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA"},
               {"i128":5000000},
               {"i128":0},
               {"i128":0}]'
```

**Swap USDC for XLM — Alpha vault:**

```bash
python scripts/manager_execute_op.py \
    --vault  CAHHS2EFYMKKBYP3LTUFPDS2JSY4EPQLUDEUOFI5I7P7MOIBSJH5AUKQ \
    --guard  CCYMGI5VYZ625TNLLLBMX2IIQHRXXXUFUKMJCRLVPRG3IQDYTWV4SUI7 \
    --fn     swap \
    --args   '[{"address":"<PHOENIX_POOL>"},
               {"address":"CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"},
               {"address":"CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA"},
               {"i128":10000000},
               {"i128":0}]'
```

---

## 6. Script reference

### Prerequisites

```bash
pip install stellar-sdk
export MANAGER_SECRET=S...your_manager_secret...
```

### Synopsis

```
python scripts/manager_execute_op.py \
    --vault   <VAULT_CONTRACT_ID>    \
    --guard   <STRATEGY_CONTRACT_ID> \
    --fn      <FUNCTION_NAME>        \
    --args    '<JSON_ARRAY>'         \
    [--network mainnet|testnet]      \
    [--fee    <STROOPS>]             \
    [--dry-run]
```

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--vault` | ✓ | — | Vault contract address (`C...`) |
| `--guard` | ✓ | — | Whitelisted strategy address (`C...`) |
| `--fn` | ✓ | — | Strategy function name |
| `--args` | ✓ | — | JSON array of ScVal-encoded arguments |
| `--network` | | `mainnet` | `mainnet` or `testnet` |
| `--fee` | | `1000000` | Inclusion fee in stroops (0.1 XLM) |
| `--secret` | | `$MANAGER_SECRET` | Manager secret key |
| `--dry-run` | | off | Simulate only, do not submit |

### `--args` encoding

Each element of the JSON array maps to one argument passed to the strategy.
The vault address is always prepended automatically — do not include it.

| JSON form | Soroban type |
|-----------|-------------|
| `{"address": "C..."}` | Contract address |
| `{"address": "G..."}` | Account address |
| `{"i128": <int>}` | Signed 128-bit integer |
| `{"i128": {"lo": N, "hi": M}}` | i128 split into two 64-bit halves |
| `{"u32": <int>}` | Unsigned 32-bit integer |
| `{"u64": <int>}` | Unsigned 64-bit integer |
| `{"symbol": "..."}` | Symbol |
| `{"string": "..."}` | String |
| `{"bool": true/false}` | Boolean |
| `{"void": null}` | Void |

### Dry-run example

```bash
python scripts/manager_execute_op.py \
    --vault  CAHHS2EFYMKKBYP3LTUFPDS2JSY4EPQLUDEUOFI5I7P7MOIBSJH5AUKQ \
    --guard  CBD7QEXZP2RVIEFD4OUWRDAXKB2BM4GKUME3EZQQEBWAABL3IEGXPGKI \
    --fn     supply \
    --args   '[{"address":"CAJJZSGMMM3PD7N33TAPHGBUGTB43OC73HVIK2L2G6BNGGGYOSSYBXBD"},
               {"address":"CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"},
               {"i128":10000000}]' \
    --dry-run
```

### Script output

```
Network : mainnet
Manager : GCDXIPE5...
Vault   : CAHHS2EF...
Guard   : CBD7QEXZ...
Fn      : supply
Args    : [...]

Simulating...
  CPU instructions : 8_234_910
  Memory bytes     : 342_100
  Min resource fee : 51423 stroops

Submitting...
  Tx hash : c296509f...
  Explorer: https://stellar.expert/explorer/public/tx/c296509f...
  Waiting for confirmation .... ✓
  Status  : SUCCESS
```

---

## 7. Mainnet contract addresses

### Vaults

| Name | Address | Portfolio |
|------|---------|-----------|
| Alpha | `CAHHS2EFYMKKBYP3LTUFPDS2JSY4EPQLUDEUOFI5I7P7MOIBSJH5AUKQ` | USDC / XLM / BTC |
| Beta  | `CDYB5FK54OXV36AQ2TBK6V2K6KYN6RXNIID6HMCYUP7EJ4BEV7BAVIYB` | USDC / XLM / PYUSD / EURC / AQUA |
| Gamma | `CCHJFS4OEKTLJLLL6OQXFIS2ECTJRA6WMUNXVS7MD6DKIIB6RTXNBZRW` | USDC only |

### Strategy contracts

| Vault | Blend | Soroswap LP | Phoenix LP |
|-------|-------|-------------|------------|
| Alpha | `CBD7QEXZP2...IEGXPGKI` | `CBWQQKHFLN...W3CVFU` | `CCYMGI5VYZ...SUI7` |
| Beta  | `CBO5XSLPO4...JOJ7V`    | `CAAPFJFF6I...EK6R`  | `CCB6KZZ6TF...PGTN` |
| Gamma | `CDMPATIFU2...HYM`      | — | — |

Full addresses:

**Blend strategies (deployed 2026-05-18):**

| Vault | Address |
|-------|---------|
| Alpha | `CBD7QEXZP2RVIEFD4OUWRDAXKB2BM4GKUME3EZQQEBWAABL3IEGXPGKI` |
| Beta  | `CBO5XSLPO4DCJJSWWWCPHZ6JDFKPFBPMQDLJWUCJJ3PEWRYO7V6JOJ7V` |
| Gamma | `CDMPATIFU2P7JRRAQZZ3655IZSNON62V3EUZK2UZH33C7ACQF6EQ2HYM` |

**Soroswap LP strategies:**

| Vault | Address |
|-------|---------|
| Alpha | `CBWQQKHFLNFXAHVMXBOFPZPPVX77JD7C7DMJ5ZCGBMRVWG7H34W3CVFU` |
| Beta  | `CAAPFJFF6IGWVZDYQC3OCXJ4ZLVTO3VVJYSVXBDVNSXSDC5U2LTKEK6R` |

**Phoenix LP strategies:**

| Vault | Address |
|-------|---------|
| Alpha | `CCYMGI5VYZ625TNLLLBMX2IIQHRXXXUFUKMJCRLVPRG3IQDYTWV4SUI7` |
| Beta  | `CCB6KZZ6TFAQCIWUO3C7IHTA554WKP4XBBB3SFR5APP4BE5JTROEPGTN` |

### Key assets and protocol contracts

| Name | Address |
|------|---------|
| USDC | `CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75` |
| XLM  | `CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA` |
| BTC  | `CAO7DDJNGMOYQPRYDY5JVZ5YEK4UQBSMGLAEWRCUOTRMDSBMGWSAATDZ` |
| EURC | `CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV` |
| AQUA | `CAUIKL3IYGMERDRUN6YSCLWVAKIFG5Q4YJHUKM4S4NJZQIA3BAS6OJPK` |
| PYUSD | `CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2` |
| Blend V2 pool | `CAJJZSGMMM3PD7N33TAPHGBUGTB43OC73HVIK2L2G6BNGGGYOSSYBXBD` |
| AssetHandler | `CAOP3N7S3BDA53AELYJA7NH3QDMLS6BFUMGNKWG2XE2ESYKBR5XUSDM5` |
| Fixed-price Oracle (USDC) | `CCLT42BFIS6FX6V7KYDJV7Y4G2JAY7KKA65NABN7FBXNEQBJHW4PQTZJ` |

---

## 8. Troubleshooting

| Error | Cause | Fix |
|-------|-------|-----|
| `StrategyNotWhitelisted` | Strategy not in vault's `ActiveGuards` | Admin calls `add_active_guard(caller, guard)` |
| `TradeGuardRejected` | Function not in `AuthorizedOps(guard)`, or it is a reserved function (`withdraw_fraction`) | Admin calls `set_authorized_ops(caller, guard, ops)` |
| `GuardNotAuthorized` | Strategy not approved in the factory's global registry | Contact factory admin |
| `NotTrader` / `NotManager` | Signing key is not the registered manager or trader | Use the correct `MANAGER_SECRET` |
| `OperationsPaused` | Manager operations are paused by admin | Admin calls `unpause_operations` |
| `TvlGuardTripped` | NAV drop after the operation exceeded `max_loss_bps` | Operation reverted — reduce trade size or adjust `max_loss_bps` |
| `ResourceLimitExceeded` | Instruction budget underestimated during simulation | Pass `--fee 5000000` to pad the budget |
| `PriceNotAvailable` | USDC fixed-price oracle entry is stale | Call `set_price` on `CCLT42BF...` to refresh the timestamp |
| `InsufficientPosition` | Withdrawal amount exceeds current Blend position | Reduce the amount or use the exact position size |
