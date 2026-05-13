#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

NETWORK="${NETWORK:-testnet}"
SOURCE_ACCOUNT="${SOURCE_ACCOUNT:-deployer}"
MANAGER_ACCOUNT="${MANAGER_ACCOUNT:-$SOURCE_ACCOUNT}"
MANAGER_SIGNER="${MANAGER_SIGNER:-$SOURCE_ACCOUNT}"
TRADER_ACCOUNT="${TRADER_ACCOUNT:-$MANAGER_ACCOUNT}"
TRADER_SIGNER="${TRADER_SIGNER:-$MANAGER_SIGNER}"
TREASURY_ACCOUNT="${TREASURY_ACCOUNT:-$MANAGER_ACCOUNT}"
ADMIN_ACCOUNT="${ADMIN_ACCOUNT:-$MANAGER_ACCOUNT}"
ADMIN_SIGNER="${ADMIN_SIGNER:-$MANAGER_SIGNER}"

ENTRY_FEE_BPS="${ENTRY_FEE_BPS:-0}"
EXIT_FEE_BPS="${EXIT_FEE_BPS:-0}"
MGMT_FEE_BPS="${MGMT_FEE_BPS:-200}"
PERF_FEE_BPS="${PERF_FEE_BPS:-1000}"
COOLDOWN_SECS="${COOLDOWN_SECS:-60}"
COOLDOWN_SLEEP_SECS="${COOLDOWN_SLEEP_SECS:-70}"
WAIT_FOR_COOLDOWN="${WAIT_FOR_COOLDOWN:-true}"
DEPLOY_ALPHA_GAMMA_AFTER_BETA="${DEPLOY_ALPHA_GAMMA_AFTER_BETA:-false}"
STELLAR_RETRY_ATTEMPTS="${STELLAR_RETRY_ATTEMPTS:-6}"
STELLAR_RETRY_SLEEP_SECS="${STELLAR_RETRY_SLEEP_SECS:-20}"

USER1_KEY="${USER1_KEY:-protocol_demo_user_1}"
USER2_KEY="${USER2_KEY:-protocol_demo_user_2}"
USER_MINT_AMOUNT="${USER_MINT_AMOUNT:-100000000000}"
MANAGER_MINT_AMOUNT="${MANAGER_MINT_AMOUNT:-100000000000}"
USER_DEPOSIT_AMOUNT="${USER_DEPOSIT_AMOUNT:-1000000000}"
USER_WITHDRAW_SHARES="${USER_WITHDRAW_SHARES:-200000000}"
TX_FAILURES=0

# PRICE_PRECISION = 1e7. Override these before running if fresher market prices
# are required for a specific demo run.
PRICE_USDC="${PRICE_USDC:-10000000}"
PRICE_XLM="${PRICE_XLM:-1700000}"
PRICE_BTC="${PRICE_BTC:-816844300000}"
PRICE_PYUSD="${PRICE_PYUSD:-10000000}"
PRICE_EURC="${PRICE_EURC:-10800000}"
PRICE_AQUA="${PRICE_AQUA:-4500}"
PRICE_USTRY="${PRICE_USTRY:-10300000}"

TS="$(date -u +%Y%m%d-%H%M%S)"
OUT_DIR="$ROOT_DIR/deployments"
REPORT_DIR="$ROOT_DIR/docs/reports"
OUT_ENV="$OUT_DIR/testnet-protocol-environment-${TS}.env"
LATEST_ENV="$OUT_DIR/testnet-protocol-environment.latest.env"
REPORT_FILE="$REPORT_DIR/testnet-protocol-environment-${TS}.adoc"
mkdir -p "$OUT_DIR" "$REPORT_DIR"

log() {
  printf "[%s] %s\n" "$(date -u +%H:%M:%S)" "$*" >&2
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Missing required command: $1" >&2
    exit 1
  }
}

stellar_retry() {
  local attempt status out_file err_file
  out_file="$(mktemp)"
  err_file="$(mktemp)"
  for attempt in $(seq 1 "$STELLAR_RETRY_ATTEMPTS"); do
    if stellar "$@" >"$out_file" 2>"$err_file"; then
      cat "$err_file" >&2
      cat "$out_file"
      rm -f "$out_file" "$err_file"
      return 0
    fi
    status=$?
    cat "$err_file" >&2
    if [[ "$attempt" -lt "$STELLAR_RETRY_ATTEMPTS" ]] &&
      rg -qi 'request timeout|timeout|temporarily unavailable|rate limit|connection reset|deadline|dns error|name resolution|failed to lookup|network.*error|low-level protocol' "$err_file"; then
      log "Retrying Stellar CLI command after transient RPC error (attempt $attempt/$STELLAR_RETRY_ATTEMPTS)"
      sleep "$STELLAR_RETRY_SLEEP_SECS"
      : >"$out_file"
      : >"$err_file"
      continue
    fi
    cat "$out_file"
    rm -f "$out_file" "$err_file"
    return "$status"
  done
}

resolve_addr() {
  local input="$1"
  if [[ "$input" =~ ^G[A-Z2-7]{55}$ ]]; then
    printf "%s" "$input"
  else
    stellar_retry keys public-key "$input"
  fi
}

ensure_key() {
  local key="$1"
  if ! stellar keys public-key "$key" >/dev/null 2>&1; then
    log "Creating and funding Stellar testnet key '$key'"
    stellar_retry keys generate "$key" --network "$NETWORK" --fund >/dev/null
  fi
}

deploy_pkg() {
  local pkg="$1"
  local alias="$2"
  shift 2
  log "Deploying $pkg as $alias"
  stellar_retry contract deploy \
    --network "$NETWORK" \
    --source-account "$SOURCE_ACCOUNT" \
    --alias "$alias" \
    --package "$pkg" \
    -- "$@"
}

