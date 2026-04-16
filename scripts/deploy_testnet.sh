#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# -----------------------------
# Config (override via env)
# -----------------------------
NETWORK="${NETWORK:-testnet}"
SOURCE_ACCOUNT="${SOURCE_ACCOUNT:-deployer}"

MANAGER_ACCOUNT="${MANAGER_ACCOUNT:-$SOURCE_ACCOUNT}"
MANAGER_SIGNER="${MANAGER_SIGNER:-$SOURCE_ACCOUNT}"
TRADER_ACCOUNT="${TRADER_ACCOUNT:-$SOURCE_ACCOUNT}"
FACTORY_ADMIN_ACCOUNT="${FACTORY_ADMIN_ACCOUNT:-$SOURCE_ACCOUNT}"
FACTORY_ADMIN_SIGNER="${FACTORY_ADMIN_SIGNER:-$SOURCE_ACCOUNT}"
ORACLE_ADMIN_ACCOUNT="${ORACLE_ADMIN_ACCOUNT:-$SOURCE_ACCOUNT}"
ORACLE_ADMIN_SIGNER="${ORACLE_ADMIN_SIGNER:-$SOURCE_ACCOUNT}"

SHARE_TOKEN_NAME="${SHARE_TOKEN_NAME:-Vault Share}"
SHARE_TOKEN_SYMBOL="${SHARE_TOKEN_SYMBOL:-VSHARE}"
SHARE_TOKEN_DECIMALS="${SHARE_TOKEN_DECIMALS:-7}"

ENTRY_FEE_BPS="${ENTRY_FEE_BPS:-0}"
EXIT_FEE_BPS="${EXIT_FEE_BPS:-0}"
MGMT_FEE_BPS="${MGMT_FEE_BPS:-0}"
PERF_FEE_BPS="${PERF_FEE_BPS:-0}"
ORACLE_MAX_AGE_LEDGERS="${ORACLE_MAX_AGE_LEDGERS:-17280}"

BASE_ASSET_CLASSIC="${BASE_ASSET_CLASSIC:-native}"

DEPLOY_GUARDS="${DEPLOY_GUARDS:-true}"
GUARD_WHITELIST="${GUARD_WHITELIST:-}" # comma-separated contract IDs; empty => base asset only

DEPLOY_BLEND_STRATEGY="${DEPLOY_BLEND_STRATEGY:-false}"
BLEND_PROTOCOL_ID="${BLEND_PROTOCOL_ID:-}"
BLEND_NAME="${BLEND_NAME:-Blend Strategy}"

DEPLOY_SOROSWAP_LP_STRATEGY="${DEPLOY_SOROSWAP_LP_STRATEGY:-false}"
SOROSWAP_ASSET_A="${SOROSWAP_ASSET_A:-}"
SOROSWAP_ASSET_B="${SOROSWAP_ASSET_B:-}"
SOROSWAP_LP_TOKEN="${SOROSWAP_LP_TOKEN:-}"
SOROSWAP_ROUTER="${SOROSWAP_ROUTER:-}"
SOROSWAP_NAME="${SOROSWAP_NAME:-Soroswap LP Strategy}"

DEPLOY_PHOENIX_LP_STRATEGY="${DEPLOY_PHOENIX_LP_STRATEGY:-false}"
PHOENIX_ASSET_A="${PHOENIX_ASSET_A:-}"
PHOENIX_ASSET_B="${PHOENIX_ASSET_B:-}"
PHOENIX_POOL="${PHOENIX_POOL:-}"
PHOENIX_NAME="${PHOENIX_NAME:-Phoenix LP Strategy}"

TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="$ROOT_DIR/deployments"
OUT_FILE="$OUT_DIR/${NETWORK}-${TIMESTAMP}.env"
mkdir -p "$OUT_DIR"

log() {
  printf "[%s] %s\n" "$(date +%H:%M:%S)" "$*" >&2
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Missing required command: $1" >&2
    exit 1
  }
}

resolve_addr() {
  local input="$1"
  if [[ "$input" =~ ^G[A-Z2-7]{55}$ ]]; then
    echo "$input"
  else
    stellar keys public-key "$input"
  fi
}

json_string() {
  local s="$1"
  s="${s//\\/\\\\}"
  s="${s//\"/\\\"}"
  printf '"%s"' "$s"
}

make_addr_vec_json() {
  local csv="$1"
  local json="["
  local first=true
  IFS=',' read -ra parts <<< "$csv"
  for raw in "${parts[@]}"; do
    local token
    token="$(echo "$raw" | xargs)"
    [[ -z "$token" ]] && continue
    if $first; then
      first=false
    else
      json+=","
    fi
    json+="\"$token\""
  done
  json+="]"
  echo "$json"
}

deploy_pkg() {
  local pkg="$1"
  local alias="$2"
  log "Deploying package '$pkg' as alias '$alias'"
  stellar contract deploy \
    --network "$NETWORK" \
    --source-account "$SOURCE_ACCOUNT" \
    --alias "$alias" \
    --package "$pkg"
}

