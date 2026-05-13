#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# Mainnet deployment script — Elyx Alpha, Beta, Gamma vaults
#
# What this script does
# ---------------------
# 1. Deploys protocol singletons: AssetHandler, ReflectorAdapter, Factory
#    (skip with SINGLETON_ENV=<path> to reuse an existing deployment).
# 2. Registers real mainnet asset SAC addresses in AssetHandler and Factory.
# 3. Sets the ReflectorAdapter as the on-chain price oracle (no manual prices).
# 4. Deploys three vault stacks:
#      Alpha — USDC / XLM / BTC      (Blend + Soroswap + Phoenix)
#      Beta  — USDC / XLM / EURC     (Blend + Soroswap + Phoenix)
#      Gamma — USDC                  (Blend-only)
# 5. Configures each vault: oracle (Reflector adapter), fees, cooldown,
#    portfolio assets, deposit assets, guards, authorized ops.
# 6. Writes a deployment env file and an AsciiDoc report.
#
# Oracle design
# -------------
# Prices come from the Reflector "External CEXs & DEXs" oracle on Stellar
# mainnet (aggregated off-chain + on-chain data, SEP-40 compatible).
# The on-chain ReflectorAdapter contract wraps Reflector's interface and
# exposes the `get_price(asset: Address) -> i128` API expected by the vault.
# No manual price-setting is needed or supported.
#
# ⚠ Blend exploit notice (February 2026)
# ----------------------------------------
# The Blend V1 YieldBlox pool (CBP7NO6F7...) was drained via USTRY oracle
# manipulation. The YieldBlox pool is NOT used here and USTRY is NOT included
# in any vault portfolio. All other Blend pools were unaffected.
# Reference: https://github.com/saariuslystoned/blnd-huntr
#
# What this script does NOT do
# -----------------------------
# - No mock or test contracts
# - No user transaction flows
# - No manual oracle price updates (Reflector handles all pricing)
# - No automatic account funding (mainnet accounts must hold real XLM)
#
# Prerequisites
# -------------
# - stellar CLI configured for mainnet
# - SOURCE_ACCOUNT, MANAGER_ACCOUNT, ADMIN_ACCOUNT in CLI keystore, funded
# - cargo + wasm32-unknown-unknown toolchain installed
#
# Usage
# -----
#   MANAGER_ACCOUNT=my-manager ADMIN_ACCOUNT=my-admin \
#     bash scripts/deploy_mainnet.sh
#
#   # Reuse existing singletons:
#   SINGLETON_ENV=deployments/mainnet-singletons.env \
#   MANAGER_ACCOUNT=my-manager ADMIN_ACCOUNT=my-admin \
#     bash scripts/deploy_mainnet.sh
# ---------------------------------------------------------------------------
set -euo pipefail

NETWORK="mainnet"   # hard-coded — this script only targets mainnet

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# ---------------------------------------------------------------------------
# Role accounts
# ---------------------------------------------------------------------------
SOURCE_ACCOUNT="${SOURCE_ACCOUNT:-deployer}"
MANAGER_ACCOUNT="${MANAGER_ACCOUNT:-$SOURCE_ACCOUNT}"
MANAGER_SIGNER="${MANAGER_SIGNER:-$MANAGER_ACCOUNT}"
TRADER_ACCOUNT="${TRADER_ACCOUNT:-$MANAGER_ACCOUNT}"
TRADER_SIGNER="${TRADER_SIGNER:-$MANAGER_SIGNER}"
TREASURY_ACCOUNT="${TREASURY_ACCOUNT:-$MANAGER_ACCOUNT}"
ADMIN_ACCOUNT="${ADMIN_ACCOUNT:-$MANAGER_ACCOUNT}"
ADMIN_SIGNER="${ADMIN_SIGNER:-$MANAGER_SIGNER}"

# ---------------------------------------------------------------------------
# Fee configuration (basis points)
# ---------------------------------------------------------------------------
ENTRY_FEE_BPS="${ENTRY_FEE_BPS:-0}"
EXIT_FEE_BPS="${EXIT_FEE_BPS:-0}"
MGMT_FEE_BPS="${MGMT_FEE_BPS:-200}"       # 2% annual
PERF_FEE_BPS="${PERF_FEE_BPS:-1000}"      # 10% of gains
COOLDOWN_SECS="${COOLDOWN_SECS:-86400}"   # 24 h default for mainnet

# ---------------------------------------------------------------------------
# Singleton reuse
# ---------------------------------------------------------------------------
SINGLETON_ENV="${SINGLETON_ENV:-}"

# ---------------------------------------------------------------------------
# Retry settings
# ---------------------------------------------------------------------------
STELLAR_RETRY_ATTEMPTS="${STELLAR_RETRY_ATTEMPTS:-6}"
STELLAR_RETRY_SLEEP_SECS="${STELLAR_RETRY_SLEEP_SECS:-30}"

# ---------------------------------------------------------------------------
# Reflector on-chain oracle
#
# Using "External CEXs & DEXs" feed — aggregated off-chain + DEX prices.
# Source: https://developers.stellar.org/docs/data/oracles/oracle-providers
#
# Alternative (on-chain Stellar DEX only):
#   CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M
# ---------------------------------------------------------------------------
REFLECTOR_CONTRACT_ID="${REFLECTOR_CONTRACT_ID:-CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN}"

