# Demo Runbook

This runbook gives you a presentation-ready, reproducible flow for the three-vault showcase:

- `Alpha` high-risk multi-asset portfolio
- `Beta` market-risk diversified portfolio
- `Gamma` low-risk USDC yield vault

## Prerequisites

- Stellar CLI installed and configured (`stellar --version`)
- Testnet keys available:
  - `deployer`
  - `demo_user` (created automatically by deploy script if missing)
- Workspace dependencies already built once (recommended)

## One-Command Demo

Run from repo root:

```bash
./scripts/run_showcase_demo.sh deploy
```

What it does:

1. Deploys/configures assets, vaults, guards, and Gamma lending setup.
2. Executes user validation transactions:
   - Alpha: deposit + withdraw
   - Beta: deposit + withdraw
   - Gamma: deposit + invest + yield simulation + withdraw
3. Writes deployment env output under `deployments/`.
4. Generates a presenter-friendly report under `docs/reports/`.

## Report-Only Mode (No New Deployment)

If you already have a recent showcase deployment env:

```bash
./scripts/run_showcase_demo.sh reuse-latest
```

This reuses the newest `deployments/showcase-three-vaults-*.env` and generates a fresh report.

## Outputs

The command prints:

- `SHOWCASE_ENV=<path>`
- `SHOWCASE_REPORT=<path>`

Main references:

- Showcase deep-dive: `docs/vaults_alpha_beta_gamma.md`
- Generated report: `docs/reports/showcase-demo-<timestamp>.md`

## Presenter Checklist

1. Open the generated report and confirm all tx links are present.
2. Open `docs/vaults_alpha_beta_gamma.md` for architecture/context.
3. During demo, show one tx per vault in Stellar Expert:
   - Alpha withdraw
   - Beta withdraw
   - Gamma invest + withdraw
