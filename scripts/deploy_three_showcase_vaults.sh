#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

NETWORK="${NETWORK:-testnet}"
SOURCE="${SOURCE_ACCOUNT:-deployer}"
TS="$(date +%Y%m%d-%H%M%S)"
OUT_ENV="deployments/showcase-three-vaults-${TS}.env"

source deployments/testnet-20260416-150521.env
MANAGER_ADDR="$MANAGER_ADDR"
TRADER_ADDR="$TRADER_ADDR"
FACTORY_ID="$FACTORY_ID"
ORACLE_ID="$ORACLE_ID"

if ! stellar keys public-key demo_user >/dev/null 2>&1; then
  stellar keys generate demo_user --network "$NETWORK" --fund --overwrite >/dev/null
fi
DEMO_USER_ADDR="$(stellar keys public-key demo_user)"

log() { printf "[%s] %s\n" "$(date +%H:%M:%S)" "$*" >&2; }

deploy_pkg() {
  local pkg="$1"
  local alias="$2"
  stellar contract deploy --network "$NETWORK" --source-account "$SOURCE" --alias "$alias" --package "$pkg"
}

invoke() {
  local signer="$1"
  shift
  stellar contract invoke --network "$NETWORK" --source-account "$signer" "$@"
}

invoke_tx() {
  local signer="$1"
  shift
  local out
  out="$(stellar contract invoke --network "$NETWORK" --source-account "$signer" "$@" 2>&1)"
  echo "$out" >&2
  echo "$out" | rg -o 'https://stellar\.expert/explorer/testnet/tx/[a-f0-9]+' | tail -n1
}

set_oracle_price() {
  local asset="$1"
  local price="$2"
  invoke "$SOURCE" --id "$ORACLE_ID" -- set_price --asset "$asset" --price "$price" >/dev/null
}

init_token() {
  local token_id="$1"
  local name="$2"
  local symbol="$3"
  invoke "$SOURCE" --id "$token_id" -- initialize --admin "$MANAGER_ADDR" --name "$name" --symbol "$symbol" --decimals 7 >/dev/null
}

mint_token() {
  local token_id="$1"
  local to="$2"
  local amt="$3"
  invoke "$SOURCE" --id "$token_id" -- mint --to "$to" --amount "$amt" >/dev/null
}

init_vault() {
  local vault_id="$1"
  local share_id="$2"
  local entry="$3"
  local exitf="$4"
  local mgmt="$5"
  local perf="$6"

  local params
  params="{\"manager\":\"$MANAGER_ADDR\",\"trader\":\"$TRADER_ADDR\",\"base_asset\":\"$USDC_ID\",\"share_token\":\"$share_id\",\"share_token_admin\":\"$MANAGER_ADDR\",\"entry_fee_bps\":$entry,\"exit_fee_bps\":$exitf,\"mgmt_fee_bps\":$mgmt,\"perf_fee_bps\":$perf}"
  invoke "$SOURCE" --id "$vault_id" -- initialize --params "$params" >/dev/null
  invoke "$SOURCE" --id "$vault_id" -- set_oracle --caller "$MANAGER_ADDR" --oracle "$ORACLE_ID" >/dev/null
  invoke "$SOURCE" --id "$FACTORY_ID" -- verify_and_register_vault --caller "$MANAGER_ADDR" --vault "$vault_id" >/dev/null
}

log "Deploying 5 demo assets"
USDC_ID="$(deploy_pkg share-token usdc-demo-${TS})"
WETH_ID="$(deploy_pkg share-token weth-demo-${TS})"
WBTC_ID="$(deploy_pkg share-token wbtc-demo-${TS})"
XAU_ID="$(deploy_pkg share-token xau-demo-${TS})"
EURC_ID="$(deploy_pkg share-token eurc-demo-${TS})"

init_token "$USDC_ID" "USD Coin" "USDC"
init_token "$WETH_ID" "Wrapped Ether" "WETH"
init_token "$WBTC_ID" "Wrapped Bitcoin" "WBTC"
init_token "$XAU_ID" "PAX Gold" "XAU"
init_token "$EURC_ID" "Euro Coin" "EURC"

