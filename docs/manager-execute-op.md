# Manager Execute-Op Script

`scripts/manager_execute_op.py` lets the manager sign and submit any
`vault.execute_op` call from the command line, targeting any whitelisted
guard (strategy) contract on any vault.

---

## Prerequisites

```bash
pip install stellar-sdk
```

Set the manager secret key as an environment variable (keep it out of
shell history):

```bash
export MANAGER_SECRET=S...your_manager_secret...
```

Or pass it inline with `--secret S...`.

---

## Synopsis

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
| `--guard` | ✓ | — | Whitelisted strategy/guard address (`C...`) |
| `--fn` | ✓ | — | Guard function name (e.g. `supply`, `withdraw_from_lending`) |
| `--args` | ✓ | — | JSON array of ScVal-encoded arguments (see below) |
| `--network` | | `mainnet` | `mainnet` or `testnet` |
| `--fee` | | `1000000` | Inclusion fee in stroops (0.1 XLM) |
| `--secret` | | `$MANAGER_SECRET` | Manager secret key — prefer the env var |
| `--dry-run` | | off | Simulate only, do not submit |

---

## How `execute_op` works

The vault's `execute_op` function is the single entry point for all manager
operations on strategies:

```
vault.execute_op(caller, guard, fn_name, args)
  │
  ├─ verifies caller = registered manager (or trader)
  ├─ verifies guard is in ActiveGuards
  ├─ verifies fn_name is in AuthorizedOps(guard)
  ├─ snapshots NAV before dispatch
  ├─ calls guard.fn_name(vault, args...)   ← vault address injected first
  └─ snapshots NAV after, reverts if drop > max_loss_bps
```

The `--args` you pass are forwarded **after** the vault address, which the
vault injects automatically. So you only need to supply the strategy-specific
arguments.

---

## `--args` format

The `--args` value is a JSON array where each element represents one
argument to the guard function. Supported types:

| JSON form | Soroban type | Example |
|-----------|-------------|---------|
| `{"address": "C..."}` | Contract address | `{"address":"CAJJ..."}` |
| `{"address": "G..."}` | Account address | `{"address":"GDZN..."}` |
| `{"i128": <int>}` | Signed 128-bit int | `{"i128":1000000}` |
| `{"i128": {"lo": N, "hi": M}}` | i128 split (lo/hi 64-bit) | `{"i128":{"lo":1000000,"hi":0}}` |
| `{"u32": <int>}` | Unsigned 32-bit int | `{"u32":1}` |
| `{"u64": <int>}` | Unsigned 64-bit int | `{"u64":100}` |
| `{"symbol": "..."}` | Symbol | `{"symbol":"supply"}` |
| `{"string": "..."}` | String | `{"string":"hello"}` |
| `{"bool": true/false}` | Boolean | `{"bool":true}` |
| `{"void": null}` | Void | `{"void":null}` |

---

## Mainnet contract addresses (2026-05-14 deployment)

### Vaults

| Name | Address |
|------|---------|
| Alpha | `CAHHS2EFYMKKBYP3LTUFPDS2JSY4EPQLUDEUOFI5I7P7MOIBSJH5AUKQ` |
| Beta  | `CDYB5FK54OXV36AQ2TBK6V2K6KYN6RXNIID6HMCYUP7EJ4BEV7BAVIYB` |
| Gamma | `CCHJFS4OEKTLJLLL6OQXFIS2ECTJRA6WMUNXVS7MD6DKIIB6RTXNBZRW` |

### Blend strategies (updated 2026-05-18)

| Vault | Blend strategy |
|-------|---------------|
| Alpha | `CBD7QEXZP2RVIEFD4OUWRDAXKB2BM4GKUME3EZQQEBWAABL3IEGXPGKI` |
| Beta  | `CBO5XSLPO4DCJJSWWWCPHZ6JDFKPFBPMQDLJWUCJJ3PEWRYO7V6JOJ7V` |
| Gamma | `CDMPATIFU2P7JRRAQZZ3655IZSNON62V3EUZK2UZH33C7ACQF6EQ2HYM` |

### Key assets

| Asset | Address |
|-------|---------|
| USDC  | `CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75` |
| XLM   | `CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA` |

### Blend V2 pool

`CAJJZSGMMM3PD7N33TAPHGBUGTB43OC73HVIK2L2G6BNGGGYOSSYBXBD`

---

## Examples

### Dry-run (simulate only)

```bash
python scripts/manager_execute_op.py \
    --vault  CAHHS2EFYMKKBYP3LTUFPDS2JSY4EPQLUDEUOFI5I7P7MOIBSJH5AUKQ \
    --guard  CBD7QEXZP2RVIEFD4OUWRDAXKB2BM4GKUME3EZQQEBWAABL3IEGXPGKI \
    --fn     supply \
    --args   '[{"address":"CAJJZSGMMM3PD7N33TAPHGBUGTB43OC73HVIK2L2G6BNGGGYOSSYBXBD"},
               {"address":"CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"},
               {"i128":1000000}]' \
    --dry-run
```

