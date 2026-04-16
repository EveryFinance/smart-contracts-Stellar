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
8. Contract Docs
   - [Vault](./contracts/vault.md)
   - [Share Token (SEP-41)](./contracts/share_token.md)
   - [Oracle](./contracts/oracle.md)
   - [Factory](./contracts/factory.md)
   - [Blend Strategy](./contracts/blend_strategy.md)
   - [Soroswap LP Strategy](./contracts/soroswap_lp_strategy.md)
   - [Phoenix LP Strategy](./contracts/phoenix_lp_strategy.md)
   - [Soroswap Trade Guard](./contracts/soroswap_trade_guard.md)
   - [Phoenix Trade Guard](./contracts/phoenix_trade_guard.md)

## Scope and Versioning

- Scope: all contracts in `contracts/` except `integration_tests` (which is test-only).
- This documentation reflects the currently deployed/tested architecture after the latest audit-driven remediations:
  - vault/share-token admin handoff hardening,
  - oracle stale-price enforcement,
  - exact-out slippage enforcement in Soroswap guard.

## Deployment References

- Deployment script: `scripts/deploy_testnet.sh`
- Example env config: `scripts/deploy.env.example`
- Latest deployment output is written under `deployments/` as `.env` files.

## Latest Testnet Deployment

Deployment timestamp: `2026-04-16 15:05:21` (file: `deployments/testnet-20260416-150521.env`)

- Share Token: `CBD2NZHYBNTN4ECUWVUT7KBTCQOXAK2D6TFDMHZPJIWL2MP56KPQWAN7`
- Vault: `CBVK3IC7ALAA5PRWKIRELDHOT6ZXSM6DZHSXDT5Y3Z5WHHVU46EIDJHE`
- Oracle: `CCFACGTFLN3CV4LRLRTPK73VUAS2VXDOPMJAZVE4HNVNVNAMREF3QMD2`
- Factory: `CCER4YYGW2GEYAYHC7E2ULUQPV5OLTYXV3GUTBQ5IG62CV5YNWHDSJKV`
- Soroswap Guard: `CAC5D67PTM7E7W7GZO7J4ENNBTMA443MQZTWHD3YTFRBZWVVMEFVPPPB`
- Phoenix Guard: `CBH4GEYHKXU54D3434M7OKJODM3LNT7ED5P6KYFYS2UG4Q46UJPCGXG3`

Explorer links:

- https://stellar.expert/explorer/testnet/contract/CBD2NZHYBNTN4ECUWVUT7KBTCQOXAK2D6TFDMHZPJIWL2MP56KPQWAN7
- https://stellar.expert/explorer/testnet/contract/CBVK3IC7ALAA5PRWKIRELDHOT6ZXSM6DZHSXDT5Y3Z5WHHVU46EIDJHE
- https://stellar.expert/explorer/testnet/contract/CCFACGTFLN3CV4LRLRTPK73VUAS2VXDOPMJAZVE4HNVNVNAMREF3QMD2
- https://stellar.expert/explorer/testnet/contract/CCER4YYGW2GEYAYHC7E2ULUQPV5OLTYXV3GUTBQ5IG62CV5YNWHDSJKV
- https://stellar.expert/explorer/testnet/contract/CAC5D67PTM7E7W7GZO7J4ENNBTMA443MQZTWHD3YTFRBZWVVMEFVPPPB
- https://stellar.expert/explorer/testnet/contract/CBH4GEYHKXU54D3434M7OKJODM3LNT7ED5P6KYFYS2UG4Q46UJPCGXG3
