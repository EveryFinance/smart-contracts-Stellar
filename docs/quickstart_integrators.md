# Quickstart for Integrators

This guide gives copy-paste commands to interact with deployed contracts on Stellar testnet.

## 1) Prerequisites

- Stellar CLI installed (`stellar --version`)
- A funded testnet identity (example alias: `deployer`)

If needed:

```bash
stellar keys generate deployer --network testnet --fund --overwrite
```

## 2) Environment Setup

Use your latest deployment file:

```bash
cd smart-contracts-Stellar
set -a
source deployments/testnet-20260416-150521.env
set +a
```

Set caller aliases (using same key for quickstart):

```bash
export SOURCE=deployer
export MANAGER=$MANAGER_ADDR
export TRADER=$TRADER_ADDR
```

## 3) Fast Health Checks

```bash
stellar contract info interface --network testnet --contract-id "$VAULT_ID"
stellar contract info interface --network testnet --contract-id "$SHARE_TOKEN_ID"
stellar contract info interface --network testnet --contract-id "$ORACLE_ID"
stellar contract info interface --network testnet --contract-id "$FACTORY_ID"
```

## 4) Read-Only Calls

### Vault

```bash
stellar contract invoke --network testnet --source-account "$SOURCE" --id "$VAULT_ID" -- get_manager
stellar contract invoke --network testnet --source-account "$SOURCE" --id "$VAULT_ID" -- get_trader
stellar contract invoke --network testnet --source-account "$SOURCE" --id "$VAULT_ID" -- get_nav
stellar contract invoke --network testnet --source-account "$SOURCE" --id "$VAULT_ID" -- get_share_price
stellar contract invoke --network testnet --source-account "$SOURCE" --id "$VAULT_ID" -- get_strategies
```

### Share Token

```bash
stellar contract invoke --network testnet --source-account "$SOURCE" --id "$SHARE_TOKEN_ID" -- name
stellar contract invoke --network testnet --source-account "$SOURCE" --id "$SHARE_TOKEN_ID" -- symbol
stellar contract invoke --network testnet --source-account "$SOURCE" --id "$SHARE_TOKEN_ID" -- total_supply
stellar contract invoke --network testnet --source-account "$SOURCE" --id "$SHARE_TOKEN_ID" -- get_admin
```

### Oracle

```bash
stellar contract invoke --network testnet --source-account "$SOURCE" --id "$ORACLE_ID" -- get_admin
stellar contract invoke --network testnet --source-account "$SOURCE" --id "$ORACLE_ID" -- get_max_age_ledgers
```

### Factory

```bash
stellar contract invoke --network testnet --source-account "$SOURCE" --id "$FACTORY_ID" -- get_admin
stellar contract invoke --network testnet --source-account "$SOURCE" --id "$FACTORY_ID" -- get_vault_count
stellar contract invoke --network testnet --source-account "$SOURCE" --id "$FACTORY_ID" -- get_vaults --offset 0 --limit 50
```

### Trade Guards

```bash
stellar contract invoke --network testnet --source-account "$SOURCE" --id "$SOROSWAP_GUARD_ID" -- get_whitelist
stellar contract invoke --network testnet --source-account "$SOURCE" --id "$PHOENIX_GUARD_ID" -- get_whitelist
```

## 5) Admin / Manager Calls

### Oracle: set price

Example sets native asset price to `1.0` (precision `10_000_000`):

```bash
stellar contract invoke --network testnet --source-account "$SOURCE" --id "$ORACLE_ID" -- \
  set_price --asset "$BASE_ASSET_ID" --price 10000000
```

### Oracle: change freshness window

```bash
stellar contract invoke --network testnet --source-account "$SOURCE" --id "$ORACLE_ID" -- \
  set_max_age_ledgers --max-age-ledgers 17280
```

### Vault: pause / unpause

```bash
stellar contract invoke --network testnet --source-account "$SOURCE" --id "$VAULT_ID" -- \
  pause --caller "$MANAGER"

stellar contract invoke --network testnet --source-account "$SOURCE" --id "$VAULT_ID" -- \
  unpause --caller "$MANAGER"
```

### Vault: update risk parameters

```bash
stellar contract invoke --network testnet --source-account "$SOURCE" --id "$VAULT_ID" -- \
  set_max_loss_bps --caller "$MANAGER" --bps 100

stellar contract invoke --network testnet --source-account "$SOURCE" --id "$VAULT_ID" -- \
  set_max_concentration_bps --caller "$MANAGER" --bps 4000
```

### Factory: check registration

```bash
stellar contract invoke --network testnet --source-account "$SOURCE" --id "$FACTORY_ID" -- \
  is_registered --vault "$VAULT_ID"
```

## 6) Trade Guard Policy Updates

Set whitelist (JSON vec format):

```bash
TOKENS='["'$BASE_ASSET_ID'"]'

stellar contract invoke --network testnet --source-account "$SOURCE" --id "$SOROSWAP_GUARD_ID" -- \
  set_whitelist --caller "$MANAGER" --tokens "$TOKENS"

stellar contract invoke --network testnet --source-account "$SOURCE" --id "$PHOENIX_GUARD_ID" -- \
  set_whitelist --caller "$MANAGER" --tokens "$TOKENS"
```

## 7) Optional: Template for New Deployment File

If you deploy again, switch to newest file:

```bash
LATEST_ENV=$(ls -t deployments/testnet-*.env | head -n 1)
set -a
source "$LATEST_ENV"
set +a
```

## 8) Explorer Links (Current Deployment)

- Share Token: `https://stellar.expert/explorer/testnet/contract/CBD2NZHYBNTN4ECUWVUT7KBTCQOXAK2D6TFDMHZPJIWL2MP56KPQWAN7`
- Vault: `https://stellar.expert/explorer/testnet/contract/CBVK3IC7ALAA5PRWKIRELDHOT6ZXSM6DZHSXDT5Y3Z5WHHVU46EIDJHE`
- Oracle: `https://stellar.expert/explorer/testnet/contract/CCFACGTFLN3CV4LRLRTPK73VUAS2VXDOPMJAZVE4HNVNVNAMREF3QMD2`
- Factory: `https://stellar.expert/explorer/testnet/contract/CCER4YYGW2GEYAYHC7E2ULUQPV5OLTYXV3GUTBQ5IG62CV5YNWHDSJKV`
- Soroswap Guard: `https://stellar.expert/explorer/testnet/contract/CAC5D67PTM7E7W7GZO7J4ENNBTMA443MQZTWHD3YTFRBZWVVMEFVPPPB`
- Phoenix Guard: `https://stellar.expert/explorer/testnet/contract/CBH4GEYHKXU54D3434M7OKJODM3LNT7ED5P6KYFYS2UG4Q46UJPCGXG3`