deploy_pkg_salted() {
  local pkg="$1"
  local alias="$2"
  local salt="$3"
  shift 3
  log "Deploying $pkg as $alias with deterministic salt $salt"
  stellar_retry contract deploy \
    --network "$NETWORK" \
    --source-account "$SOURCE_ACCOUNT" \
    --alias "$alias" \
    --package "$pkg" \
    --salt "$salt" \
    -- "$@"
}

contract_id_for_salt() {
  local salt="$1"
  # Computed locally from deployer pubkey + salt — no network round-trip needed.
  stellar contract id wasm --source-account "$SOURCE_ACCOUNT" --salt "$salt"
}

salt_for() {
  local label="$1"
  printf "%s" "$label" | sha256sum | cut -d' ' -f1
}

invoke() {
  local signer="$1"
  shift
  stellar_retry contract invoke --network "$NETWORK" --source-account "$signer" "$@"
}

invoke_clean() {
  invoke "$@" | tr -d '"'
}

val_address() {
  printf '{"address":"%s"}' "$1"
}

val_i128() {
  printf '{"i128":"%s"}' "$1"
}

val_bool() {
  printf '{"bool":%s}' "$1"
}

invoke_tx() {
  local __result_var="$1"
  shift
  local signer="$1"
  shift
  local out
  out="$(stellar_retry contract invoke --network "$NETWORK" --source-account "$signer" "$@" 2>&1)"
  printf "%s\n" "$out" >&2
  if printf "%s\n" "$out" | rg -q '❌ error:'; then
    local reason
    reason="$(printf "%s\n" "$out" | rg '❌ error:' | head -n1 | sed 's/^❌ error: //')"
    TX_FAILURES=$((TX_FAILURES + 1))
    printf -v "$__result_var" "failed: %s" "$reason"
    return 0
  fi
  local link
  link="$(printf "%s\n" "$out" | rg -o 'https://stellar\.expert/explorer/testnet/tx/[a-f0-9]+' | tail -n1 || true)"
  if [[ -n "$link" ]]; then
    printf -v "$__result_var" "%s" "$link"
  else
    printf -v "$__result_var" "submitted"
  fi
}

deploy_mock_asset() {
  local label="$1"
  local name="$2"
  local symbol="$3"
  deploy_pkg "mock-asset" "${label,,}-${TS}" --name "$name" --symbol "$symbol" --decimals 7
}

set_price() {
  local asset="$1"
  local price="$2"
  invoke "$ADMIN_SIGNER" --id "$ORACLE_ID" -- set_price --asset "$asset" --price "$price" >/dev/null
}

register_asset_everywhere() {
  local asset="$1"
  invoke "$ADMIN_SIGNER" --id "$ASSET_HANDLER_ID" -- add_asset --caller "$ADMIN_ADDR" --asset "$asset" >/dev/null
  invoke "$ADMIN_SIGNER" --id "$FACTORY_ID" -- add_authorized_asset --caller "$ADMIN_ADDR" --asset "$asset" >/dev/null
}

authorize_guard() {
  local guard="$1"
  invoke "$ADMIN_SIGNER" --id "$FACTORY_ID" -- add_authorized_guard --caller "$ADMIN_ADDR" --guard "$guard" >/dev/null
}

vault_params_json() {
  local label="$1"
  local share="$2"
  local share_admin="$3"
  printf '{"admin":"%s","manager":"%s","manager_name":"%s Manager","trader":"%s","base_asset":"%s","share_token":"%s","share_token_admin":"%s","treasury":"%s","entry_fee_bps":%s,"exit_fee_bps":%s,"mgmt_fee_bps":%s,"perf_fee_bps":%s,"factory":"%s","is_private":false}' \
    "$ADMIN_ADDR" "$MANAGER_ADDR" "$label" "$TRADER_ADDR" "$USDC_ID" "$share" "$share_admin" "$TREASURY_ADDR" \
    "$ENTRY_FEE_BPS" "$EXIT_FEE_BPS" "$MGMT_FEE_BPS" "$PERF_FEE_BPS" "$FACTORY_ID"
}

adoc_escape() {
  printf "%s" "$1" | sed 's/|/\\|/g'
}

token_balance() {
  local token_id="$1"
  local account="$2"
  invoke_clean "$SOURCE_ACCOUNT" --id "$token_id" -- balance --id "$account" || printf "0"
}

share_balance() {
  local share_id="$1"
  local account="$2"
  token_balance "$share_id" "$account"
}

strategy_value() {
  local strategy="$1"
  local vault="$2"
  if [[ -z "$strategy" ]]; then
    printf "0"
    return
  fi
  invoke_clean "$SOURCE_ACCOUNT" --id "$strategy" -- get_total_value --vault "$vault" || printf "unavailable"
}

vault_nav() {
  local vault="$1"
  invoke_clean "$SOURCE_ACCOUNT" --id "$vault" -- get_nav || printf "unavailable"
}

vault_share_price() {
  local vault="$1"
  invoke_clean "$SOURCE_ACCOUNT" --id "$vault" -- get_share_price || printf "unavailable"
}