invoke() {
  local signer="$1"
  shift
  stellar contract invoke --network "$NETWORK" --source-account "$signer" "$@"
}

require_cmd stellar

log "Resolving key addresses"
MANAGER_ADDR="$(resolve_addr "$MANAGER_ACCOUNT")"
TRADER_ADDR="$(resolve_addr "$TRADER_ACCOUNT")"
FACTORY_ADMIN_ADDR="$(resolve_addr "$FACTORY_ADMIN_ACCOUNT")"
ORACLE_ADMIN_ADDR="$(resolve_addr "$ORACLE_ADMIN_ACCOUNT")"

log "Resolving base asset contract id from classic asset '$BASE_ASSET_CLASSIC'"
BASE_ASSET_ID="$(stellar contract id asset --network "$NETWORK" --asset "$BASE_ASSET_CLASSIC")"

log "Deploying core contracts"
SHARE_TOKEN_ID="$(deploy_pkg "share-token" "share-token-$TIMESTAMP")"
VAULT_ID="$(deploy_pkg "vault" "vault-$TIMESTAMP")"
ORACLE_ID="$(deploy_pkg "oracle" "oracle-$TIMESTAMP")"
FACTORY_ID="$(deploy_pkg "factory" "factory-$TIMESTAMP")"

log "Initializing share token"
invoke "$MANAGER_SIGNER" \
  --id "$SHARE_TOKEN_ID" -- \
  initialize \
  --admin "$MANAGER_ADDR" \
  --name "$SHARE_TOKEN_NAME" \
  --symbol "$SHARE_TOKEN_SYMBOL" \
  --decimals "$SHARE_TOKEN_DECIMALS" >/dev/null

log "Initializing oracle"
invoke "$ORACLE_ADMIN_SIGNER" \
  --id "$ORACLE_ID" -- \
  initialize --admin "$ORACLE_ADMIN_ADDR" >/dev/null

log "Setting oracle max age ledgers to $ORACLE_MAX_AGE_LEDGERS"
invoke "$ORACLE_ADMIN_SIGNER" \
  --id "$ORACLE_ID" -- \
  set_max_age_ledgers --max-age-ledgers "$ORACLE_MAX_AGE_LEDGERS" >/dev/null

log "Initializing factory"
invoke "$FACTORY_ADMIN_SIGNER" \
  --id "$FACTORY_ID" -- \
  initialize --admin "$FACTORY_ADMIN_ADDR" >/dev/null

VAULT_PARAMS_JSON="{\"manager\":\"$MANAGER_ADDR\",\"trader\":\"$TRADER_ADDR\",\"base_asset\":\"$BASE_ASSET_ID\",\"share_token\":\"$SHARE_TOKEN_ID\",\"share_token_admin\":\"$MANAGER_ADDR\",\"entry_fee_bps\":$ENTRY_FEE_BPS,\"exit_fee_bps\":$EXIT_FEE_BPS,\"mgmt_fee_bps\":$MGMT_FEE_BPS,\"perf_fee_bps\":$PERF_FEE_BPS}"

log "Initializing vault"
invoke "$MANAGER_SIGNER" \
  --id "$VAULT_ID" -- \
  initialize --params "$VAULT_PARAMS_JSON" >/dev/null

log "Wiring oracle into vault"
invoke "$MANAGER_SIGNER" \
  --id "$VAULT_ID" -- \
  set_oracle --caller "$MANAGER_ADDR" --oracle "$ORACLE_ID" >/dev/null

log "Registering vault in factory"
invoke "$FACTORY_ADMIN_SIGNER" \
  --id "$FACTORY_ID" -- \
  verify_and_register_vault --caller "$FACTORY_ADMIN_ADDR" --vault "$VAULT_ID" >/dev/null

SOROSWAP_GUARD_ID=""
PHOENIX_GUARD_ID=""

if [[ "$DEPLOY_GUARDS" == "true" ]]; then
  if [[ -z "$GUARD_WHITELIST" ]]; then
    GUARD_WHITELIST="$BASE_ASSET_ID"
  fi
  GUARD_TOKENS_JSON="$(make_addr_vec_json "$GUARD_WHITELIST")"

  log "Deploying trade guards"
  SOROSWAP_GUARD_ID="$(deploy_pkg "soroswap-trade-guard" "soroswap-guard-$TIMESTAMP")"
  PHOENIX_GUARD_ID="$(deploy_pkg "phoenix-trade-guard" "phoenix-guard-$TIMESTAMP")"

  log "Initializing Soroswap guard"
  invoke "$MANAGER_SIGNER" \
    --id "$SOROSWAP_GUARD_ID" -- \
    initialize --vault "$VAULT_ID" --manager "$MANAGER_ADDR" --tokens "$GUARD_TOKENS_JSON" >/dev/null

  log "Initializing Phoenix guard"
  invoke "$MANAGER_SIGNER" \
    --id "$PHOENIX_GUARD_ID" -- \
    initialize --vault "$VAULT_ID" --manager "$MANAGER_ADDR" --tokens "$GUARD_TOKENS_JSON" >/dev/null