# Mint balances for manager and demo user
mint_token "$USDC_ID" "$MANAGER_ADDR" 300000000000
mint_token "$USDC_ID" "$DEMO_USER_ADDR" 300000000000
for tok in "$WETH_ID" "$WBTC_ID" "$XAU_ID" "$EURC_ID"; do
  mint_token "$tok" "$MANAGER_ADDR" 100000000000
  mint_token "$tok" "$DEMO_USER_ADDR" 100000000000
done

log "Setting oracle prices"
set_oracle_price "$USDC_ID" 10000000
set_oracle_price "$WETH_ID" 35000000000
set_oracle_price "$WBTC_ID" 650000000000
set_oracle_price "$XAU_ID" 24000000000
set_oracle_price "$EURC_ID" 10800000

log "Deploying Alpha vault"
ALPHA_SHARE_ID="$(deploy_pkg share-token alpha-share-${TS})"
ALPHA_VAULT_ID="$(deploy_pkg vault alpha-vault-${TS})"
init_token "$ALPHA_SHARE_ID" "Alpha Vault Share" "aVSH"
init_vault "$ALPHA_VAULT_ID" "$ALPHA_SHARE_ID" 100 100 250 2500
invoke "$SOURCE" --id "$ALPHA_VAULT_ID" -- set_max_loss_bps --caller "$MANAGER_ADDR" --bps 1500 >/dev/null
invoke "$SOURCE" --id "$ALPHA_VAULT_ID" -- set_deposit_cap --caller "$MANAGER_ADDR" --cap 1000000000000 >/dev/null

ALPHA_GUARD_ID="$(deploy_pkg soroswap-trade-guard alpha-guard-${TS})"
invoke "$SOURCE" --id "$ALPHA_GUARD_ID" -- initialize --vault "$ALPHA_VAULT_ID" --manager "$MANAGER_ADDR" --tokens "[\"$USDC_ID\",\"$WETH_ID\",\"$WBTC_ID\"]" >/dev/null

log "Deploying Beta vault"
BETA_SHARE_ID="$(deploy_pkg share-token beta-share-${TS})"
BETA_VAULT_ID="$(deploy_pkg vault beta-vault-${TS})"
init_token "$BETA_SHARE_ID" "Beta Vault Share" "bVSH"
init_vault "$BETA_VAULT_ID" "$BETA_SHARE_ID" 50 50 150 1500
invoke "$SOURCE" --id "$BETA_VAULT_ID" -- set_max_loss_bps --caller "$MANAGER_ADDR" --bps 700 >/dev/null
invoke "$SOURCE" --id "$BETA_VAULT_ID" -- set_deposit_cap --caller "$MANAGER_ADDR" --cap 1500000000000 >/dev/null

BETA_GUARD_ID="$(deploy_pkg soroswap-trade-guard beta-guard-${TS})"
invoke "$SOURCE" --id "$BETA_GUARD_ID" -- initialize --vault "$BETA_VAULT_ID" --manager "$MANAGER_ADDR" --tokens "[\"$USDC_ID\",\"$WETH_ID\",\"$WBTC_ID\",\"$XAU_ID\",\"$EURC_ID\"]" >/dev/null

log "Deploying Gamma vault"
GAMMA_SHARE_ID="$(deploy_pkg share-token gamma-share-${TS})"
GAMMA_VAULT_ID="$(deploy_pkg vault gamma-vault-${TS})"
init_token "$GAMMA_SHARE_ID" "Gamma Vault Share" "gVSH"
init_vault "$GAMMA_VAULT_ID" "$GAMMA_SHARE_ID" 10 10 50 500
invoke "$SOURCE" --id "$GAMMA_VAULT_ID" -- set_max_loss_bps --caller "$MANAGER_ADDR" --bps 200 >/dev/null
invoke "$SOURCE" --id "$GAMMA_VAULT_ID" -- set_exit_cooldown_secs --caller "$MANAGER_ADDR" --secs 60 >/dev/null
invoke "$SOURCE" --id "$GAMMA_VAULT_ID" -- set_deposit_cap --caller "$MANAGER_ADDR" --cap 2000000000000 >/dev/null

