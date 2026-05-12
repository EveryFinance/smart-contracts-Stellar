# Protocol Documentation

This folder contains the full technical documentation for the Every Finance protocol, now rebranded to Elyx Finance.

Dapp: https://www.elyx.finance/

## Contents

1. [Architecture](./architecture.md)
2. [Interaction Flows](./interactions.md)
3. [Security Model](./security.md)
4. [Integrator Quickstart](./quickstart_integrators.md)
5. [Backend Integration Quickstart (JS/TS)](./quickstart_backend_integration.md)
6. [Vaults Alpha Beta Gamma](./vaults_alpha_beta_gamma.md)
7. [Demo Runbook](./demo_runbook.md)
8. [Reviewer Quickstart](./reviewer_quickstart.md)
9. [Coverage Report](./coverage_report.md)
10. Contract Docs
   - [Vault](./contracts/vault.md)
   - [Share Token (SEP-41)](./contracts/share_token.md)
   - [Oracle Contracts (AssetHandler, ReflectorAdapter, DIAAdapter, Mock)](./contracts/oracle.md)
   - [Factory](./contracts/factory.md)
   - [Blend Strategy](./contracts/blend_strategy.md)
   - [Soroswap LP Strategy](./contracts/soroswap_lp_strategy.md)
   - [Phoenix LP Strategy](./contracts/phoenix_lp_strategy.md)

## Scope and Versioning

- Scope: all contracts in `contracts/` except `integration_tests` (test-only).
- This documentation reflects the current architecture after all audit-driven and design remediations:
  - Three-tier on-chain oracle (AssetHandler + ReflectorAdapter + DIAAdapter)
  - Split pause controls: `pause_deposits` / `pause_operations` (admin-controlled)
  - Private-pool controls moved to admin (`set_private_pool`, `add_member`, `remove_member`)
  - `is_private` field added to `VaultParams` for construction-time private mode
  - Strategy contracts implement the full guard interface — no separate trade guard contracts
  - `execute_op` replaces `execute_trade`; vault injects its own address as first arg
  - Multi-asset NAV, proportional withdrawal, PnL tracking
  - Vault shares are non-transferable by default; enabling transfers makes
    cooldown non-hard and PnL informational

## Deployment References

- Deployment script: `scripts/deploy_testnet.sh`
- Example env config: `scripts/deploy.env.example`
- Latest deployment output is written under `deployments/` as `.env` files.

## Latest Testnet Deployment

Deployment timestamp: `2026-04-16 15:05:21` (file: `deployments/testnet-20260416-150521.env`)

> Note: the addresses below predate the current architecture revision.
> Redeploy after upgrading to pick up vault admin split, adapter contracts, and AssetHandler.

- Share Token: `CBD2NZHYBNTN4ECUWVUT7KBTCQOXAK2D6TFDMHZPJIWL2MP56KPQWAN7`
- Vault: `CBVK3IC7ALAA5PRWKIRELDHOT6ZXSM6DZHSXDT5Y3Z5WHHVU46EIDJHE`
- Oracle (mock): `CCFACGTFLN3CV4LRLRTPK73VUAS2VXDOPMJAZVE4HNVNVNAMREF3QMD2`
- Factory: `CCER4YYGW2GEYAYHC7E2ULUQPV5OLTYXV3GUTBQ5IG62CV5YNWHDSJKV`
