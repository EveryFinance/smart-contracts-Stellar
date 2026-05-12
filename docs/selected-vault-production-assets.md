# Selected Vault Production Assets

Date: `2026-05-12`

This document records the selected production asset set for the Alpha, Beta, and
Gamma vaults on Stellar.

Full study:
`docs/stellar-production-portfolio-asset-study-2026-05-12.md`

## Final Selection

| Vault | Selected Assets | Allocation |
|---|---|---|
| Alpha | `XLM`, `BTC`, `USDC` | `50% XLM`, `25% BTC`, `25% USDC` |
| Beta | `USDC`, `XLM`, `PYUSD`, `EURC`, `AQUA` | `40% USDC`, `25% XLM`, `15% PYUSD`, `10% EURC`, `10% AQUA` |
| Beta with RWA | `USDC`, `XLM`, `PYUSD`, `EURC`, `AQUA`, `USTRY` | `35% USDC`, `25% XLM`, `15% PYUSD`, `10% EURC`, `10% AQUA`, `5% USTRY` |
| Gamma | `USDC` | `100% USDC` |
| Gamma with RWA | `USDC`, `USTRY` | `90% USDC`, `10% USTRY` |

## Asset Identifiers

| Asset | Good Name | Stellar Identifier |
|---|---|---|
| `XLM` | Stellar Lumens | Native Stellar asset |
| `USDC` | Circle USD Coin | `USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN` |
| `BTC` | Ultra Capital tethered BTC | `BTC:GDPJALI4AZKUU2W426U5WKMAT6CN3AJRPIIRYR2YM54TL2GDWO5O2MZM` |
| `PYUSD` | PayPal USD | `PYUSD:GDQE7IXJ4HUHV6RQHIUPRJSEZE4DRS5WY577O2FY6YQ5LVWZ7JZTU2V5` |
| `EURC` | Circle EURC | `EURC:GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2` |
| `AQUA` | Aquarius liquidity governance token | `AQUA:GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA` |
| `USTRY` | Etherfuse US Treasury Notes Stablebond | `USTRY:GCRYUGD5NVARGXT56XEZI5CIFCQETYHAPQQTHO2O3IQZTHDH4LATMYWC` |

## Deployment Rules

- Deposits should remain `USDC` only for all vaults.
- `USTRY` is optional and requires legal, issuer, redemption, liquidity, and
  oracle review before activation.
- Gold/metal assets are not selected for Alpha, Beta, or Gamma.
- `BTC` is selected for Alpha only.
- `ETH` is not selected for phase-1 launch; keep it on the Alpha watchlist.
- `BLND` is not selected for direct holdings; represent Blend through USDC
  lending.