append_snapshot() {
  local title="$1"
  local vault="$2"
  local share="$3"
  local user="$4"
  local blend="$5"
  local soroswap="$6"
  local phoenix="$7"

  local nav share_price supply user_usdc user_shares blend_value soroswap_value phoenix_value
  nav="$(vault_nav "$vault")"
  share_price="$(vault_share_price "$vault")"
  supply="$(invoke_clean "$SOURCE_ACCOUNT" --id "$share" -- total_supply || printf "0")"
  user_usdc="$(token_balance "$USDC_ID" "$user")"
  user_shares="$(share_balance "$share" "$user")"
  blend_value="$(strategy_value "$blend" "$vault")"
  soroswap_value="$(strategy_value "$soroswap" "$vault")"
  phoenix_value="$(strategy_value "$phoenix" "$vault")"

  {
    printf "\n==== %s\n\n" "$title"
    printf "[cols=\"1,1\",options=\"header\"]\n|===\n| Metric | Value\n"
    printf '| NAV | `%s`\n' "$nav"
    printf '| Share token price | `%s`\n' "$share_price"
    printf '| Share token supply | `%s`\n' "$supply"
    printf '| User USDC balance | `%s`\n' "$user_usdc"
    printf '| User share balance | `%s`\n' "$user_shares"
    printf '| Blend strategy value | `%s`\n' "$blend_value"
    printf '| Soroswap LP strategy value | `%s`\n' "$soroswap_value"
    printf '| Phoenix LP strategy value | `%s`\n' "$phoenix_value"
    printf "|===\n\n"
    printf ".Idle vault balances\n"
    printf "[cols=\"1,1\",options=\"header\"]\n|===\n| Asset | Balance\n"
    local asset_label
    for asset_label in USDC XLM BTC PYUSD EURC AQUA USTRY; do
      local id_var="${asset_label}_ID"
      local id="${!id_var}"
      printf '| %s | `%s`\n' "$asset_label" "$(token_balance "$id" "$vault")"
    done
    printf "|===\n"
  } >> "$REPORT_FILE"
}

append_tx_row() {
  local phase="$1"
  local vault="$2"
  local action="$3"
  local tx="$4"
  {
    printf "\n.Transaction\n"
    printf "[cols=\"1,2\",options=\"header\"]\n|===\n| Field | Value\n"
    printf '| Phase | %s\n' "$phase"
    printf '| Vault | %s\n' "$vault"
    printf '| Action | %s\n' "$action"
    printf '| Result | %s\n' "$tx"
    printf "|===\n"
  } >> "$REPORT_FILE"
}

deploy_vault_stack() {
  local label="$1"
  local assets_csv="$2"
  local pair_asset="$3"
  local rwa_asset="$4"
  local strategy_mode="${5:-all}"

  local upper="${label^^}"
  local lower="${label,,}"
  local share_id vault_id predicted_vault_id vault_salt blend_pool_id blend_id soroswap_router_id soroswap_id phoenix_share_id phoenix_pool_id phoenix_id params

  share_id="$(deploy_pkg "share-token" "${lower}-share-${TS}" --admin "$MANAGER_ADDR" --name "$label Vault Share" --symbol "${upper}SH" --decimals 7)"
  vault_salt="$(salt_for "${NETWORK}:${SOURCE_ACCOUNT}:${TS}:${lower}:vault")"
  predicted_vault_id="$(contract_id_for_salt "$vault_salt")"
  if [[ -z "$predicted_vault_id" ]]; then
    echo "Failed to predict vault id — contract_id_for_salt returned empty" >&2
    exit 1
  fi
  invoke "$MANAGER_SIGNER" --id "$share_id" -- set_admin --new-admin "$predicted_vault_id" >/dev/null
  params="$(vault_params_json "$label" "$share_id" "$predicted_vault_id")"
  vault_id="$(deploy_pkg_salted "vault" "${lower}-vault-${TS}" "$vault_salt" --params "$params")"
  if [[ "$vault_id" != "$predicted_vault_id" ]]; then
    echo "Predicted vault id $predicted_vault_id did not match deployed id $vault_id" >&2
    exit 1
  fi

  invoke "$MANAGER_SIGNER" --id "$vault_id" -- set_oracle --caller "$MANAGER_ADDR" --oracle "$ORACLE_ID" >/dev/null
  invoke "$MANAGER_SIGNER" --id "$vault_id" -- set_exit_cooldown_secs --caller "$MANAGER_ADDR" --secs "$COOLDOWN_SECS" >/dev/null
  invoke "$ADMIN_SIGNER" --id "$vault_id" -- set_share_transfers_enabled --caller "$ADMIN_ADDR" --enabled false >/dev/null
  invoke "$ADMIN_SIGNER" --id "$FACTORY_ID" -- verify_and_register_vault --caller "$ADMIN_ADDR" --vault "$vault_id" >/dev/null

  IFS=',' read -ra asset_labels <<< "$assets_csv"
  for asset_label in "${asset_labels[@]}"; do
    asset_label="$(printf "%s" "$asset_label" | xargs)"
    local id_var="${asset_label}_ID"
    invoke "$MANAGER_SIGNER" --id "$vault_id" -- add_portfolio_asset --caller "$MANAGER_ADDR" --asset "${!id_var}" >/dev/null
  done
  invoke "$MANAGER_SIGNER" --id "$vault_id" -- add_deposit_asset --caller "$MANAGER_ADDR" --asset "$USDC_ID" >/dev/null

  blend_pool_id="$(deploy_pkg "mock-blend-pool" "${lower}-blend-pool-${TS}")"
  invoke "$MANAGER_SIGNER" --id "$blend_pool_id" -- initialize --admin "$MANAGER_ADDR" --token "$USDC_ID" >/dev/null
  blend_id="$(deploy_pkg "blend-strategy" "${lower}-blend-${TS}")"
  invoke "$MANAGER_SIGNER" --id "$blend_id" -- initialize --vault "$vault_id" --name "$label USDC Lending" >/dev/null

  if [[ "$strategy_mode" == "all" ]]; then
    local pair_var="${pair_asset}_ID"
    local pair_price_var="PRICE_${pair_asset}"
    soroswap_router_id="$(deploy_pkg "mock-soroswap-router" "${lower}-soroswap-router-${TS}" --token0 "$USDC_ID" --token1 "${!pair_var}" --price0 "$PRICE_USDC" --price1 "${!pair_price_var}")"
    soroswap_id="$(deploy_pkg "soroswap-lp-strategy" "${lower}-soroswap-lp-${TS}")"
    invoke "$MANAGER_SIGNER" --id "$soroswap_id" -- initialize \
      --vault "$vault_id" \
      --router "$soroswap_router_id" \
      --name "$label Soroswap LP" >/dev/null

    local rwa_var="${rwa_asset}_ID"
    phoenix_share_id="$(deploy_mock_asset "${lower}-phoenix-share" "$label Phoenix LP Share" "${upper}PLP")"
    phoenix_pool_id="$(deploy_pkg "mock-phoenix-pool" "${lower}-phoenix-pool-${TS}" --share-token "$phoenix_share_id" --token-a "$USDC_ID" --token-b "${!rwa_var}")"
    phoenix_id="$(deploy_pkg "phoenix-lp-strategy" "${lower}-phoenix-lp-${TS}")"
    invoke "$MANAGER_SIGNER" --id "$phoenix_id" -- initialize \
      --vault "$vault_id" \
      --name "$label Phoenix LP" >/dev/null
  else
    soroswap_router_id=""
    soroswap_id=""
    phoenix_share_id=""
    phoenix_pool_id=""
    phoenix_id=""
  fi

  for guard in "$blend_id" "$soroswap_id" "$phoenix_id"; do
    if [[ -z "$guard" ]]; then
      continue
    fi
    authorize_guard "$guard"
    invoke "$MANAGER_SIGNER" --id "$vault_id" -- add_active_guard --caller "$MANAGER_ADDR" --guard "$guard" >/dev/null
  done
  invoke "$MANAGER_SIGNER" --id "$vault_id" -- set_authorized_ops --caller "$MANAGER_ADDR" --guard "$blend_id" --ops '["supply","withdraw_from_lending"]' >/dev/null
  if [[ "$strategy_mode" == "all" ]]; then
    invoke "$MANAGER_SIGNER" --id "$vault_id" -- set_authorized_ops --caller "$MANAGER_ADDR" --guard "$soroswap_id" --ops '["swap","add_liquidity","remove_liquidity"]' >/dev/null
    invoke "$MANAGER_SIGNER" --id "$vault_id" -- set_authorized_ops --caller "$MANAGER_ADDR" --guard "$phoenix_id" --ops '["swap","add_liquidity","remove_liquidity"]' >/dev/null
  fi

  printf -v "${upper}_VAULT_ID" "%s" "$vault_id"
  printf -v "${upper}_SHARE_ID" "%s" "$share_id"
  printf -v "${upper}_BLEND_POOL_ID" "%s" "$blend_pool_id"
  printf -v "${upper}_BLEND_ID" "%s" "$blend_id"
  printf -v "${upper}_SOROSWAP_ROUTER_ID" "%s" "$soroswap_router_id"
  printf -v "${upper}_SOROSWAP_ID" "%s" "$soroswap_id"
  printf -v "${upper}_PHOENIX_SHARE_ID" "%s" "$phoenix_share_id"
  printf -v "${upper}_PHOENIX_POOL_ID" "%s" "$phoenix_pool_id"
  printf -v "${upper}_PHOENIX_ID" "%s" "$phoenix_id"
  printf -v "${upper}_PAIR_ASSET" "%s" "$pair_asset"
  printf -v "${upper}_RWA_ASSET" "%s" "$rwa_asset"
  printf -v "${upper}_PORTFOLIO" "%s" "$assets_csv"
  printf -v "${upper}_STRATEGY_MODE" "%s" "$strategy_mode"
}