GAMMA_GUARD_ID="$(deploy_pkg soroswap-trade-guard gamma-guard-${TS})"
invoke "$SOURCE" --id "$GAMMA_GUARD_ID" -- initialize --vault "$GAMMA_VAULT_ID" --manager "$MANAGER_ADDR" --tokens "[\"$USDC_ID\"]" >/dev/null

# Gamma lending setup via BlendStrategy + mock lending protocol
MOCK_POOL_ID="$(deploy_pkg mock-blend-pool gamma-mockpool-${TS})"
invoke "$SOURCE" --id "$MOCK_POOL_ID" -- initialize --admin "$MANAGER_ADDR" --token "$USDC_ID" >/dev/null

GAMMA_BLEND_STRATEGY_ID="$(deploy_pkg blend-strategy gamma-blend-${TS})"
invoke "$SOURCE" --id "$GAMMA_BLEND_STRATEGY_ID" -- initialize --vault "$GAMMA_VAULT_ID" --asset "$USDC_ID" --protocol "$MOCK_POOL_ID" --manager "$MANAGER_ADDR" --name "Gamma USDC Lending" >/dev/null
invoke "$SOURCE" --id "$GAMMA_VAULT_ID" -- set_strategies --caller "$MANAGER_ADDR" --strategies "[\"$GAMMA_BLEND_STRATEGY_ID\"]" >/dev/null
invoke "$SOURCE" --id "$GAMMA_VAULT_ID" -- set_trade_guard --caller "$MANAGER_ADDR" --strategy "$GAMMA_BLEND_STRATEGY_ID" --guard "$GAMMA_GUARD_ID" >/dev/null

log "Running user deposit/withdraw transactions"
DEPOSIT_AMOUNT=1000000000

# Alpha user flow
invoke "$SOURCE" --id "$USDC_ID" -- approve --from "$DEMO_USER_ADDR" --spender "$ALPHA_VAULT_ID" --amount "$DEPOSIT_AMOUNT" --expiration_ledger 99999999 >/dev/null
ALPHA_DEPOSIT_TX="$(invoke_tx demo_user --id "$ALPHA_VAULT_ID" -- deposit --amount "$DEPOSIT_AMOUNT" --from "$DEMO_USER_ADDR")"
ALPHA_SHARES="$(invoke "$SOURCE" --id "$ALPHA_SHARE_ID" -- balance --id "$DEMO_USER_ADDR" | tr -d '"')"
ALPHA_WITHDRAW_TX="$(invoke_tx demo_user --id "$ALPHA_VAULT_ID" -- withdraw --share_amount "$ALPHA_SHARES" --from "$DEMO_USER_ADDR" --to "$DEMO_USER_ADDR")"

# Beta user flow
invoke "$SOURCE" --id "$USDC_ID" -- approve --from "$DEMO_USER_ADDR" --spender "$BETA_VAULT_ID" --amount "$DEPOSIT_AMOUNT" --expiration_ledger 99999999 >/dev/null
BETA_DEPOSIT_TX="$(invoke_tx demo_user --id "$BETA_VAULT_ID" -- deposit --amount "$DEPOSIT_AMOUNT" --from "$DEMO_USER_ADDR")"
BETA_SHARES="$(invoke "$SOURCE" --id "$BETA_SHARE_ID" -- balance --id "$DEMO_USER_ADDR" | tr -d '"')"
BETA_WITHDRAW_TX="$(invoke_tx demo_user --id "$BETA_VAULT_ID" -- withdraw --share_amount "$BETA_SHARES" --from "$DEMO_USER_ADDR" --to "$DEMO_USER_ADDR")"