# Max age (seconds) for a Reflector price to be considered fresh.
# Reflector External/CEX updates every ~5 minutes; 1 h gives ample slack.
REFLECTOR_MAX_AGE_SECS="${REFLECTOR_MAX_AGE_SECS:-3600}"

# ---------------------------------------------------------------------------
# Mainnet asset SAC addresses
#
# Sources:
#   Soroswap token list: https://github.com/soroswap/token-list
#   Blend mainnet.contracts.json: https://github.com/blend-capital/blend-utils
#
# Verify any address with:
#   stellar contract id asset --asset <CODE:ISSUER or native> --network mainnet
# ---------------------------------------------------------------------------

# USDC — Circle (issuer GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN)
USDC_ID="${USDC_ID:-CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75}"

# XLM — native Stellar asset SAC
XLM_ID="${XLM_ID:-CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA}"

# EURC — Circle Euro Coin (issuer GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP)
EURC_ID="${EURC_ID:-CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV}"

# AQUA — Aquarius governance token (issuer GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA)
# NOTE: Verify Reflector has an AQUA price feed before including in a vault portfolio.
AQUA_ID="${AQUA_ID:-CAUIKL3IYGMERDRUN6YSCLWVAKIFG5Q4YJHUKM4S4NJZQIA3BAS6OJPK}"

# BTC — Ultra Capital bridge (issuer GDPJALI4AZKUU2W426U5WKMAT6CN3AJRPIIRYR2YM54TL2GDWO5O2MZM)
# Multiple BTC issuers exist on Stellar; override BTC_ID to use a different one.
# Other common issuers: GAUTUYY2THLF7SGITDFMXJVYH3LBV (interstellar.exchange)
BTC_ID="${BTC_ID:-CAO7DDJNGMOYQPRYDY5JVZ5YEK4UQBSMGLAEWRCUOTRMDSBMGWSAATDZ}"

# ---------------------------------------------------------------------------
# External protocol addresses (informational — needed at execute_op time only)
#
# Soroswap AMM
#   Router:  https://github.com/soroswap/core (mainnet releases)
#   Factory: CA4HEQTL2WPEUYKYKCDOHCDNIV4QHNJ7EL4J4NQ6VADP7SYHVRYZ7AW2
SOROSWAP_ROUTER_ID="${SOROSWAP_ROUTER_ID:-CAG5LRYQ5JVEUI5TEID72EYOVX44TTUJT5BQR2J6J77FH65PCCFAJDDH}"

# Blend lending pools (pass as execute_op arg at runtime, not needed at deploy time)
#   Source: https://docs.blend.capital/mainnet-deployments
#                  https://docs-v1.blend.capital/mainnet-deployments
#
#   V1 Pool Factory:          CCZD6ESMOGMPWH2KRO4O7RGTAPGTUPFWFQBELQSS7ZUK63V3TZWETGAG
#   V1 Backstop:              CAO3AGAMZVRMHITL36EJ2VZQWKYRPWMQAPDQD5YEOF3GIF7T44U4JAL3
#   V1 Fixed XLM-USDC pool:  CDVQVKOY2YSXS2IC7KN6MNASSHPAO7UN2UR2ON4OI2SKMFJNVAMDX6DP
#   ⚠ V1 YieldBlox EXPLOITED (Feb 2026) — do NOT use: CBP7NO6F7FRDHSOFQBT2L2UWYIZ2PU76JKVRYAQTG3KZSQLYAOKIF2WB
#
#   V2 Pool Factory:          CDSYOAVXFY7SM5S64IZPPPYB4GVGGLMQVFREPSQQEZVIWXX5R23G4QSU
#   V2 Backstop:              CAQQR5SWBXKIGZKPBZDH3KM5GQ5GUTPKB7JAFCINLZBC5WXPJKRG3IM7
#   V2 Fixed XLM-USDC pool:  CAJJZSGMMM3PD7N33TAPHGBUGTB43OC73HVIK2L2G6BNGGGYOSSYBXBD
#   V2 YieldBlox pool:        CCCCIQSDILITHMM7PBSLVDT5MISSY7R26MNZXCX4H7J5JQ5FPIYOGYFS
#
# Compatible with V1 and V2. Default to V2 Fixed pool (XLM/USDC).
# Override with BLEND_POOL_ID env var to target a different pool.
BLEND_V1_FIXED_POOL_ID="CDVQVKOY2YSXS2IC7KN6MNASSHPAO7UN2UR2ON4OI2SKMFJNVAMDX6DP"
BLEND_V2_FIXED_POOL_ID="CAJJZSGMMM3PD7N33TAPHGBUGTB43OC73HVIK2L2G6BNGGGYOSSYBXBD"
BLEND_POOL_ID="${BLEND_POOL_ID:-$BLEND_V2_FIXED_POOL_ID}"