run_user_deposit() {
  local phase="$1"
  local label="$2"
  local user_key="$3"
  local user_addr="$4"
  local amount="$5"
  local upper="${label^^}"
  local vault_var="${upper}_VAULT_ID"
  local share_var="${upper}_SHARE_ID"
  local blend_var="${upper}_BLEND_ID"
  local soroswap_var="${upper}_SOROSWAP_ID"
  local phoenix_var="${upper}_PHOENIX_ID"
  append_snapshot "$phase $label before user deposit" "${!vault_var}" "${!share_var}" "$user_addr" "${!blend_var}" "${!soroswap_var}" "${!phoenix_var}"
  local tx
  invoke_tx tx "$user_key" --id "${!vault_var}" -- deposit --amount "$amount" --from "$user_addr" --asset "$USDC_ID" --min-shares-out 0
  append_tx_row "$phase" "$label" "User deposit $amount USDC units" "$tx"
  append_snapshot "$phase $label after user deposit" "${!vault_var}" "${!share_var}" "$user_addr" "${!blend_var}" "${!soroswap_var}" "${!phoenix_var}"
}

run_user_withdraw() {
  local phase="$1"
  local label="$2"
  local user_key="$3"
  local user_addr="$4"
  local share_amount="$5"
  local upper="${label^^}"
  local vault_var="${upper}_VAULT_ID"
  local share_var="${upper}_SHARE_ID"
  local blend_var="${upper}_BLEND_ID"
  local soroswap_var="${upper}_SOROSWAP_ID"
  local phoenix_var="${upper}_PHOENIX_ID"
  append_snapshot "$phase $label before user withdrawal" "${!vault_var}" "${!share_var}" "$user_addr" "${!blend_var}" "${!soroswap_var}" "${!phoenix_var}"
  local available_shares
  available_shares="$(share_balance "${!share_var}" "$user_addr")"
  if ! [[ "$available_shares" =~ ^[0-9]+$ ]]; then
    append_tx_row "$phase" "$label" "User withdraw $share_amount shares" "skipped: share balance unavailable"
    return
  fi
  if [[ "$available_shares" -le 0 ]]; then
    append_tx_row "$phase" "$label" "User withdraw $share_amount shares" "skipped: user has no shares"
    return
  fi
  if [[ "$available_shares" -lt "$share_amount" ]]; then
    share_amount="$available_shares"
  fi
  local tx
  invoke_tx tx "$user_key" --id "${!vault_var}" -- withdraw --share_amount "$share_amount" --from "$user_addr" --to "$user_addr" --min-base-out 0
  append_tx_row "$phase" "$label" "User withdraw $share_amount shares" "$tx"
  append_snapshot "$phase $label after user withdrawal" "${!vault_var}" "${!share_var}" "$user_addr" "${!blend_var}" "${!soroswap_var}" "${!phoenix_var}"
}