# Gamma user flow + lending actions
invoke "$SOURCE" --id "$USDC_ID" -- approve --from "$DEMO_USER_ADDR" --spender "$GAMMA_VAULT_ID" --amount "$DEPOSIT_AMOUNT" --expiration_ledger 99999999 >/dev/null
GAMMA_DEPOSIT_TX="$(invoke_tx demo_user --id "$GAMMA_VAULT_ID" -- deposit --amount "$DEPOSIT_AMOUNT" --from "$DEMO_USER_ADDR")"

# Manager allocates and simulates yield
GAMMA_INVEST_TX="$(invoke_tx "$SOURCE" --id "$GAMMA_VAULT_ID" -- invest --caller "$MANAGER_ADDR" --strategy "$GAMMA_BLEND_STRATEGY_ID" --amount 700000000)"
GAMMA_YIELD_TX="$(invoke_tx "$SOURCE" --id "$MOCK_POOL_ID" -- add_yield --caller "$MANAGER_ADDR" --amount 30000000)"
invoke "$SOURCE" --id "$GAMMA_BLEND_STRATEGY_ID" -- sync_position --caller "$MANAGER_ADDR" --actual_position 730000000 >/dev/null

# Move ledger forward to satisfy cooldown
stellar contract invoke --network "$NETWORK" --source-account "$SOURCE" --id "$USDC_ID" -- balance --id "$MANAGER_ADDR" >/dev/null
stellar contract invoke --network "$NETWORK" --source-account "$SOURCE" --id "$USDC_ID" -- balance --id "$MANAGER_ADDR" >/dev/null
stellar contract invoke --network "$NETWORK" --source-account "$SOURCE" --id "$USDC_ID" -- balance --id "$MANAGER_ADDR" >/dev/null

GAMMA_SHARES="$(invoke "$SOURCE" --id "$GAMMA_SHARE_ID" -- balance --id "$DEMO_USER_ADDR" | tr -d '"')"
GAMMA_WITHDRAW_TX="$(invoke_tx demo_user --id "$GAMMA_VAULT_ID" -- withdraw --share_amount "$GAMMA_SHARES" --from "$DEMO_USER_ADDR" --to "$DEMO_USER_ADDR")"

cat > "$OUT_ENV" <<ENVVARS
NETWORK=$NETWORK
TIMESTAMP=$TS
SOURCE_ACCOUNT=$SOURCE
DEMO_USER_ADDR=$DEMO_USER_ADDR
USDC_ID=$USDC_ID
WETH_ID=$WETH_ID
WBTC_ID=$WBTC_ID
XAU_ID=$XAU_ID
EURC_ID=$EURC_ID
ALPHA_VAULT_ID=$ALPHA_VAULT_ID
ALPHA_SHARE_ID=$ALPHA_SHARE_ID
ALPHA_GUARD_ID=$ALPHA_GUARD_ID
BETA_VAULT_ID=$BETA_VAULT_ID
BETA_SHARE_ID=$BETA_SHARE_ID
BETA_GUARD_ID=$BETA_GUARD_ID
GAMMA_VAULT_ID=$GAMMA_VAULT_ID
GAMMA_SHARE_ID=$GAMMA_SHARE_ID
GAMMA_GUARD_ID=$GAMMA_GUARD_ID
MOCK_POOL_ID=$MOCK_POOL_ID
GAMMA_BLEND_STRATEGY_ID=$GAMMA_BLEND_STRATEGY_ID
ALPHA_DEPOSIT_TX=$ALPHA_DEPOSIT_TX
ALPHA_WITHDRAW_TX=$ALPHA_WITHDRAW_TX
BETA_DEPOSIT_TX=$BETA_DEPOSIT_TX
BETA_WITHDRAW_TX=$BETA_WITHDRAW_TX
GAMMA_DEPOSIT_TX=$GAMMA_DEPOSIT_TX
GAMMA_INVEST_TX=$GAMMA_INVEST_TX
GAMMA_YIELD_TX=$GAMMA_YIELD_TX
GAMMA_WITHDRAW_TX=$GAMMA_WITHDRAW_TX
ENVVARS

log "Wrote $OUT_ENV"
cat "$OUT_ENV"