# Phoenix DEX pools (pass as execute_op arg at runtime, not needed at deploy time)
#   Source: https://github.com/Phoenix-Protocol-Group/phoenix-contracts/blob/main/scripts/upgrade_mainnet.sh
#
#   Factory:    CB4SVAWJA6TSRNOJZ7W2AWFW46D5VR4ZMFZKDIKXEINZCZEGZCJZCKMI
#   Multihop:   CCLZRD4E72T7JCZCN3P7KNPYNXFYKQCL64ECLX7WP5GNVYPYJGU2IO2G
#
#   XLM/USDC:   CBHCRSVX3ZZ7EGTSYMKPEFGZNWRVCSESQR3UABET4MIW52N4EVU6BIZX
#   XLM/EURC:   CBISULYO5ZGS32WTNCBMEFCNKNSLFXCQ4Z3XHVDP4X4FLPSEALGSY3PS
#   PHO/USDC:   CD5XNKK3B6BEF2N7ULNHHGAMOKZ7P6456BFNIHRF4WNTEDKBRWAE7IAA
#   XLM/PHO:    CBCZGGNOEUZG4CAAE7TGTQQHETZMKUT4OIPFHHPKEUX46U4KXBBZ3GLH
#   USDC/VEUR:  CDQLKNH3725BUP4HPKQKMM7OO62FDVXVTO7RCYPID527MZHJG2F3QBJW
#   USDC/VCHF:  CBW5G5SO5SDYUGQVU7RMZ2KJ34POM3AMODOBIV2RQYG4KJDUUBVC3P2T
#   XLM/USDX:   CDMXKSLG5GITGFYERUW2MRYOBUQCMRT2QE5Y4PU3QZ53EBFWUXAXUTBC
#   EURX/USDC:  CC6MJZN3HFOJKXN42ANTSCLRFOMHLFXHWPNAX64DQNUEBDMUYMPHASAV
#   XLM/EURX:   CB5QUVK5GS3IU23TMFZQ3P5J24YBBZP5PHUQAEJ2SP5K55PFTJRUQG2L
#   XLM/GBPX:   CCKOC2LJTPDBKDHTL3M5UO7HFZ2WFIHSOKCELMKQP3TLCIVUBKOQL4HB
#   GBPX/USDC:  CCUCE5H5CKW3S7JBESGCES6ZGDMWLNRY3HOFET3OH33MXZWKXNJTKSM3
#
# Alpha vault uses XLM/USDC; Beta vault uses XLM/EURC.
# Override with PHOENIX_ALPHA_POOL_ID / PHOENIX_BETA_POOL_ID env vars.
PHOENIX_ALPHA_POOL_ID="${PHOENIX_ALPHA_POOL_ID:-CBHCRSVX3ZZ7EGTSYMKPEFGZNWRVCSESQR3UABET4MIW52N4EVU6BIZX}"
PHOENIX_BETA_POOL_ID="${PHOENIX_BETA_POOL_ID:-CBISULYO5ZGS32WTNCBMEFCNKNSLFXCQ4Z3XHVDP4X4FLPSEALGSY3PS}"

# ---------------------------------------------------------------------------
# Output files
# ---------------------------------------------------------------------------
TS="$(date -u +%Y%m%d-%H%M%S)"
OUT_DIR="$ROOT_DIR/deployments"
REPORT_DIR="$ROOT_DIR/docs/reports"
OUT_ENV="$OUT_DIR/mainnet-${TS}.env"
LATEST_ENV="$OUT_DIR/mainnet.latest.env"
REPORT_FILE="$REPORT_DIR/mainnet-${TS}.adoc"
mkdir -p "$OUT_DIR" "$REPORT_DIR"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

log() {
  printf "[%s] %s\n" "$(date -u +%H:%M:%S)" "$*" >&2
}

die() {
  printf "ERROR: %s\n" "$*" >&2
  exit 1
}