run_manager_actions() {
  local label="$1"
  local upper="${label^^}"
  local vault_var="${upper}_VAULT_ID"
  local share_var="${upper}_SHARE_ID"
  local blend_var="${upper}_BLEND_ID"
  local pool_var="${upper}_BLEND_POOL_ID"
  local soroswap_var="${upper}_SOROSWAP_ID"
  local soroswap_router_var="${upper}_SOROSWAP_ROUTER_ID"
  local phoenix_var="${upper}_PHOENIX_ID"
  local phoenix_pool_var="${upper}_PHOENIX_POOL_ID"
  local pair_var="${upper}_PAIR_ASSET"
  local rwa_var="${upper}_RWA_ASSET"
  local pair_id_var="${!pair_var}_ID"
  local rwa_id_var="${!rwa_var}_ID"

  append_snapshot "Phase 2 $label before manager actions" "${!vault_var}" "${!share_var}" "$USER1_ADDR" "${!blend_var}" "${!soroswap_var}" "${!phoenix_var}"

  local tx
  if [[ -n "${!soroswap_var}" ]]; then
    invoke_tx tx "$MANAGER_SIGNER" --id "${!vault_var}" -- execute_op --caller "$MANAGER_ADDR" --guard "${!soroswap_var}" --fn-name swap --args "[$(val_address "$USDC_ID"),$(val_address "${!pair_id_var}"),$(val_i128 100000000),$(val_i128 0)]"
    append_tx_row "Phase 2" "$label" "Manager Soroswap mock swap USDC to ${!pair_var}" "$tx"
  fi

  # supply/withdraw_from_lending: args [pool, asset, amount] — vault injects vault at front
  invoke_tx tx "$MANAGER_SIGNER" --id "${!vault_var}" -- execute_op --caller "$MANAGER_ADDR" --guard "${!blend_var}" --fn-name supply --args "[$(val_address "${!pool_var}"),$(val_address "$USDC_ID"),$(val_i128 300000000)]"
  append_tx_row "Phase 2" "$label" "Manager lending supply" "$tx"

  # Withdraw the full amount so Blend has no remaining position before Phase 3.
  invoke_tx tx "$MANAGER_SIGNER" --id "${!vault_var}" -- execute_op --caller "$MANAGER_ADDR" --guard "${!blend_var}" --fn-name withdraw_from_lending --args "[$(val_address "${!pool_var}"),$(val_address "$USDC_ID"),$(val_i128 300000000)]"
  append_tx_row "Phase 2" "$label" "Manager lending withdrawal" "$tx"

  if [[ -n "${!soroswap_var}" ]]; then
    # add_liquidity: args [lp_token, asset_a, asset_b, amount_a, amount_b, min_a, min_b]
    invoke_tx tx "$MANAGER_SIGNER" --id "${!vault_var}" -- execute_op --caller "$MANAGER_ADDR" --guard "${!soroswap_var}" --fn-name add_liquidity --args "[$(val_address "${!soroswap_router_var}"),$(val_address "$USDC_ID"),$(val_address "${!pair_id_var}"),$(val_i128 50000000),$(val_i128 50000000),$(val_i128 0),$(val_i128 0)]"
    append_tx_row "Phase 2" "$label" "Manager add Soroswap LP liquidity" "$tx"
    local lp_balance
    lp_balance="$(invoke_clean "$SOURCE_ACCOUNT" --id "${!soroswap_var}" -- get_lp_balance --lp-token "${!soroswap_router_var}")"
    if [[ "$lp_balance" -gt 0 ]]; then
      # Remove ALL LP so Soroswap has no remaining position before Phase 3.
      # remove_liquidity: args [lp_token, asset_a, asset_b, lp_amount, min_a, min_b]
      invoke_tx tx "$MANAGER_SIGNER" --id "${!vault_var}" -- execute_op --caller "$MANAGER_ADDR" --guard "${!soroswap_var}" --fn-name remove_liquidity --args "[$(val_address "${!soroswap_router_var}"),$(val_address "$USDC_ID"),$(val_address "${!pair_id_var}"),$(val_i128 "$lp_balance"),$(val_i128 0),$(val_i128 0)]"
      append_tx_row "Phase 2" "$label" "Manager remove Soroswap LP liquidity" "$tx"
    fi
  fi

  if [[ -n "${!phoenix_var}" ]]; then
    # swap: args [pool, asset_in, asset_out, amount_in, min_out]
    invoke_tx tx "$MANAGER_SIGNER" --id "${!vault_var}" -- execute_op --caller "$MANAGER_ADDR" --guard "${!phoenix_var}" --fn-name swap --args "[$(val_address "${!phoenix_pool_var}"),$(val_address "$USDC_ID"),$(val_address "${!rwa_id_var}"),$(val_i128 50000000),$(val_i128 0)]"
    append_tx_row "Phase 2" "$label" "Manager Phoenix mock swap USDC to ${!rwa_var}" "$tx"
  fi

  invoke_tx tx "$MANAGER_SIGNER" --id "${!pool_var}" -- add_yield --caller "$MANAGER_ADDR" --amount 10000000
  append_tx_row "Phase 2" "$label" "Manager adds mock lending yield" "$tx"

  append_snapshot "Phase 2 $label after manager actions" "${!vault_var}" "${!share_var}" "$USER1_ADDR" "${!blend_var}" "${!soroswap_var}" "${!phoenix_var}"
}