### Supply 0.1 USDC to Blend — Alpha vault

```bash
python scripts/manager_execute_op.py \
    --vault  CAHHS2EFYMKKBYP3LTUFPDS2JSY4EPQLUDEUOFI5I7P7MOIBSJH5AUKQ \
    --guard  CBD7QEXZP2RVIEFD4OUWRDAXKB2BM4GKUME3EZQQEBWAABL3IEGXPGKI \
    --fn     supply \
    --args   '[{"address":"CAJJZSGMMM3PD7N33TAPHGBUGTB43OC73HVIK2L2G6BNGGGYOSSYBXBD"},
               {"address":"CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"},
               {"i128":1000000}]'
```

**Arg breakdown** (`blend.supply(vault, pool, asset, amount)`):

| Position | Value | Meaning |
|----------|-------|---------|
| 0 | `CAJJ...` | Blend V2 pool address |
| 1 | `CCW6...` | USDC token address |
| 2 | `1000000` | Amount in USDC stroop (0.1 USDC) |

> The vault address is injected automatically by `execute_op` before these args.

---

### Withdraw from Blend lending — Beta vault

```bash
python scripts/manager_execute_op.py \
    --vault  CDYB5FK54OXV36AQ2TBK6V2K6KYN6RXNIID6HMCYUP7EJ4BEV7BAVIYB \
    --guard  CBO5XSLPO4DCJJSWWWCPHZ6JDFKPFBPMQDLJWUCJJ3PEWRYO7V6JOJ7V \
    --fn     withdraw_from_lending \
    --args   '[{"address":"CAJJZSGMMM3PD7N33TAPHGBUGTB43OC73HVIK2L2G6BNGGGYOSSYBXBD"},
               {"address":"CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"},
               {"i128":1000000}]'
```

---

### Withdraw 50% of all Blend positions — Gamma vault

`withdraw_fraction(vault, numerator, denominator, to)` — the vault sends
underlying tokens directly to `to`.

```bash
python scripts/manager_execute_op.py \
    --vault  CCHJFS4OEKTLJLLL6OQXFIS2ECTJRA6WMUNXVS7MD6DKIIB6RTXNBZRW \
    --guard  CDMPATIFU2P7JRRAQZZ3655IZSNON62V3EUZK2UZH33C7ACQF6EQ2HYM \
    --fn     withdraw_fraction \
    --args   '[{"u32":1},{"u32":2},
               {"address":"CCHJFS4OEKTLJLLL6OQXFIS2ECTJRA6WMUNXVS7MD6DKIIB6RTXNBZRW"}]'
```

**Arg breakdown** (`blend.withdraw_fraction(vault, numerator, denominator, to)`):

| Position | Value | Meaning |
|----------|-------|---------|
| 0 | `1` (u32) | Numerator |
| 1 | `2` (u32) | Denominator → 1/2 = 50% |
| 2 | Gamma vault address | Recipient of withdrawn tokens |

---

### Testnet

```bash
python scripts/manager_execute_op.py \
    --network testnet \
    --vault  <TESTNET_VAULT_ID> \
    --guard  <TESTNET_STRATEGY_ID> \
    --fn     supply \
    --args   '[...]'
```

---

## Output

```
Network : mainnet
Manager : GCDXIPE5MSBFCYXNM2MP322PGWJWE45T7XJQR73TCY7CC5XIENTSFELX
Vault   : CAHHS2EFYMKKBYP3LTUFPDS2JSY4EPQLUDEUOFI5I7P7MOIBSJH5AUKQ
Guard   : CBD7QEXZP2RVIEFD4OUWRDAXKB2BM4GKUME3EZQQEBWAABL3IEGXPGKI
Fn      : supply
Args    : [{"address":"CAJJ..."},{"address":"CCW6..."},{"i128":1000000}]

Simulating...
  CPU instructions : 8_234_910
  Memory bytes     : 342_100
  Min resource fee : 51423 stroops

Submitting...
  Tx hash : c296509f5d320ab8428aa21495c8a4eea9895c26d3d3d77839f6eba63eab902b
  Explorer: https://stellar.expert/explorer/public/tx/c296509f...
  Waiting for confirmation .... ✓
  Status  : SUCCESS
```

---

## Troubleshooting

| Error | Cause | Fix |
|-------|-------|-----|
| `guard is not whitelisted` | Strategy not in vault's `ActiveGuards` | Register the guard via `vault.add_guard(...)` as admin |
| `fn_name not authorized` | Function not in `AuthorizedOps(guard)` | Add it via `vault.authorize_op(guard, fn_name)` |
| `NotManager` | Signing key is not the registered manager | Use the correct `MANAGER_SECRET` |
| `ResourceLimitExceeded` | Instruction budget too tight for simulation estimate | Add `--fee 5000000` to pad the budget |
| `PriceNotAvailable` (oracle) | USDC fixed-price oracle price entry is stale | Call `set_price` on `CCLT42BF...` to refresh the timestamp |