stellar_retry() {
  local attempt out_file err_file
  out_file="$(mktemp)"
  err_file="$(mktemp)"
  for attempt in $(seq 1 "$STELLAR_RETRY_ATTEMPTS"); do
    if stellar "$@" >"$out_file" 2>"$err_file"; then
      cat "$err_file" >&2
      cat "$out_file"
      rm -f "$out_file" "$err_file"
      return 0
    fi
    local status=$?
    cat "$err_file" >&2
    if [[ "$attempt" -lt "$STELLAR_RETRY_ATTEMPTS" ]] &&
      rg -qi 'request timeout|timeout|temporarily unavailable|rate limit|connection reset|deadline|dns error' "$err_file"; then
      log "Retrying after transient error (attempt $attempt/$STELLAR_RETRY_ATTEMPTS)"
      sleep "$STELLAR_RETRY_SLEEP_SECS"
      : >"$out_file" && : >"$err_file"
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

sac_address() {
  # Compute the SAC address for a Stellar classic asset or native XLM.
  # Usage: sac_address "USDC:GISSUER..." or sac_address "native"
  stellar contract id asset --asset "$1" --network "$NETWORK" 2>/dev/null \
    | tr -d '"' \
    || die "Failed to compute SAC address for asset: $1"
}

deploy_pkg() {
  local pkg="$1" alias="$2"; shift 2
  log "Deploying $pkg as $alias"
  stellar_retry contract deploy \
    --network "$NETWORK" --source-account "$SOURCE_ACCOUNT" \
    --alias "$alias" --package "$pkg" -- "$@"
}

deploy_pkg_salted() {
  local pkg="$1" alias="$2" salt="$3"; shift 3
  log "Deploying $pkg as $alias (deterministic salt)"
  stellar_retry contract deploy \
    --network "$NETWORK" --source-account "$SOURCE_ACCOUNT" \
    --alias "$alias" --package "$pkg" --salt "$salt" -- "$@"
}

contract_id_for_salt() {
  stellar contract id wasm --source-account "$SOURCE_ACCOUNT" --salt "$1"
}

salt_for() {
  printf "%s" "$1" | sha256sum | cut -d' ' -f1
}

invoke() {
  local signer="$1"; shift
  stellar_retry contract invoke --network "$NETWORK" --source-account "$signer" "$@"
}

register_asset() {
  # Register asset in AssetHandler (NAV tracking) and Factory (whitelist).
  local asset="$1"
  invoke "$ADMIN_SIGNER" --id "$ASSET_HANDLER_ID" \
    -- add_asset --caller "$ADMIN_ADDR" --asset "$asset" >/dev/null
  invoke "$ADMIN_SIGNER" --id "$FACTORY_ID" \
    -- add_authorized_asset --caller "$ADMIN_ADDR" --asset "$asset" >/dev/null
}

authorize_guard() {
  invoke "$ADMIN_SIGNER" --id "$FACTORY_ID" \
    -- add_authorized_guard --caller "$ADMIN_ADDR" --guard "$1" >/dev/null
}

vault_params_json() {
  local label="$1" share="$2" share_admin="$3"
  printf '{"admin":"%s","manager":"%s","manager_name":"%s","trader":"%s","base_asset":"%s","share_token":"%s","share_token_admin":"%s","treasury":"%s","entry_fee_bps":%s,"exit_fee_bps":%s,"mgmt_fee_bps":%s,"perf_fee_bps":%s,"factory":"%s","is_private":false}' \
    "$ADMIN_ADDR" "$MANAGER_ADDR" "$label Vault" "$TRADER_ADDR" \
    "$USDC_ID" "$share" "$share_admin" "$TREASURY_ADDR" \
    "$ENTRY_FEE_BPS" "$EXIT_FEE_BPS" "$MGMT_FEE_BPS" "$PERF_FEE_BPS" \
    "$FACTORY_ID"
}

# ---------------------------------------------------------------------------
# Vault stack deployment
#
# Parameters:
#   label         — Alpha | Beta | Gamma
#   assets_csv    — comma-separated asset variable names, e.g. "USDC,XLM,BTC"
#   strategy_mode — "all" (Blend + Soroswap + Phoenix) | "blend" (Blend only)
# ---------------------------------------------------------------------------
deploy_vault_stack() {
  local label="$1" assets_csv="$2" strategy_mode="${3:-all}"
  local upper="${label^^}" lower="${label,,}"
  local share_id vault_id predicted_vault_id vault_salt params
  local blend_id="" soroswap_id="" phoenix_id=""

  # Share token ──────────────────────────────────────────────────────────────
  share_id="$(deploy_pkg "share-token" "${lower}-share-${TS}" \
    --admin "$MANAGER_ADDR" \
    --name "$label Vault Share" \
    --symbol "${upper}SH" \
    --decimals 7)"

  # Vault (deterministic address) ────────────────────────────────────────────
  vault_salt="$(salt_for "mainnet:${SOURCE_ACCOUNT}:${TS}:${lower}:vault")"
  predicted_vault_id="$(contract_id_for_salt "$vault_salt")"
  [[ -n "$predicted_vault_id" ]] || die "Failed to predict vault id for $label"

  invoke "$MANAGER_SIGNER" --id "$share_id" \
    -- set_admin --new-admin "$predicted_vault_id" >/dev/null

  params="$(vault_params_json "$label" "$share_id" "$predicted_vault_id")"
  vault_id="$(deploy_pkg_salted "vault" "${lower}-vault-${TS}" "$vault_salt" \
    --params "$params")"

  [[ "$vault_id" == "$predicted_vault_id" ]] || \
    die "Vault id mismatch: predicted=$predicted_vault_id actual=$vault_id"

  # Post-deploy vault configuration ──────────────────────────────────────────
  # Oracle — admin only; points to our ReflectorAdapter
  invoke "$ADMIN_SIGNER" --id "$vault_id" \
    -- set_oracle --caller "$ADMIN_ADDR" --oracle "$REFLECTOR_ADAPTER_ID" >/dev/null

  invoke "$MANAGER_SIGNER" --id "$vault_id" \
    -- set_exit_cooldown_secs --caller "$MANAGER_ADDR" --secs "$COOLDOWN_SECS" >/dev/null

  # Share transfers disabled by default (admin can enable later via governance)
  invoke "$ADMIN_SIGNER" --id "$vault_id" \
    -- set_share_transfers_enabled --caller "$ADMIN_ADDR" --enabled false >/dev/null

  invoke "$ADMIN_SIGNER" --id "$FACTORY_ID" \
    -- verify_and_register_vault --caller "$ADMIN_ADDR" --vault "$vault_id" >/dev/null

  # Portfolio + deposit assets ───────────────────────────────────────────────
  IFS=',' read -ra asset_labels <<< "$assets_csv"
  for asset_label in "${asset_labels[@]}"; do
    asset_label="$(printf "%s" "$asset_label" | xargs)"
    local id_var="${asset_label}_ID"
    [[ -n "${!id_var:-}" ]] || die "Asset ID variable ${id_var} is empty for $label vault"
    invoke "$MANAGER_SIGNER" --id "$vault_id" \
      -- add_portfolio_asset --caller "$MANAGER_ADDR" --asset "${!id_var}" >/dev/null
  done
  invoke "$MANAGER_SIGNER" --id "$vault_id" \
    -- add_deposit_asset --caller "$MANAGER_ADDR" --asset "$USDC_ID" >/dev/null

  # Blend strategy ───────────────────────────────────────────────────────────
  # Pool address is NOT needed at deploy time — passed per execute_op call.
  blend_id="$(deploy_pkg "blend-strategy" "${lower}-blend-${TS}")"
  invoke "$MANAGER_SIGNER" --id "$blend_id" \
    -- initialize --vault "$vault_id" --name "$label USDC Lending" >/dev/null

  # Soroswap + Phoenix strategies (strategy_mode == "all") ───────────────────
  if [[ "$strategy_mode" == "all" ]]; then
    soroswap_id="$(deploy_pkg "soroswap-lp-strategy" "${lower}-soroswap-lp-${TS}")"
    invoke "$MANAGER_SIGNER" --id "$soroswap_id" \
      -- initialize \
      --vault "$vault_id" \
      --router "$SOROSWAP_ROUTER_ID" \
      --name "$label Soroswap LP" >/dev/null

    # Pool address passed at execute_op time — not needed here.
    phoenix_id="$(deploy_pkg "phoenix-lp-strategy" "${lower}-phoenix-lp-${TS}")"
    invoke "$MANAGER_SIGNER" --id "$phoenix_id" \
      -- initialize --vault "$vault_id" --name "$label Phoenix LP" >/dev/null
  fi

  # Guard registration + authorized ops ─────────────────────────────────────
  for guard in "$blend_id" "$soroswap_id" "$phoenix_id"; do
    [[ -z "$guard" ]] && continue
    authorize_guard "$guard"
    invoke "$MANAGER_SIGNER" --id "$vault_id" \
      -- add_active_guard --caller "$MANAGER_ADDR" --guard "$guard" >/dev/null
  done

  invoke "$MANAGER_SIGNER" --id "$vault_id" \
    -- set_authorized_ops --caller "$MANAGER_ADDR" --guard "$blend_id" \
    --ops '["supply","withdraw_from_lending"]' >/dev/null

  if [[ "$strategy_mode" == "all" ]]; then
    invoke "$MANAGER_SIGNER" --id "$vault_id" \
      -- set_authorized_ops --caller "$MANAGER_ADDR" --guard "$soroswap_id" \
      --ops '["swap","add_liquidity","remove_liquidity"]' >/dev/null
    invoke "$MANAGER_SIGNER" --id "$vault_id" \
      -- set_authorized_ops --caller "$MANAGER_ADDR" --guard "$phoenix_id" \
      --ops '["swap","add_liquidity","remove_liquidity"]' >/dev/null
  fi

  # Export ───────────────────────────────────────────────────────────────────
  printf -v "${upper}_VAULT_ID"      "%s" "$vault_id"
  printf -v "${upper}_SHARE_ID"      "%s" "$share_id"
  printf -v "${upper}_BLEND_ID"      "%s" "$blend_id"
  printf -v "${upper}_SOROSWAP_ID"   "%s" "$soroswap_id"
  printf -v "${upper}_PHOENIX_ID"    "%s" "$phoenix_id"
  printf -v "${upper}_PORTFOLIO"     "%s" "$assets_csv"
  printf -v "${upper}_STRATEGY_MODE" "%s" "$strategy_mode"

  log "$label vault deployed: $vault_id"
}

init_vault_stack_vars() {
  for label in ALPHA BETA GAMMA; do
    for suffix in VAULT_ID SHARE_ID BLEND_ID SOROSWAP_ID PHOENIX_ID PORTFOLIO STRATEGY_MODE; do
      printf -v "${label}_${suffix}" "%s" ""
    done
  done
}

# ---------------------------------------------------------------------------
# Env + report writers
# ---------------------------------------------------------------------------
write_env_file() {
  {
    printf "NETWORK=%s\n"          "$NETWORK"
    printf "TIMESTAMP=%s\n"        "$TS"
    printf "SOURCE_ACCOUNT=%s\n"   "$SOURCE_ACCOUNT"
    printf "MANAGER_ADDR=%s\n"     "$MANAGER_ADDR"
    printf "TRADER_ADDR=%s\n"      "$TRADER_ADDR"
    printf "TREASURY_ADDR=%s\n"    "$TREASURY_ADDR"
    printf "ADMIN_ADDR=%s\n"       "$ADMIN_ADDR"
    printf "\n# Protocol singletons\n"
    printf "ASSET_HANDLER_ID=%s\n"      "$ASSET_HANDLER_ID"
    printf "REFLECTOR_ADAPTER_ID=%s\n"  "$REFLECTOR_ADAPTER_ID"
    printf "FACTORY_ID=%s\n"            "$FACTORY_ID"
    printf "\n# Mainnet asset SAC addresses\n"
    for label in USDC XLM EURC AQUA BTC; do
      printf "%s_ID=%s\n" "$label" "${!label_ID:-}" 2>/dev/null || true
    done
    printf "USDC_ID=%s\n"  "$USDC_ID"
    printf "XLM_ID=%s\n"   "$XLM_ID"
    printf "EURC_ID=%s\n"  "$EURC_ID"
    printf "AQUA_ID=%s\n"  "$AQUA_ID"
    printf "BTC_ID=%s\n"   "$BTC_ID"
    printf "\n# External protocol addresses (runtime reference)\n"
    printf "REFLECTOR_CONTRACT_ID=%s\n" "$REFLECTOR_CONTRACT_ID"
    printf "SOROSWAP_ROUTER_ID=%s\n"    "$SOROSWAP_ROUTER_ID"
    printf "BLEND_V1_FIXED_POOL_ID=%s\n" "$BLEND_V1_FIXED_POOL_ID"
    printf "BLEND_V2_FIXED_POOL_ID=%s\n" "$BLEND_V2_FIXED_POOL_ID"
    printf "BLEND_POOL_ID=%s\n"          "$BLEND_POOL_ID"
    printf "PHOENIX_ALPHA_POOL_ID=%s\n" "${PHOENIX_ALPHA_POOL_ID:-}"
    printf "PHOENIX_BETA_POOL_ID=%s\n"  "${PHOENIX_BETA_POOL_ID:-}"
    printf "\n# Vault stacks\n"
    for label in ALPHA BETA GAMMA; do
      for suffix in VAULT_ID SHARE_ID BLEND_ID SOROSWAP_ID PHOENIX_ID PORTFOLIO STRATEGY_MODE; do
        local var="${label}_${suffix}"
        printf "%s=%s\n" "$var" "${!var:-}"
      done
      printf "\n"
    done
    printf "REPORT_FILE=%s\n" "$REPORT_FILE"
  } > "$OUT_ENV"
  cp "$OUT_ENV" "$LATEST_ENV"
  log "Env written to $OUT_ENV"
}

write_report() {
  {
    printf "= Elyx Mainnet Deployment Report\n:toc:\n:sectnums:\n\n"
    printf 'Generated at `%s UTC`.\n\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"

    printf "== Roles\n\n"
    printf "[cols=\"1,1\"]\n|===\n| Role | Address\n"
    printf '| Manager  | `%s`\n' "$MANAGER_ADDR"
    printf '| Trader   | `%s`\n' "$TRADER_ADDR"
    printf '| Treasury | `%s`\n' "$TREASURY_ADDR"
    printf '| Admin    | `%s`\n' "$ADMIN_ADDR"
    printf "|===\n\n"

    printf "== Protocol Singletons\n\n"
    printf "[cols=\"1,1\"]\n|===\n| Contract | Address\n"
    printf '| AssetHandler      | `%s`\n' "$ASSET_HANDLER_ID"
    printf '| ReflectorAdapter  | `%s`\n' "$REFLECTOR_ADAPTER_ID"
    printf '| Factory           | `%s`\n' "$FACTORY_ID"
    printf '| Reflector (upstream) | `%s`\n' "$REFLECTOR_CONTRACT_ID"
    printf "|===\n\n"

    printf "== Mainnet Asset Addresses\n\n"
    printf "[cols=\"1,1,1\"]\n|===\n| Asset | SAC Address | Source\n"
    printf '| USDC  | `%s` | Circle / Soroswap token list\n' "$USDC_ID"
    printf '| XLM   | `%s` | Native SAC / Blend mainnet.contracts.json\n' "$XLM_ID"
    printf '| EURC  | `%s` | Circle / Soroswap token list\n' "$EURC_ID"
    printf '| AQUA  | `%s` | Aquarius / Soroswap token list\n' "$AQUA_ID"
    printf '| BTC   | `%s` | Ultra Capital bridge / Soroswap token list\n' "$BTC_ID"
    printf "|===\n\n"

    printf "== Oracle\n\n"
    printf "Prices sourced from Reflector (External CEXs + DEXs feed, SEP-40).\n"
    printf "No manual price updates required.\n\n"
    printf "[cols=\"1,1\"]\n|===\n| Item | Value\n"
    printf '| Reflector upstream  | `%s`\n' "$REFLECTOR_CONTRACT_ID"
    printf '| ReflectorAdapter    | `%s`\n' "$REFLECTOR_ADAPTER_ID"
    printf '| Price freshness max | %s seconds\n' "$REFLECTOR_MAX_AGE_SECS"
    printf "|===\n\n"

    printf "== Vault Configuration\n\n"
    printf '* Entry fee: `%s` bps\n' "$ENTRY_FEE_BPS"
    printf '* Exit fee: `%s` bps\n' "$EXIT_FEE_BPS"
    printf '* Management fee: `%s` bps\n' "$MGMT_FEE_BPS"
    printf '* Performance fee: `%s` bps\n' "$PERF_FEE_BPS"
    printf '* Exit cooldown: `%s` seconds\n' "$COOLDOWN_SECS"
    printf '* Share transfers: disabled at deploy\n\n'

    printf "== Deployed Vault Stacks\n\n"
    printf "[cols=\"1,1,1,1,1,1\"]\n|===\n| Vault | Address | Share Token | Blend | Soroswap | Phoenix\n"
    for label in ALPHA BETA GAMMA; do
      printf '| %s | `%s` | `%s` | `%s` | `%s` | `%s`\n' \
        "$label" "${!label_VAULT_ID:-}" "" "" "" "" 2>/dev/null || true
    done
    for label in ALPHA BETA GAMMA; do
      local v="${label}_VAULT_ID" s="${label}_SHARE_ID"
      local b="${label}_BLEND_ID" so="${label}_SOROSWAP_ID" p="${label}_PHOENIX_ID"
      printf '| %s | `%s` | `%s` | `%s` | `%s` | `%s`\n' \
        "$label" "${!v:-}" "${!s:-}" "${!b:-}" "${!so:-}" "${!p:-}"
    done
    printf "|===\n\n"

    printf "== Portfolio Assets\n\n"
    printf "[cols=\"1,1,1\"]\n|===\n| Vault | Portfolio | Strategies\n"
    printf '| Alpha | `%s` | Blend, Soroswap LP, Phoenix LP\n' "$ALPHA_PORTFOLIO"
    printf '| Beta  | `%s` | Blend, Soroswap LP, Phoenix LP\n' "$BETA_PORTFOLIO"
    printf '| Gamma | `%s` | Blend only\n'                      "$GAMMA_PORTFOLIO"
    printf "|===\n\n"

    printf "== External Protocol Reference\n\n"
    printf "These addresses are NOT deployed by this script. Pass them as args to execute_op.\n\n"
    printf "[cols=\"1,1\"]\n|===\n| Protocol | Address\n"
    printf '| Soroswap Router          | `%s`\n' "$SOROSWAP_ROUTER_ID"
    printf '| Blend V1 Factory         | `CCZD6ESMOGMPWH2KRO4O7RGTAPGTUPFWFQBELQSS7ZUK63V3TZWETGAG`\n'
    printf '| Blend V1 Backstop        | `CAO3AGAMZVRMHITL36EJ2VZQWKYRPWMQAPDQD5YEOF3GIF7T44U4JAL3`\n'
    printf '| Blend V1 Fixed XLM-USDC  | `%s`\n' "$BLEND_V1_FIXED_POOL_ID"
    printf '| Blend V2 Factory         | `CDSYOAVXFY7SM5S64IZPPPYB4GVGGLMQVFREPSQQEZVIWXX5R23G4QSU`\n'
    printf '| Blend V2 Backstop        | `CAQQR5SWBXKIGZKPBZDH3KM5GQ5GUTPKB7JAFCINLZBC5WXPJKRG3IM7`\n'
    printf '| Blend V2 Fixed XLM-USDC  | `%s`\n' "$BLEND_V2_FIXED_POOL_ID"
    printf '| Blend V2 YieldBlox pool  | `CCCCIQSDILITHMM7PBSLVDT5MISSY7R26MNZXCX4H7J5JQ5FPIYOGYFS`\n'
    printf '| Active Blend pool (default V2 Fixed) | `%s`\n' "$BLEND_POOL_ID"
    printf '| Phoenix Factory          | `CB4SVAWJA6TSRNOJZ7W2AWFW46D5VR4ZMFZKDIKXEINZCZEGZCJZCKMI`\n'
    printf '| Phoenix Multihop         | `CCLZRD4E72T7JCZCN3P7KNPYNXFYKQCL64ECLX7WP5GNVYPYJGU2IO2G`\n'
    printf '| Phoenix Alpha Pool (XLM/USDC)  | `%s`\n' "$PHOENIX_ALPHA_POOL_ID"
    printf '| Phoenix Beta Pool (XLM/EURC)   | `%s`\n' "$PHOENIX_BETA_POOL_ID"
    printf "|===\n\n"

    printf "WARNING: Blend V1 YieldBlox pool (CBP7NO6F7...) was exploited in February 2026.\n"
    printf "Do NOT use it. USTRY is not included in any vault portfolio.\n\n"

    printf "== Post-Deploy Checklist\n\n"
    printf "* [ ] Verify all asset SAC addresses on stellar.expert/explorer/public\n"
    printf "* [ ] Confirm Reflector has price feeds for EURC, AQUA, BTC on mainnet\n"
    printf "* [ ] Confirm Soroswap router address at docs.soroswap.finance\n"
    printf "* [ ] Verify Phoenix pool addresses still current at app.phoenix-hub.io\n"
    printf "* [ ] Seed deposit made to each vault (factory seed_deposit)\n"
    printf "* [ ] Test execute_op with small amounts before opening to users\n"
    printf "* [ ] ReflectorAdapter freshness window (%s s) matches Reflector update rate\n" "$REFLECTOR_MAX_AGE_SECS"
  } > "$REPORT_FILE"
}

# ---------------------------------------------------------------------------
# Pre-flight
# ---------------------------------------------------------------------------
require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "Missing required command: $1"
}
require_cmd stellar
require_cmd rg
require_cmd sha256sum
require_cmd cut