fi

BLEND_STRATEGY_ID=""
if [[ "$DEPLOY_BLEND_STRATEGY" == "true" ]]; then
  [[ -n "$BLEND_PROTOCOL_ID" ]] || { echo "BLEND_PROTOCOL_ID is required when DEPLOY_BLEND_STRATEGY=true" >&2; exit 1; }
  BLEND_STRATEGY_ID="$(deploy_pkg "blend-strategy" "blend-strategy-$TIMESTAMP")"
  log "Initializing blend strategy"
  invoke "$MANAGER_SIGNER" \
    --id "$BLEND_STRATEGY_ID" -- \
    initialize \
    --vault "$VAULT_ID" \
    --asset "$BASE_ASSET_ID" \
    --protocol "$BLEND_PROTOCOL_ID" \
    --manager "$MANAGER_ADDR" \
    --name "$BLEND_NAME" >/dev/null
fi

SOROSWAP_LP_STRATEGY_ID=""
if [[ "$DEPLOY_SOROSWAP_LP_STRATEGY" == "true" ]]; then
  [[ -n "$SOROSWAP_ASSET_A" && -n "$SOROSWAP_ASSET_B" && -n "$SOROSWAP_LP_TOKEN" && -n "$SOROSWAP_ROUTER" ]] || {
    echo "SOROSWAP_ASSET_A, SOROSWAP_ASSET_B, SOROSWAP_LP_TOKEN, and SOROSWAP_ROUTER are required when DEPLOY_SOROSWAP_LP_STRATEGY=true" >&2
    exit 1
  }
  SOROSWAP_LP_STRATEGY_ID="$(deploy_pkg "soroswap-lp-strategy" "soroswap-lp-$TIMESTAMP")"
  log "Initializing soroswap LP strategy"
  invoke "$MANAGER_SIGNER" \
    --id "$SOROSWAP_LP_STRATEGY_ID" -- \
    initialize \
    --vault "$VAULT_ID" \
    --asset-a "$SOROSWAP_ASSET_A" \
    --asset-b "$SOROSWAP_ASSET_B" \
    --lp-token "$SOROSWAP_LP_TOKEN" \
    --router "$SOROSWAP_ROUTER" \
    --manager "$MANAGER_ADDR" \
    --name "$SOROSWAP_NAME" >/dev/null
fi

PHOENIX_LP_STRATEGY_ID=""
if [[ "$DEPLOY_PHOENIX_LP_STRATEGY" == "true" ]]; then
  [[ -n "$PHOENIX_ASSET_A" && -n "$PHOENIX_ASSET_B" && -n "$PHOENIX_POOL" ]] || {
    echo "PHOENIX_ASSET_A, PHOENIX_ASSET_B, and PHOENIX_POOL are required when DEPLOY_PHOENIX_LP_STRATEGY=true" >&2
    exit 1
  }
  PHOENIX_LP_STRATEGY_ID="$(deploy_pkg "phoenix-lp-strategy" "phoenix-lp-$TIMESTAMP")"
  log "Initializing phoenix LP strategy"
  invoke "$MANAGER_SIGNER" \
    --id "$PHOENIX_LP_STRATEGY_ID" -- \
    initialize \
    --vault "$VAULT_ID" \
    --asset-a "$PHOENIX_ASSET_A" \
    --asset-b "$PHOENIX_ASSET_B" \
    --phoenix-pool "$PHOENIX_POOL" \
    --manager "$MANAGER_ADDR" \
    --name "$PHOENIX_NAME" >/dev/null
fi

log "Writing deployment output to $OUT_FILE"
cat > "$OUT_FILE" <<ENVVARS
NETWORK=$NETWORK
TIMESTAMP=$TIMESTAMP
SOURCE_ACCOUNT=$SOURCE_ACCOUNT
MANAGER_ADDR=$MANAGER_ADDR
TRADER_ADDR=$TRADER_ADDR
FACTORY_ADMIN_ADDR=$FACTORY_ADMIN_ADDR
ORACLE_ADMIN_ADDR=$ORACLE_ADMIN_ADDR
BASE_ASSET_CLASSIC=$BASE_ASSET_CLASSIC
BASE_ASSET_ID=$BASE_ASSET_ID
SHARE_TOKEN_ID=$SHARE_TOKEN_ID
VAULT_ID=$VAULT_ID
ORACLE_ID=$ORACLE_ID
FACTORY_ID=$FACTORY_ID
SOROSWAP_GUARD_ID=$SOROSWAP_GUARD_ID
PHOENIX_GUARD_ID=$PHOENIX_GUARD_ID
BLEND_STRATEGY_ID=$BLEND_STRATEGY_ID
SOROSWAP_LP_STRATEGY_ID=$SOROSWAP_LP_STRATEGY_ID
PHOENIX_LP_STRATEGY_ID=$PHOENIX_LP_STRATEGY_ID
ENVVARS

cat "$OUT_FILE"

log "Done"