run_full_vault_flow() {
  local label="$1"
  local before_failures="$TX_FAILURES"

  log "Running Beta-gated flow for $label: Phase 1 deposit"
  run_user_deposit "Phase 1" "$label" "$USER1_KEY" "$USER1_ADDR" "$USER_DEPOSIT_AMOUNT"

  if [[ "$WAIT_FOR_COOLDOWN" == "true" ]]; then
    log "Waiting $COOLDOWN_SLEEP_SECS seconds for $label Phase 1 withdrawal cooldown"
    sleep "$COOLDOWN_SLEEP_SECS"
  fi

  log "Running $label Phase 1 withdrawal"
  run_user_withdraw "Phase 1" "$label" "$USER1_KEY" "$USER1_ADDR" "$USER_WITHDRAW_SHARES"

  log "Running $label Phase 2 manager actions"
  run_manager_actions "$label"

  log "Running $label Phase 3 user deposit after manager actions"
  run_user_deposit "Phase 3" "$label" "$USER2_KEY" "$USER2_ADDR" "$USER_DEPOSIT_AMOUNT"

  if [[ "$WAIT_FOR_COOLDOWN" == "true" ]]; then
    log "Waiting $COOLDOWN_SLEEP_SECS seconds for $label Phase 3 withdrawal cooldown"
    sleep "$COOLDOWN_SLEEP_SECS"
  fi

  log "Running $label Phase 3 withdrawal after manager actions"
  run_user_withdraw "Phase 3" "$label" "$USER2_KEY" "$USER2_ADDR" "$USER_WITHDRAW_SHARES"

  if [[ "$TX_FAILURES" -gt "$before_failures" ]]; then
    log "$label flow completed with $((TX_FAILURES - before_failures)) failed transaction(s)"
    return 1
  fi
  log "$label flow completed successfully"
  return 0
}

write_env_file() {
  {
    printf "NETWORK=%s\n" "$NETWORK"
    printf "TIMESTAMP=%s\n" "$TS"
    printf "SOURCE_ACCOUNT=%s\n" "$SOURCE_ACCOUNT"
    printf "MANAGER_ADDR=%s\n" "$MANAGER_ADDR"
    printf "TRADER_ADDR=%s\n" "$TRADER_ADDR"
    printf "TREASURY_ADDR=%s\n" "$TREASURY_ADDR"
    printf "ADMIN_ADDR=%s\n" "$ADMIN_ADDR"
    printf "USER1_KEY=%s\n" "$USER1_KEY"
    printf "USER1_ADDR=%s\n" "$USER1_ADDR"
    printf "USER2_KEY=%s\n" "$USER2_KEY"
    printf "USER2_ADDR=%s\n" "$USER2_ADDR"
    for label in USDC XLM BTC PYUSD EURC AQUA USTRY; do
      local id_var="${label}_ID"
      local price_var="PRICE_${label}"
      printf "%s_ID=%s\n" "$label" "${!id_var}"
      printf "%s=%s\n" "$price_var" "${!price_var}"
    done
    printf "ASSET_HANDLER_ID=%s\n" "$ASSET_HANDLER_ID"
    printf "ORACLE_ID=%s\n" "$ORACLE_ID"
    printf "FACTORY_ID=%s\n" "$FACTORY_ID"
    for label in ALPHA BETA GAMMA; do
      for suffix in VAULT_ID SHARE_ID BLEND_POOL_ID BLEND_ID SOROSWAP_ROUTER_ID SOROSWAP_ID PHOENIX_SHARE_ID PHOENIX_POOL_ID PHOENIX_ID PORTFOLIO PAIR_ASSET RWA_ASSET STRATEGY_MODE; do
        local var="${label}_${suffix}"
        printf "%s=%s\n" "$var" "${!var}"
      done
    done
    printf "REPORT_FILE=%s\n" "$REPORT_FILE"
  } > "$OUT_ENV"
  cp "$OUT_ENV" "$LATEST_ENV"
}

