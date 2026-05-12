# Stellar Production Portfolio Asset Study

Project: Every Finance / Elyx Finance

Study date: `2026-05-12`

This study records the production asset selection for the Alpha, Beta, and Gamma
vaults on Stellar. It separates investment design from implementation, and it
distinguishes assets that are merely tradable from assets that are acceptable for
vault use.

## Final Selected Portfolios

These are the recommended phase-1 production portfolios. Deposits should remain
`USDC` only for all vaults.

| Vault | Objective | Selected Assets | Target Allocation |
|---|---|---|---|
| Alpha | Aggressive volatile portfolio | `XLM`, `BTC`, `USDC` | `50% XLM`, `25% BTC`, `25% USDC` |
| Beta | Stellar DeFi ecosystem portfolio | `USDC`, `XLM`, `PYUSD`, `EURC`, `AQUA`; optional `USTRY` | `40% USDC`, `25% XLM`, `15% PYUSD`, `10% EURC`, `10% AQUA`; if RWA is approved use `35% USDC`, `25% XLM`, `15% PYUSD`, `10% EURC`, `10% AQUA`, `5% USTRY` |
| Gamma | Conservative stable / yield portfolio | `USDC`; optional `USTRY` | `100% USDC`; if RWA is approved use `90% USDC`, `10% USTRY` |

## Selected Asset Identifiers

| Asset | Good Name | Stellar Identifier | Portfolio Use |
|---|---|---|---|
| `XLM` | Stellar Lumens | Native Stellar asset | Alpha core, Beta network liquidity sleeve |
| `USDC` | Circle USD Coin | `USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN` | Base asset, deposits, withdrawal buffer, lending |
| `BTC` | Ultra Capital tethered BTC | `BTC:GDPJALI4AZKUU2W426U5WKMAT6CN3AJRPIIRYR2YM54TL2GDWO5O2MZM` | Alpha volatile sleeve |
| `PYUSD` | PayPal USD | `PYUSD:GDQE7IXJ4HUHV6RQHIUPRJSEZE4DRS5WY577O2FY6YQ5LVWZ7JZTU2V5` | Beta PayFi / stablecoin ecosystem sleeve |
| `EURC` | Circle EURC | `EURC:GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2` | Beta euro stablecoin sleeve |
| `AQUA` | Aquarius liquidity governance token | `AQUA:GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA` | Beta capped Stellar liquidity ecosystem sleeve |
| `USTRY` | Etherfuse US Treasury Notes Stablebond | `USTRY:GCRYUGD5NVARGXT56XEZI5CIFCQETYHAPQQTHO2O3IQZTHDH4LATMYWC` | Optional Beta/Gamma RWA sleeve after legal/oracle review |

## Design Rationale

Alpha should be aggressive, but not reckless. It should use assets with stronger
Stellar liquidity and clear issuer/route assumptions. For phase 1, `XLM`,
`BTC`, and `USDC` are selected. `ETH` remains on the Alpha watchlist because
Aquarius liquidity is better than Horizon native-pool liquidity, but it is still
not selected for launch sizing.

Beta should represent the Stellar DeFi ecosystem, not a diluted Alpha. Therefore
`BTC` and `ETH` are excluded by default. Beta uses:

- `USDC` for Blend lending, base liquidity, and withdrawals.
- `XLM` for native network exposure and the deepest Stellar routing pair.
- `PYUSD` for PayFi / stablecoin ecosystem exposure.
- `EURC` for fiat stablecoin diversification.
- `AQUA` as a capped Aquarius/liquidity-governance sleeve.
- Optional `USTRY` for RWA exposure after legal, issuer, oracle, and route checks.

Gamma should preserve capital. It launches as `USDC` only. `USTRY` can be added
as a capped RWA sleeve after a separate review. Gamma should not hold volatile
assets or commodity tokens by default.

## DEX TVL Review

At the time of review, DeFiLlama ranked `Aquarius Stellar` as the largest Stellar
DEX by TVL.