log "Resolving role addresses"
MANAGER_ADDR="$(resolve_addr "$MANAGER_ACCOUNT")"
TRADER_ADDR="$(resolve_addr "$TRADER_ACCOUNT")"
TREASURY_ADDR="$(resolve_addr "$TREASURY_ACCOUNT")"
ADMIN_ADDR="$(resolve_addr "$ADMIN_ACCOUNT")"

# Verify + recompute SAC addresses to ensure correctness
log "Verifying mainnet asset SAC addresses"
USDC_COMPUTED="$(sac_address "USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN")"
XLM_COMPUTED="$(sac_address "native")"
EURC_COMPUTED="$(sac_address "EURC:GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP")"
AQUA_COMPUTED="$(sac_address "AQUA:GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA")"
BTC_COMPUTED="$(sac_address "BTC:GDPJALI4AZKUU2W426U5WKMAT6CN3AJRPIIRYR2YM54TL2GDWO5O2MZM")"

for asset in USDC XLM EURC AQUA BTC; do
  local_var="${asset}_ID"
  computed_var="${asset}_COMPUTED"
  if [[ "${!local_var}" != "${!computed_var}" ]]; then
    log "WARNING: ${asset}_ID override (${!local_var}) differs from computed SAC (${!computed_var})"
    log "         Using computed address. Set ${asset}_ID manually to override."
    printf -v "$local_var" "%s" "${!computed_var}"
  fi
