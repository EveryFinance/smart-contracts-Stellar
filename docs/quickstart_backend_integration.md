# Backend Integration Quickstart (JS/TS)

This guide shows practical ways to integrate protocol contracts from a backend service.

## 1) Load Deployment IDs

Use your deployment env file:

```bash
set -a
source deployments/testnet-20260416-150521.env
set +a
```

For Node apps, mirror these values in `.env`:

```env
STELLAR_NETWORK=testnet
STELLAR_SOURCE=deployer
VAULT_ID=CBVK3IC7ALAA5PRWKIRELDHOT6ZXSM6DZHSXDT5Y3Z5WHHVU46EIDJHE
SHARE_TOKEN_ID=CBD2NZHYBNTN4ECUWVUT7KBTCQOXAK2D6TFDMHZPJIWL2MP56KPQWAN7
ORACLE_ID=CCFACGTFLN3CV4LRLRTPK73VUAS2VXDOPMJAZVE4HNVNVNAMREF3QMD2
FACTORY_ID=CCER4YYGW2GEYAYHC7E2ULUQPV5OLTYXV3GUTBQ5IG62CV5YNWHDSJKV
SOROSWAP_GUARD_ID=CAC5D67PTM7E7W7GZO7J4ENNBTMA443MQZTWHD3YTFRBZWVVMEFVPPPB
PHOENIX_GUARD_ID=CBH4GEYHKXU54D3434M7OKJODM3LNT7ED5P6KYFYS2UG4Q46UJPCGXG3
```

## 2) Option A: Robust CLI Wrapper from Node

This is the fastest reliable path for backend jobs and bots.

```ts
// src/stellarCli.ts
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

export async function invoke(contractId: string, fnAndArgs: string[]) {
  const args = [
    "contract", "invoke",
    "--network", process.env.STELLAR_NETWORK || "testnet",
    "--source-account", process.env.STELLAR_SOURCE || "deployer",
    "--id", contractId,
    "--",
    ...fnAndArgs,
  ];

  const { stdout } = await execFileAsync("stellar", args);
  return stdout.trim();
}
```

Example usage:

```ts
import { invoke } from "./stellarCli";

const nav = await invoke(process.env.VAULT_ID!, ["get_nav"]);
const sharePrice = await invoke(process.env.VAULT_ID!, ["get_share_price"]);
console.log({ nav, sharePrice });
```

Write call example:

```ts
await invoke(process.env.ORACLE_ID!, [
  "set_price",
  "--asset", process.env.BASE_ASSET_ID!,
  "--price", "10000000",
]);
```

## 3) Option B: Generated Type Bindings

Generate bindings from deployed contracts:

```bash
mkdir -p generated
stellar contract bindings typescript \
  --network testnet \
  --contract-id "$VAULT_ID" \
  --output-dir generated/vault

stellar contract bindings typescript \
  --network testnet \
  --contract-id "$ORACLE_ID" \
  --output-dir generated/oracle
```

Then import generated clients in your service. Regenerate when ABI changes.

## 4) Suggested Backend Service Layout

```text
src/
  config.ts               # env parsing
  stellarCli.ts           # invoke wrapper
  protocol/
    vault.ts              # vault-facing methods
    oracle.ts             # oracle admin methods
    factory.ts            # registry reads
  jobs/
    navSnapshotJob.ts     # periodic NAV snapshots
    oracleFreshnessJob.ts # stale-price alerting
```

## 5) Minimal Protocol Service Example

```ts
// src/protocol/vault.ts
import { invoke } from "../stellarCli";

const VAULT_ID = process.env.VAULT_ID!;

export async function getNav() {
  return invoke(VAULT_ID, ["get_nav"]);
}

export async function getSharePrice() {
  return invoke(VAULT_ID, ["get_share_price"]);
}

export async function pauseVault(caller: string) {
  return invoke(VAULT_ID, ["pause", "--caller", caller]);
}
```

## 6) Production Integration Checklist

1. Separate keys by role (`manager`, `trader`, `oracle admin`, `factory admin`).
2. Do not keep secrets in source; use vault/KMS.
3. Add retry/backoff around CLI/RPC calls.
4. Persist tx hashes for every state-changing call.
5. Monitor oracle freshness (`get_max_age_ledgers`, stale read failures).
6. Alert on guard or NAV-trip errors.

## 7) Useful Calls for Monitoring

- `vault.get_nav`
- `vault.get_share_price`
- `vault.get_strategies`
- `oracle.get_max_age_ledgers`
- `factory.get_vault_count`
- `factory.get_vaults`

## 8) References

- Integrator CLI quickstart: `docs/quickstart_integrators.md`
- Architecture: `docs/architecture.md`
- Security model: `docs/security.md`