write_report_header() {
  {
    printf "= Stellar Testnet Protocol Deployment and Testing Report\n"
    printf ":toc:\n:sectnums:\n\n"
    printf 'Generated at `%s UTC` on `%s`.\n\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$NETWORK"
    printf "== Deployment Information\n\n"
    printf "[cols=\"1,1\",options=\"header\"]\n|===\n| Item | Address\n"
    printf '| AssetHandler | `%s`\n' "$ASSET_HANDLER_ID"
    printf '| Oracle | `%s`\n' "$ORACLE_ID"
    printf '| Factory | `%s`\n' "$FACTORY_ID"
    printf '| Manager | `%s`\n' "$MANAGER_ADDR"
    printf '| Trader | `%s`\n' "$TRADER_ADDR"
    printf '| Treasury | `%s`\n' "$TREASURY_ADDR"
    printf '| User 1 | `%s`\n' "$USER1_ADDR"
    printf '| User 2 | `%s`\n' "$USER2_ADDR"
    printf "|===\n\n"
    printf "=== Mock Assets\n\n"
    printf "[cols=\"1,1,1\",options=\"header\"]\n|===\n| Asset | Contract | Initial price, 1e7 scale\n"
    for label in USDC XLM BTC PYUSD EURC AQUA USTRY; do
      local id_var="${label}_ID"
      local price_var="PRICE_${label}"
      printf '| %s | `%s` | `%s`\n' "$label" "${!id_var}" "${!price_var}"
    done
    printf "|===\n\n"
    printf "USDC is the only user deposit asset for every vault. Other assets are manager-listed portfolio assets.\n\n"
    printf "=== Vaults and Strategies\n\n"
    printf "[cols=\"1,1,1,1,1,1\",options=\"header\"]\n|===\n| Vault | Vault Address | Share Token | Blend Strategy | Soroswap Strategy | Phoenix Strategy\n"
    for label in ALPHA BETA GAMMA; do
      local vault_var="${label}_VAULT_ID"
      local share_var="${label}_SHARE_ID"
      local blend_var="${label}_BLEND_ID"
      local soroswap_var="${label}_SOROSWAP_ID"
      local phoenix_var="${label}_PHOENIX_ID"
      printf '| %s | `%s` | `%s` | `%s` | `%s` | `%s`\n' "$label" "${!vault_var}" "${!share_var}" "${!blend_var}" "${!soroswap_var}" "${!phoenix_var}"
    done
    printf "|===\n\n"
    printf "== Vault Configuration\n\n"
    printf '* Entry fee: `%s` bps\n' "$ENTRY_FEE_BPS"
    printf '* Withdraw fee: `%s` bps\n' "$EXIT_FEE_BPS"
    printf '* Management fee: `%s` bps\n' "$MGMT_FEE_BPS"
    printf '* Performance fee: `%s` bps\n' "$PERF_FEE_BPS"
    printf '* Share transferability: `false` for every vault\n'
    printf '* Cooldown: `%s` seconds for every vault\n\n' "$COOLDOWN_SECS"
    printf "NOTE: Beta is deployed and tested first. Alpha and Gamma are deployed only when DEPLOY_ALPHA_GAMMA_AFTER_BETA=true and the Beta flow succeeds.\n\n"
    printf "=== Initial Portfolio Configuration\n\n"
    printf "[cols=\"1,1,1\",options=\"header\"]\n|===\n| Vault | Supported portfolio assets | Strategy allocation at deployment\n"
    printf '| Alpha | `%s` | Blend, Soroswap LP, Phoenix LP deployed and active; initial position value `0`\n' "$ALPHA_PORTFOLIO"
    printf '| Beta | `%s` | Blend, Soroswap LP, Phoenix LP deployed and active; initial position value `0`\n' "$BETA_PORTFOLIO"
    printf '| Gamma | `%s` | Blend deployed and active; initial position value `0`\n' "$GAMMA_PORTFOLIO"
    printf "|===\n\n"
    printf "== Transaction Tracking\n\n"
  } > "$REPORT_FILE"
}

close_tx_table() {
  :
}

append_testing_instructions() {
  {
    printf "\n== Testing Instructions\n\n"
    printf "Source deployment variables:\n\n"
    printf "[source,bash]\n----\nsource %s\n----\n\n" "$OUT_ENV"
    printf "Mint test USDC freely for any user:\n\n"
    printf "[source,bash]\n----\nstellar contract invoke --network %s --source-account <payer> --id \"$USDC_ID\" -- mint --to <G...USER> --amount 1000000000\n----\n\n" "$NETWORK"
    printf "Deposit USDC into a vault:\n\n"
    printf "[source,bash]\n----\nstellar contract invoke --network %s --source-account <user-key> --id \"$BETA_VAULT_ID\" -- deposit --amount 1000000000 --from <G...USER> --asset \"$USDC_ID\" --min-shares-out 0\n----\n\n" "$NETWORK"
    printf "Withdraw shares after the 1 minute cooldown:\n\n"
    printf "[source,bash]\n----\nstellar contract invoke --network %s --source-account <user-key> --id \"$BETA_VAULT_ID\" -- withdraw --share_amount <shares> --from <G...USER> --to <G...USER> --min-base-out 0\n----\n\n" "$NETWORK"
    printf "Manager lending supply example:\n\n"
    printf "[source,bash]\n----\nstellar contract invoke --network %s --source-account \"$MANAGER_SIGNER\" --id \"$BETA_VAULT_ID\" -- execute_op --caller \"$MANAGER_ADDR\" --guard \"$BETA_BLEND_ID\" --fn-name supply --args '[{\"i128\":\"300000000\"}]'\n----\n\n" "$NETWORK"
    printf "Verify NAV and share price:\n\n"
    printf "[source,bash]\n----\nstellar contract invoke --network %s --source-account \"$SOURCE_ACCOUNT\" --id \"$BETA_VAULT_ID\" -- get_nav\nstellar contract invoke --network %s --source-account \"$SOURCE_ACCOUNT\" --id \"$BETA_VAULT_ID\" -- get_share_price\n----\n" "$NETWORK" "$NETWORK"
  } >> "$REPORT_FILE"
}

init_vault_stack_vars() {
  for label in ALPHA BETA GAMMA; do
    for suffix in VAULT_ID SHARE_ID BLEND_POOL_ID BLEND_ID SOROSWAP_ROUTER_ID SOROSWAP_ID PHOENIX_SHARE_ID PHOENIX_POOL_ID PHOENIX_ID PORTFOLIO PAIR_ASSET RWA_ASSET STRATEGY_MODE; do
      printf -v "${label}_${suffix}" "%s" ""
    done
  done
}

append_post_beta_deployment_update() {
  {
    printf "\n== Post-Beta Deployment Update\n\n"
    printf "Beta transaction flow succeeded first. Alpha and Gamma were deployed only after that gate passed.\n\n"
    printf "[cols=\"1,1,1,1,1,1\",options=\"header\"]\n|===\n| Vault | Vault Address | Share Token | Blend Strategy | Soroswap Strategy | Phoenix Strategy\n"
    for label in ALPHA GAMMA; do
      local vault_var="${label}_VAULT_ID"
      local share_var="${label}_SHARE_ID"
      local blend_var="${label}_BLEND_ID"
      local soroswap_var="${label}_SOROSWAP_ID"
      local phoenix_var="${label}_PHOENIX_ID"
      printf '| %s | `%s` | `%s` | `%s` | `%s` | `%s`\n' "$label" "${!vault_var}" "${!share_var}" "${!blend_var}" "${!soroswap_var}" "${!phoenix_var}"
    done
    printf "|===\n\n"
  } >> "$REPORT_FILE"
}