done
log "Asset SAC addresses verified."

log "Building release WASM artifacts"
cargo build --target wasm32-unknown-unknown --release >/dev/null

# ---------------------------------------------------------------------------
# Singletons: AssetHandler, ReflectorAdapter, Factory
# ---------------------------------------------------------------------------
if [[ -n "$SINGLETON_ENV" ]]; then
  [[ -f "$SINGLETON_ENV" ]] || die "SINGLETON_ENV not found: $SINGLETON_ENV"
  log "Sourcing singletons from $SINGLETON_ENV"
  # shellcheck source=/dev/null
  source "$SINGLETON_ENV"
else
  log "Deploying AssetHandler"
  ASSET_HANDLER_ID="$(deploy_pkg "asset_handler" "mainnet-asset-handler-${TS}" \
    --admin "$ADMIN_ADDR")"

  log "Deploying ReflectorAdapter (wraps Reflector $REFLECTOR_CONTRACT_ID)"
  REFLECTOR_ADAPTER_ID="$(deploy_pkg "reflector" "mainnet-reflector-adapter-${TS}" \
    --admin "$ADMIN_ADDR" \
    --reflector "$REFLECTOR_CONTRACT_ID")"

  # Set freshness window (default 3600 s)
  invoke "$ADMIN_SIGNER" --id "$REFLECTOR_ADAPTER_ID" \
    -- set_max_age_secs --caller "$ADMIN_ADDR" --secs "$REFLECTOR_MAX_AGE_SECS" >/dev/null

  log "Deploying Factory"
  FACTORY_ID="$(deploy_pkg "factory" "mainnet-factory-${TS}" \
    --admin "$ADMIN_ADDR" \
    --asset-handler "\"$ASSET_HANDLER_ID\"")"

  # Wire ReflectorAdapter as the primary oracle for AssetHandler
  invoke "$ADMIN_SIGNER" --id "$ASSET_HANDLER_ID" \
    -- set_primary_oracle --caller "$ADMIN_ADDR" --oracle "$REFLECTOR_ADAPTER_ID" >/dev/null

  # Register all mainnet assets
  log "Registering mainnet assets with AssetHandler and Factory"
  for asset_id in "$USDC_ID" "$XLM_ID" "$EURC_ID" "$AQUA_ID" "$BTC_ID"; do
    register_asset "$asset_id"
  done
fi

# ---------------------------------------------------------------------------
# Vault stacks
# ---------------------------------------------------------------------------
init_vault_stack_vars

log "Deploying Alpha vault (USDC / XLM / BTC — Blend + Soroswap + Phoenix)"
deploy_vault_stack "Alpha" "USDC,XLM,BTC" "all"

log "Deploying Beta vault (USDC / XLM / EURC — Blend + Soroswap + Phoenix)"
deploy_vault_stack "Beta" "USDC,XLM,EURC" "all"

log "Deploying Gamma vault (USDC — Blend only)"
deploy_vault_stack "Gamma" "USDC" "blend"

# ---------------------------------------------------------------------------
# Outputs
# ---------------------------------------------------------------------------
write_env_file
write_report

log "Mainnet deployment complete"
log "Env   : $OUT_ENV"
log "Report: $REPORT_FILE"
printf "MAINNET_ENV=%s\n"    "$OUT_ENV"
printf "MAINNET_REPORT=%s\n" "$REPORT_FILE"
