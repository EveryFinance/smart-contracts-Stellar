#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

NETWORK="${NETWORK:-testnet}"
SOURCE_ACCOUNT="${SOURCE_ACCOUNT:-deployer}"
RUN_TS="$(date +%Y%m%d-%H%M%S)"
REPORT_DIR="docs/reports"

mkdir -p "$REPORT_DIR"

log() { printf "[%s] %s\n" "$(date +%H:%M:%S)" "$*" >&2; }

latest_showcase_env() {
  ls -1t deployments/showcase-three-vaults-*.env 2>/dev/null | head -n1
}

build_report() {
  local env_file="$1"
  local report_file="$2"

  # shellcheck disable=SC1090
  source "$env_file"

  cat > "$report_file" <<EOF
# Three-Vault Demo Run Report

- Generated at: \`$(date -u +"%Y-%m-%d %H:%M:%S UTC")\`
- Network: \`${NETWORK:-testnet}\`
- Source account alias: \`${SOURCE_ACCOUNT:-deployer}\`
- Deployment env: \`$env_file\`

## Vault Summary

### Alpha (High Risk)
- Vault: \`${ALPHA_VAULT_ID:-N/A}\`
- Share: \`${ALPHA_SHARE_ID:-N/A}\`
- Guard: \`${ALPHA_GUARD_ID:-N/A}\`
- Deposit tx: ${ALPHA_DEPOSIT_TX:-N/A}
- Withdraw tx: ${ALPHA_WITHDRAW_TX:-N/A}

### Beta (Market Risk)
- Vault: \`${BETA_VAULT_ID:-N/A}\`
- Share: \`${BETA_SHARE_ID:-N/A}\`
- Guard: \`${BETA_GUARD_ID:-N/A}\`
- Deposit tx: ${BETA_DEPOSIT_TX:-N/A}
- Withdraw tx: ${BETA_WITHDRAW_TX:-N/A}

### Gamma (Low Risk, USDC)
- Vault: \`${GAMMA_VAULT_ID:-N/A}\`
- Share: \`${GAMMA_SHARE_ID:-N/A}\`
- Guard: \`${GAMMA_GUARD_ID:-N/A}\`
- Active strategy: \`${GAMMA_ACTIVE_STRATEGY_ID:-${GAMMA_BLEND_STRATEGY_ID:-N/A}}\`
- Active pool: \`${GAMMA_ACTIVE_POOL_ID:-${MOCK_POOL_ID:-N/A}}\`

#### Gamma user flow
- Deposit tx: ${GAMMA_DEPOSIT_TX_V2:-${GAMMA_DEPOSIT_TX:-N/A}}
- Invest tx: ${GAMMA_INVEST_TX_V2:-${GAMMA_INVEST_TX:-N/A}}
- Yield tx: ${GAMMA_YIELD_TX_V2:-${GAMMA_YIELD_TX:-N/A}}
- Withdraw tx: ${GAMMA_WITHDRAW_TX_V2:-${GAMMA_WITHDRAW_TX:-N/A}}

## Presentation References

- Detailed showcase doc: \`docs/vaults_alpha_beta_gamma.md\`
- Deployment file: \`$env_file\`
EOF
}

MODE="${1:-deploy}"
if [[ "$MODE" != "deploy" && "$MODE" != "reuse-latest" ]]; then
  echo "Usage: $0 [deploy|reuse-latest]" >&2
  exit 1
fi

if [[ "$MODE" == "deploy" ]]; then
  log "Running full showcase deployment + validation flow"
  ./scripts/deploy_three_showcase_vaults.sh
fi

ENV_FILE="$(latest_showcase_env)"
if [[ -z "${ENV_FILE:-}" ]]; then
  echo "No showcase deployment env found under deployments/." >&2
  exit 1
fi

REPORT_FILE="${REPORT_DIR}/showcase-demo-${RUN_TS}.md"
build_report "$ENV_FILE" "$REPORT_FILE"

log "Demo run completed"
log "Env: $ENV_FILE"
log "Report: $REPORT_FILE"

echo "SHOWCASE_ENV=$ENV_FILE"
echo "SHOWCASE_REPORT=$REPORT_FILE"