| DEX | Approx TVL at Review |
|---|---:|
| Aquarius Stellar | `52.6M USD` |
| Stellar DEX | `23.8M USD` |
| LumenSwap | `6.8M USD` |
| Phoenix DeFi Hub | `1.48M USD` |
| Soroswap | `1.20M USD` |
| Scopuly | `1.08M USD` |

Because Aquarius is the largest Stellar DEX by TVL, the final liquidity read
uses both Horizon native SDEX/AMM data and Aquarius AMM API data.

## Aquarius Liquidity Snapshot

Snapshot date: `2026-05-12`

Aquarius reserves are normalized from 7-decimal Stellar token units.

| Pair | Best Aquarius Route | Normalized Reserves | Production Read |
|---|---|---|---|
| `XLM/USDC` | constant-product, `0.0010` fee, gauge enabled | `13,552,530.2704614 XLM` / `2,252,255.5288185 USDC` | Very strong. Main production route. |
| `PYUSD/USDC` | stable pool, `0.0010` fee, gauge enabled | `4,128,734.7322288 PYUSD` / `3,900,378.4895238 USDC` | Strong enough for a capped Beta sleeve. |
| `AQUA/XLM` | constant-product, `0.0030` fee, gauge enabled | `2,140,630,634.363503 AQUA` / `4,527,169.9192923 XLM` | Strongest AQUA route. Supports capped Beta exposure. |
| `AQUA/USDC` | constant-product, `0.0030` fee, gauge enabled | `782,088,096.5950393 AQUA` / `276,194.8640418 USDC` | Strong secondary AQUA route. |
| `USTRY/USDC` | constant-product, `0.0030` fee | `1,024,527.9024710 USTRY` / `1,089,405.3494025 USDC` | Best current RWA route. Use only after legal/oracle approval. |
| `EURC/USDC` | concentrated, `0.0030` fee, gauge enabled | `9,516.4934311 EURC` / `14,526.1488892 USDC` | Tradable but much smaller than PYUSD/USDC and USTRY/USDC. Keep capped. |
| `BTC/USDC` | concentrated, `0.0030` fee, gauge enabled | `0.3654146 BTC` / `37,927.9293684 USDC` | Usable only for small Alpha sizing. |
| `ETH/USDC` | constant-product, `0.0030` fee, gauge enabled | `37.6268150 ETH` / `87,302.6707225 USDC` | Watchlist for Alpha; not selected for Beta. |
| `XAU/USDC` | constant-product, `0.0030` fee | `375,249,401.2598363 XAU` / `879.2177209 USDC` | Tradable, but not approved as RWA. |
| `PALL/XLM` | constant-product, `0.0010` fee | `23,763,446.6669510 PALL` / `1.3189117 XLM` | Route exists but no meaningful vault liquidity. |

## Horizon Native Liquidity Snapshot

Horizon data is useful for native SDEX/orderbook and Stellar native AMM depth,
but it understated some Soroban DEX routes that are deep on Aquarius.

Key observations:

- `XLM/USDC` was strong on both Horizon native AMM and Aquarius.
- `BTC/USDC` existed, but should be size-limited.
- `ETH/USDC` looked too thin on Horizon and only became more plausible after
  checking Aquarius.
- `PYUSD/USDC`, `AQUA/XLM`, and `USTRY/USDC` are much stronger on Aquarius than
  the Horizon-only view suggested.
- `BLND` is important to Stellar DeFi, but current direct DEX liquidity is not
  strong enough for vault holdings. Blend should be represented through lending,
  not BLND token exposure.

## RWA Selection

Selected RWA:

| Asset | Good Name | Identifier | Decision |
|---|---|---|---|
| `USTRY` | Etherfuse US Treasury Notes Stablebond | `USTRY:GCRYUGD5NVARGXT56XEZI5CIFCQETYHAPQQTHO2O3IQZTHDH4LATMYWC` | Preferred RWA sleeve for Beta/Gamma after legal/oracle review. |

RWA watchlist:

| Asset | Good Name | Decision |
|---|---|---|
| `USDY` | Ondo US Dollar Yield | Good product, but not selected until Stellar route depth and compliance assumptions are stronger. Not for U.S. persons. |
| `BENJI` | Franklin OnChain U.S. Government Money Fund / FOBXX | High-quality but permissioned; use only for an institutional/KYC vault. |
| `WTGXX` | WisdomTree Government Money Market Digital Fund | Verify official Stellar access path directly with WisdomTree before use. |
| `deJTRSY` | Centrifuge DeFi Janus Henderson Anemoy Treasury wrapper | Future candidate; wait for official identifier, oracle, and route depth. |
| `deJAAA` | Centrifuge DeFi AAA-rated CLO strategy wrapper | Future candidate; more complex credit risk than Treasury products. |
| `YLDS` | Figure YLDS yield-bearing dollar product | Permissioned and no DEX route found in the snapshot. |

Commodity / metal watchlist:

| Asset | Identifier / Signal | Decision |
|---|---|---|
| `XAU` | `XAU:GBCB4WO6J4ET55RWK2SVX76LUQ4PQ7TCDHG2YFILQML7D6XR3HACLXAU` with `xau.cl` signal | Tradable on Aquarius, but not approved until backing, redemption, legal issuer, and oracle assumptions are verified. |
| `PAXG` | multiple third-party-looking Stellar issuers | Not selected; no official Paxos Stellar issuer verified. |
| `XAUT` | multiple third-party-looking Stellar issuers | Not selected; no official Tether Gold Stellar issuer verified. |
| `PALL`, `SLVR`, `XAG` | metals.bid-style assets | Not selected for default vaults; may fit a separate commodity product after issuer/backing review. |

## Deployment Rules

- Deposit asset: `USDC` only for Alpha, Beta, and Gamma.
- Do not add an asset to `PortfolioAssets` unless it has issuer validation,
  live route liquidity, and `AssetHandler` oracle support.
- LP valuation must decompose reserves and price underlying assets, not opaque
  LP tokens.
- `USTRY`, `XAU`, and any RWA/commodity asset must not be added until legal,
  redemption, and oracle checks are complete.
- Shares remain non-transferable by default. If transfers are enabled, cooldown
  is not a hard control and PnL is informational.

## Sources

- Stellar USDC/EURC overview:
  `https://stellar.org/products-and-tools/circle-usdc-eurc`
- Circle USDC on Stellar:
  `https://www.circle.com/multi-chain-usdc/stellar`
- Circle EURC:
  `https://www.circle.com/eurc`
- Stellar PYUSD launch:
  `https://stellar.org/press/paypal-pyusd-is-now-available-on-stellar`
- Aquarius AMM API:
  `https://amm-api.aqua.network/pools/?size=500`
- DeFiLlama Aquarius Stellar TVL:
  `https://defillama.com/protocol/aquarius-stellar`
- Aquarius AMM rewards:
  `https://docs.aqua.network/aquarius-aqua-rewards/aquarius-amm-rewards`
- Blend lending docs:
  `https://docs.blend.capital/users/lending-borrowing/lending`
- Blend BLND token docs:
  `https://docs.blend.capital/users/blnd-token`
- Etherfuse Stablebonds:
  `https://etherfuse.com/products/stablebonds`
- Etherfuse USTRY product:
  `https://app.etherfuse.com/bonds/USTRY`
- Franklin Templeton BENJI on Stellar:
  `https://www.franklintempleton.com/press-releases/news-room/2026/franklin-templeton-stellar-development-foundation-mark-five-years-of-benji-the-first-u-s-registered-tokenized-money-market-fund`
- WisdomTree WTGXX:
  `https://www.wisdomtree.com/investments/digital-funds/money-market/wtgxx`
- Centrifuge deRWA on Stellar:
  `https://stellar.org/press/centrifuge-brings-derwa-to-stellar-launching-with-usd20m-into-dejtrsy-and-dejaaa`
- Stellar liquidity docs:
  `https://developers.stellar.org/docs/learn/fundamentals/liquidity-on-stellar-sdex-liquidity-pools`