require_cmd stellar
require_cmd rg
require_cmd sha256sum
require_cmd cut

log "Resolving role addresses"
ensure_key "$USER1_KEY"
ensure_key "$USER2_KEY"
MANAGER_ADDR="$(resolve_addr "$MANAGER_ACCOUNT")"
TRADER_ADDR="$(resolve_addr "$TRADER_ACCOUNT")"
TREASURY_ADDR="$(resolve_addr "$TREASURY_ACCOUNT")"
ADMIN_ADDR="$(resolve_addr "$ADMIN_ACCOUNT")"
USER1_ADDR="$(resolve_addr "$USER1_KEY")"
USER2_ADDR="$(resolve_addr "$USER2_KEY")"

log "Building release WASM artifacts"
cargo build --target wasm32-unknown-unknown --release >/dev/null

log "Deploying core protocol contracts"
ASSET_HANDLER_ID="$(deploy_pkg "asset_handler" "asset-handler-${TS}" --admin "$ADMIN_ADDR")"
ORACLE_ID="$(deploy_pkg "oracle" "oracle-${TS}" --admin "$ADMIN_ADDR")"
FACTORY_ID="$(deploy_pkg "factory" "factory-${TS}" --admin "$ADMIN_ADDR" --asset-handler "\"$ASSET_HANDLER_ID\"")"
invoke "$ADMIN_SIGNER" --id "$ASSET_HANDLER_ID" -- set_primary_oracle --caller "$ADMIN_ADDR" --oracle "$ORACLE_ID" >/dev/null

log "Deploying public-mint mock assets"
USDC_ID="$(deploy_mock_asset "USDC" "Mock USD Coin" "USDC")"
XLM_ID="$(deploy_mock_asset "XLM" "Mock Stellar Lumens" "XLM")"
BTC_ID="$(deploy_mock_asset "BTC" "Mock Bitcoin" "BTC")"
PYUSD_ID="$(deploy_mock_asset "PYUSD" "Mock PayPal USD" "PYUSD")"
EURC_ID="$(deploy_mock_asset "EURC" "Mock Euro Coin" "EURC")"
AQUA_ID="$(deploy_mock_asset "AQUA" "Mock Aquarius" "AQUA")"
USTRY_ID="$(deploy_mock_asset "USTRY" "Mock Etherfuse USTRY" "USTRY")"

for label in USDC XLM BTC PYUSD EURC AQUA USTRY; do
  id_var="${label}_ID"
  price_var="PRICE_${label}"
  register_asset_everywhere "${!id_var}"
  set_price "${!id_var}" "${!price_var}"
done

for user in "$USER1_ADDR" "$USER2_ADDR"; do
  invoke "$SOURCE_ACCOUNT" --id "$USDC_ID" -- mint --to "$user" --amount "$USER_MINT_AMOUNT" >/dev/null
done
for label in USDC XLM BTC PYUSD EURC AQUA USTRY; do
  id_var="${label}_ID"
  invoke "$SOURCE_ACCOUNT" --id "${!id_var}" -- mint --to "$MANAGER_ADDR" --amount "$MANAGER_MINT_AMOUNT" >/dev/null
done

init_vault_stack_vars

log "Deploying and configuring Beta vault stack first"
deploy_vault_stack "Beta" "USDC,XLM,PYUSD,EURC,AQUA,USTRY" "XLM" "USTRY"

write_env_file
write_report_header
append_snapshot "Initial Beta state" "$BETA_VAULT_ID" "$BETA_SHARE_ID" "$USER1_ADDR" "$BETA_BLEND_ID" "$BETA_SOROSWAP_ID" "$BETA_PHOENIX_ID"

log "Running full transaction flow for Beta first"
if run_full_vault_flow "Beta"; then
  if [[ "$DEPLOY_ALPHA_GAMMA_AFTER_BETA" == "true" ]]; then
    log "Beta succeeded; deploying Alpha and Gamma"
    deploy_vault_stack "Alpha" "USDC,XLM,BTC" "XLM" "BTC"
    deploy_vault_stack "Gamma" "USDC,USTRY" "USTRY" "USTRY" "blend"
    write_env_file
    append_post_beta_deployment_update
    append_snapshot "Initial Alpha state" "$ALPHA_VAULT_ID" "$ALPHA_SHARE_ID" "$USER1_ADDR" "$ALPHA_BLEND_ID" "$ALPHA_SOROSWAP_ID" "$ALPHA_PHOENIX_ID"
    append_snapshot "Initial Gamma state" "$GAMMA_VAULT_ID" "$GAMMA_SHARE_ID" "$USER1_ADDR" "$GAMMA_BLEND_ID" "$GAMMA_SOROSWAP_ID" "$GAMMA_PHOENIX_ID"
    log "Running Alpha after successful Beta gate"
    run_full_vault_flow "Alpha" || true
    log "Running Gamma with Blend-only strategy"
    run_full_vault_flow "Gamma" || true
  else
    log "Beta succeeded; skipping Alpha and Gamma because DEPLOY_ALPHA_GAMMA_AFTER_BETA is not true"
  fi
else
  log "Beta failed; skipping Alpha and Gamma transaction flows"
fi

close_tx_table
append_testing_instructions

if [[ "$TX_FAILURES" -gt 0 ]]; then
  log "Deployment completed with $TX_FAILURES failed transaction(s)"
else
  log "Deployment and all requested transaction flows completed successfully"
fi
log "Env: $OUT_ENV"
log "Report: $REPORT_FILE"
printf "TESTNET_PROTOCOL_ENV=%s\n" "$OUT_ENV"
printf "TESTNET_PROTOCOL_REPORT=%s\n" "$REPORT_FILE"
