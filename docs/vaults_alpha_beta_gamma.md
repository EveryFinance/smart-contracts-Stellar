# Vaults Alpha Beta Gamma

Project: Every Finance (rebranded to Elyx Finance)

Dapp: https://www.elyx.finance/

This document records a complete on-chain showcase deployment on **Stellar testnet**.

- Deployment timestamp: `2026-04-16 19:20:20`
- Deployment env file: `deployments/vault_alpha_beta_gamma.latest.env`
- Manager: `GB2HC2NLXR7LHKXGS2IZL4F5LZVQVKRBKCWONQQW4WIYUXDILHORWQPZ`
- Demo user: `GCZZW2O23FN6IULHJF7R3JLZVQ2MCG2TYSQFYPG7WQGWUZFTT7X75RTI`

## Production Vault Design

The deployed testnet assets below are mock/demo assets. The production vault
design should use Stellar mainnet assets only after issuer, liquidity, oracle,
and strategy support have been verified.

Design date: `2026-05-12`

Standalone asset study:
`docs/stellar-production-portfolio-asset-study-2026-05-12.md`

Market context used for the design:

- Circle supports native `USDC` and `EURC` on Stellar.
- Blend is the primary Stellar lending primitive.
- Soroswap, Phoenix, Aquarius, and Sushi provide Stellar DEX/AMM liquidity
  surfaces.
- Aquarius/AQUA is a Stellar-native liquidity incentive and governance layer.
- Stellar DeFi is adding stablecoin, concentrated-liquidity, and RWA/tokenized
  treasury markets such as `PYUSD/USDC`, `USDY/USDC`, and institutional RWA
  collateral.

Reference sources:

- Stellar USDC/EURC overview:
  `https://stellar.org/products-and-tools/circle-usdc-eurc`
- Circle USDC on Stellar:
  `https://www.circle.com/multi-chain-usdc/stellar`
- Circle EURC:
  `https://www.circle.com/eurc`
- StellarExpert USDC asset:
  `https://stellar.expert/explorer/public/asset/USDC-GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN`
- StellarExpert EURC asset:
  `https://stellar.expert/explorer/public/asset/EURC-GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2`
- Stellar PYUSD launch:
  `https://stellar.org/press/paypal-pyusd-is-now-available-on-stellar`
- Etherfuse Stablebonds:
  `https://etherfuse.com/products/stablebonds`
- Etherfuse USTRY product:
  `https://app.etherfuse.com/bonds/USTRY`
- Franklin Templeton BENJI on Stellar:
  `https://www.franklintempleton.com/press-releases/news-room/2026/franklin-templeton-stellar-development-foundation-mark-five-years-of-benji-the-first-u-s-registered-tokenized-money-market-fund`
- WisdomTree WTGXX product:
  `https://www.wisdomtree.com/investments/digital-funds/money-market/wtgxx`
- Centrifuge deRWA on Stellar:
  `https://stellar.org/press/centrifuge-brings-derwa-to-stellar-launching-with-usd20m-into-dejtrsy-and-dejaaa`
- Blend BLND token docs:
  `https://docs.blend.capital/users/blnd-token`
- LOBSTR Ultra Capital BTC asset:
  `https://lobstr.co/assets/BTC%3AGDPJALI4AZKUU2W426U5WKMAT6CN3AJRPIIRYR2YM54TL2GDWO5O2MZM`
- LOBSTR Ultra Capital ETH asset:
  `https://lobstr.co/assets/ETH%3AGBFXOHVAS43OIWNIO7XLRJAHT3BICFEIKOJLZVXNT572MISM4CMGSOCC`
- Blend lending docs:
  `https://docs.blend.capital/users/lending-borrowing/lending`
- Soroswap supported AMMs:
  `https://docs.soroswap.finance/smart-contracts/soroswap-aggregator/supported-amms`
- Aquarius AMM rewards:
  `https://docs.aqua.network/aquarius-aqua-rewards/aquarius-amm-rewards`
- Aquarius AMM API:
  `https://amm-api.aqua.network/pools/?size=500`
- DeFiLlama Aquarius Stellar TVL:
  `https://defillama.com/protocol/aquarius-stellar`
- Stellar SDEX/liquidity-pool docs:
  `https://developers.stellar.org/docs/learn/fundamentals/liquidity-on-stellar-sdex-liquidity-pools`
- Stellar DeFi ecosystem update:
  `https://developers.stellar.org/meetings/2026/04/16`
- Crypto market-cap screening reference:
  `https://www.coingecko.com/`

### Production Asset Universe

The following assets are the preferred production candidates because they are
recognizable Stellar assets with established trading or deposit/withdrawal
routes. Final deployment must still verify current pool depth, oracle coverage,
and issuer/anchor status.

| Asset | Stellar Identifier | Role | Production Use |
|---|---|---|---|
| XLM | Native Stellar asset | Native network asset | Alpha core, Beta growth sleeve |
| USDC | `USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN` | Circle USD stablecoin | Base asset, deposits, lending, withdrawal buffer |
| EURC | `EURC:GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2` | Circle EUR stablecoin | Beta stable diversification, Gamma optional stable sleeve |
| PYUSD | `PYUSD:GDQE7IXJ4HUHV6RQHIUPRJSEZE4DRS5WY577O2FY6YQ5LVWZ7JZTU2V5` | PayPal/Paxos USD stablecoin | Beta PayFi / stablecoin ecosystem sleeve |
| AQUA | `AQUA:GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA` | Aquarius liquidity governance token | Beta capped Stellar DeFi ecosystem sleeve |
| USTRY | `USTRY:GCRYUGD5NVARGXT56XEZI5CIFCQETYHAPQQTHO2O3IQZTHDH4LATMYWC` | Etherfuse US Treasury Notes Stablebond | Gamma/Beta RWA sleeve after legal/oracle checks |
| USDY | `USDY:GAJMPX5NBOG6TQFPQGRABJEEB2YE7RFRLUKJDZAZGAD5GFX4J7TADAZ6` | Ondo US Dollar Yield | RWA watchlist; high issuer quality but thin Stellar DEX liquidity |
| BENJI | `BENJI:GBHNGLLIE3KWGKCHIKMHJ5HVZHYIK7WTBE4QF5PLAKL4CJGSEU7HZIW5` | Franklin OnChain U.S. Government Money Fund / FOBXX | Institutional/direct-access watchlist, not permissionless DEX sleeve |
| BTC | `BTC:GDPJALI4AZKUU2W426U5WKMAT6CN3AJRPIIRYR2YM54TL2GDWO5O2MZM` | Ultra Capital tethered BTC | Alpha core; Beta excluded by default |
| ETH | `ETH:GBFXOHVAS43OIWNIO7XLRJAHT3BICFEIKOJLZVXNT572MISM4CMGSOCC` | Ultra Capital tethered ETH | Alpha watchlist until Stellar DEX liquidity improves |

Excluded from the default production portfolios:

- `SOL`: not selected because the currently visible Stellar SOL candidates are
  not sufficiently trusted for vault use: no verified issuer/domain, no
  deposit/withdraw route, and weak liquidity signals.
- `ETH`: useful long-term Alpha watchlist asset, but not selected for Beta
  phase 1 because live Stellar DEX liquidity is currently too thin.
- `BTC`: useful Alpha asset, but not selected for Beta because Beta is intended
  to represent the Stellar DeFi ecosystem rather than large-cap crypto beta.
- `BLND`: important Blend protocol token, but live Stellar DEX liquidity is too
  thin for direct vault holdings. Beta should represent Blend through USDC
  lending until BLND liquidity improves.
- `WTGXX`: high-quality WisdomTree Government Money Market Digital Fund, but not
  selected until an official Stellar asset identifier and vault-compatible access
  path are verified directly with WisdomTree.
- `deJTRSY` / `deJAAA`: strong Centrifuge RWA candidates for future Stellar DeFi,
  but not selected until official Stellar identifiers, oracle support, and live
  route liquidity are verified.
- `YLDS`: official-looking Figure YLDS asset exists, but it is permissioned and
  has no live DEX route in the snapshot.
- Other bridged or wrapped assets: require the same issuer, redemption,
  liquidity, and oracle checks before inclusion.

### Asset Selection Rules

Each production asset should pass these checks before being added to
`PortfolioAssets`:

- Reputable issuer or native network asset.
- Enough on-chain liquidity for deposits, withdrawals, and rebalances.
- Primary and fallback oracle support through `AssetHandler`.
- Active strategy support where the vault intends to use LP or lending.
- Clear risk classification: stable, volatile, DeFi governance/incentive, RWA,
  or cash buffer.

### Alpha Vault — Aggressive Volatile Portfolio

Objective: maximize upside through volatile Stellar DeFi exposure, accepting
larger drawdowns and higher rebalance risk.

Portfolio review:

- Alpha should use the best-capitalized and most liquid volatile assets available
  through Stellar rails, not only the highest-APY or highest-beta assets.
- BTC and ETH exposure should be core because they are the dominant crypto
  reserve and smart-contract assets by market capitalization and liquidity.
- XLM should be core because it is the native Stellar asset and the deepest
  network beta available inside the ecosystem.
- AQUA should not be a default Alpha asset. It is important to Stellar liquidity
  incentives, but it carries much higher ecosystem-token risk than BTC, ETH, or
  XLM and should remain on the tactical watchlist.
- USDC remains necessary as cash, base asset, lending sleeve, and withdrawal
  buffer.

Target allocation:

| Sleeve | Target | Assets | Strategy Intent |
|---|---:|---|---|
| Bitcoin beta | 35% | Ultra Capital `BTC` | Highest-cap crypto exposure; spot first, LP only if liquidity is deep enough |
| Ethereum beta | 30% | Ultra Capital `ETH` | Large-cap smart-contract exposure; spot first, LP only with reliable oracle/liquidity |
| Stellar beta | 25% | `XLM` | Core native Stellar exposure and XLM/USDC liquidity |
| Cash / rebalance buffer | 10% | `USDC` | Idle liquidity, Blend supply, and withdrawal buffer |

Recommended strategy mix:

- Spot: BTC, ETH, and XLM are the primary Alpha exposures.
- LP: `XLM/USDC`, `BTC/USDC`, and `ETH/USDC` only when pool liquidity,
  slippage, and oracle coverage are sufficient.
- Lending: keep the USDC buffer supplied to Blend when utilization and pool
  liquidity are healthy.
- Rebalancing: wider bands, e.g. rebalance only if a sleeve drifts more than
  7.5% absolute from target.

Risk controls:

- Shares should stay non-transferable.
- Use the strictest `max_loss_bps` that still permits expected LP/slippage
  behavior.
- Small-cap ecosystem tokens are excluded by default. Governance may approve a
  temporary tactical satellite, capped at 5%, only after liquidity and oracle
  review.
- Require explicit manager review before adding any new volatile asset.
- Do not allocate to assets without fallback pricing.

### Beta Vault — Balanced Stellar DeFi Ecosystem Portfolio

Objective: represent the Stellar DeFi ecosystem without becoming a second Alpha
vault. Beta should combine stable liquidity, XLM network exposure, Stellar
liquidity-layer exposure, and payment/stablecoin assets used by Stellar DeFi.

Portfolio review:

- Beta should not hold BTC/ETH by default. Those are large-cap crypto assets,
  but they make Beta look like a lower-risk Alpha instead of a Stellar DeFi
  index.
- USDC should be the largest sleeve because it is the base asset, withdrawal
  buffer, and primary Blend lending asset.
- XLM should be the core volatile sleeve because it is the native Stellar asset
  and the deepest DeFi routing pair.
- AQUA is appropriate for a capped Beta sleeve because Aquarius is a Stellar
  liquidity and incentive layer and AQUA/XLM has materially better native AMM
  liquidity than the current ETH/USDC route.
- PYUSD and EURC add Stellar payment/stablecoin ecosystem exposure without
  turning Beta into a pure crypto beta vault.
- BLND should not be held directly yet despite being a core DeFi token, because
  current live DEX liquidity is too thin. Blend exposure should come from
  lending strategy use.

Target allocation:

| Sleeve | Target | Assets | Strategy Intent |
|---|---:|---|---|
| Core DeFi stable | 40% | Circle `USDC` | Blend lending, cash buffer, base asset |
| Stellar network liquidity | 25% | `XLM` | Core network exposure and XLM/USDC liquidity |
| PayFi / USD stable ecosystem | 15% | `PYUSD` | Stellar stablecoin/payment ecosystem exposure, route through PYUSD/USDC |
| Euro stable ecosystem | 10% | `EURC` | FX diversification and stablecoin liquidity |
| Liquidity-governance ecosystem | 10% | `AQUA` | Capped Aquarius exposure; route through AQUA/XLM or AQUA/USDC only if live liquidity passes |

Recommended strategy mix:

- Lending: primary USDC sleeve through Blend.
- LP: controlled `XLM/USDC`, `AQUA/XLM`, `PYUSD/USDC`, and `EURC/USDC` only
  after liquidity checks.
- Spot: XLM, PYUSD, EURC, and AQUA can be held directly when oracle support is
  available.
- Rebalancing: medium bands, e.g. 5% absolute drift.

Risk controls:

- Shares non-transferable by default.
- Stable assets must have clear issuer, redemption, and oracle assumptions.
- No single non-stable volatile asset should exceed 25% after rebalance.
- AQUA must remain capped at 10% unless governance approves a larger ecosystem
  token sleeve after liquidity/oracle review.
- BLND is watchlist-only until live DEX liquidity materially improves.
- LP exposure should be capped separately from spot exposure.

### Gamma Vault — Conservative Stable / Yield Portfolio

Objective: preserve capital and generate lower-volatility yield from stable and
high-quality collateral markets.

Portfolio review:

- Gamma should not hold volatile assets as strategic exposure.
- USDC should dominate because it is the vault base asset, the deepest stable
  asset for Stellar payments/DeFi, and the cleanest Blend lending sleeve.
- EURC can add fiat diversification, but should remain smaller than USDC.
- Stable-stable LP and RWA/treasury sleeves are optional yield enhancers and
  should be capped because they introduce liquidity, oracle, and issuer risk.

Target allocation:

| Sleeve | Target | Assets | Strategy Intent |
|---|---:|---|---|
| Core stable yield | 70% | `USDC` | Blend lending and withdrawal liquidity |
| Euro stable diversification | 10% | `EURC` | Stable FX diversification |
| Stablecoin LP | 10% | `PYUSD/USDC` or `EURC/USDC` | Low-volatility LP only with deep liquidity and conservative slippage |
| RWA / treasury sleeve | 10% | approved `USTRY`; `USDY` or `deJTRSY` only after liquidity/oracle review | Conservative yield / collateral diversification |

Recommended strategy mix:

- Lending: USDC supply to Blend is the primary strategy.
- LP: stable-stable LP only, with tight slippage and low `max_loss_bps`.
- RWA: add only after issuer/legal, oracle, redemption, and liquidity review.
- Rebalancing: tight bands, e.g. 2.5% absolute drift.

Risk controls:

- Shares should remain non-transferable.
- Avoid volatile assets except small operational dust if needed by tooling.
- Keep a direct USDC withdrawal buffer before allocating to less-liquid RWA.
- Pause new deposits if oracle freshness or stablecoin liquidity deteriorates.

### RWA Asset Selection

RWA assets require a stricter filter than normal crypto assets. A production
vault should not treat issuer AUM as equal to usable on-chain liquidity. The
asset must have a verified issuer, clear legal/redemption terms, oracle support,
and a vault-compatible trading or subscription path.

Selected launch candidate:

| Asset | Good Name | Stellar Identifier | Use |
|---|---|---|---|
| `USTRY` | Etherfuse US Treasury Notes Stablebond | `USTRY:GCRYUGD5NVARGXT56XEZI5CIFCQETYHAPQQTHO2O3IQZTHDH4LATMYWC` | Best current RWA candidate for a small Gamma/Beta sleeve. It has an official Etherfuse domain, Blend/Soroswap ecosystem references, and usable live `USTRY/USDC` orderbook depth. |

Qualified watchlist:

| Asset | Good Name | Identifier / Status | Reason |
|---|---|---|---|
| `USDY` | Ondo US Dollar Yield | `USDY:GAJMPX5NBOG6TQFPQGRABJEEB2YE7RFRLUKJDZAZGAD5GFX4J7TADAZ6` | Strong issuer/product quality, but current Stellar DEX liquidity is thin. Use only after better route depth or direct subscription/redemption support. Not for U.S. persons. |
| `BENJI` | Franklin OnChain U.S. Government Money Fund / FOBXX | `BENJI:GBHNGLLIE3KWGKCHIKMHJ5HVZHYIK7WTBE4QF5PLAKL4CJGSEU7HZIW5` | High-quality regulated money-market fund, but permissioned and not DEX-tradable in the snapshot. Use only for a KYC/authorized institutional vault. |
| `WTGXX` | WisdomTree Government Money Market Digital Fund | Verify directly with WisdomTree before use | Strong product, but no vault-ready official Stellar identifier was selected from public Horizon results. |
| `deJTRSY` | Centrifuge DeFi Janus Henderson Anemoy Treasury Fund wrapper | Pending official Stellar identifier and liquidity verification | Good future RWA candidate; wait for confirmed Stellar launch details, oracle, and DeFi route depth. |
| `deJAAA` | Centrifuge DeFi AAA-rated CLO strategy wrapper | Pending official Stellar identifier and liquidity verification | Good future RWA candidate; higher credit-structure complexity than Treasury products. |
| `YLDS` | Figure YLDS yield-bearing dollar product | `YLDS:GAC7MOPTQLQUM3KC24AW4GHS3RLF72LPEZO54AH7EZ6TSMGRB5SOAVH3` | Official-looking, permissioned, no DEX route in the snapshot. Watchlist only. |

Commodity / precious-metal watchlist:

| Asset | Name / Issuer Signal | Identifier / Route | Reason |
|---|---|---|---|
| `XAU` | `xau.cl` gold-like asset | `XAU:GBCB4WO6J4ET55RWK2SVX76LUQ4PQ7TCDHG2YFILQML7D6XR3HACLXAU`; Aquarius `XAU/USDC` route exists | Tradable on Aquarius with visible pool depth, but not selected until backing, redemption, legal issuer, and oracle assumptions are reviewed. |
| `PAXG` | Paxos Gold name appears on Stellar through multiple third-party-looking issuers | no selected issuer | Not selected. No official Paxos Stellar issuer was verified in this review. |
| `XAUT` | Tether Gold name appears on Stellar through multiple third-party-looking issuers | no selected issuer | Not selected. No official Tether Gold Stellar issuer was verified in this review. |
| `PALL` | metals.bid palladium-like asset | `PALL:GCJXFDVEEFGQYBUCYD6XVEZJEHBD2CDIGBFJWX7UWAMYCO5CRRAMMETL`; Aquarius `PALL/XLM` route exists | Not selected for Gamma; commodity and issuer risk are outside the conservative USD yield mandate. |
| `SLVR` / `XAG` | metals.bid silver-like assets | `SLVR:GDGZD3MUKV7GLIOIY72KUORZFGSJXK3S5OUWKG5WNN3UZPEZVAIFMETL`, `XAG:GAWRG476YFFKLPDTJOPOLSS5BXPCQ7ONVQKGCEYCCD3NXYUKDTB5METL` | Not selected. Some routes exist, but volume/liquidity quality and issuer assumptions are not strong enough for the default vaults. |

Not selected for default Gamma:

- `CETES`: Etherfuse Mexico Treasury Stablebond. Good issuer/product concept, but
  it introduces MXN sovereign/FX exposure and live `CETES/USDC` sell-side depth
  was too thin for a conservative USD vault.
- `TESOURO`: Etherfuse Brazil Treasury Stablebond. Interesting global sovereign
  yield asset, but it introduces BRL sovereign/FX exposure and should be capped
  to a separate global-RWA product, not default Gamma.
- Commodity tokens such as `XAU`, `PALL`, `SLVR`, and `XAG`: tradable routes may
  exist, especially on Aquarius, but they are not selected for Alpha/Beta/Gamma
  until proof of backing, redemption, legal issuer, and robust oracle support are
  verified. These are candidates for a separate commodity vault, not the default
  conservative RWA sleeve.

Recommended RWA allocation:

| Vault | RWA Allocation |
|---|---|
| Beta | Optional `5% USTRY` sleeve, funded from `USDC`, only after oracle and legal review. |
| Gamma | Optional `10% USTRY` sleeve after a liquidity snapshot and direct redemption review; otherwise remain `100% USDC`. |

Do not add RWAs to `DepositAssets`. Deposits should remain `USDC` only; the vault
manager can allocate into the approved RWA sleeve after price, liquidity, and
compliance checks pass.

### Production Implementation Notes

- `USDC` should remain the base asset for all three vaults.
- Alpha default `PortfolioAssets`: `BTC`, `ETH`, `XLM`, `USDC`.
- Beta default `PortfolioAssets`: `USDC`, `XLM`, `PYUSD`, `EURC`, `AQUA`, with
  optional `USTRY` after RWA review.
- Gamma default `PortfolioAssets`: `USDC`, with optional `EURC` and `USTRY`
  after liquidity review.
- `PortfolioAssets` should include every asset that can be held directly by the
  vault.
- `DepositAssets` should be narrower than `PortfolioAssets`; for production,
  start with `USDC` deposits only unless multi-asset deposit UX and slippage
  controls are ready.
- Every production asset must be registered in `AssetHandler` before it is added
  to a vault.
- Every strategy must be factory-authorized, active on the vault, and configured
  with only the required `AuthorizedOps`.
- Do not enable share transfers for these vaults unless the product explicitly
  accepts non-hard cooldown and informational PnL reporting.

### Production Configuration Matrix

Use this matrix as the production deployment target. The vault can be configured
more conservatively than this matrix, but should not be configured more broadly
without a new risk review.

| Vault | Base Asset | DepositAssets | PortfolioAssets | Default Strategy Routes |
|---|---|---|---|---|
| Alpha | `USDC` | `USDC` only | `BTC`, `ETH`, `XLM`, `USDC` | Spot BTC/ETH/XLM; optional `XLM/USDC`, `BTC/USDC`, `ETH/USDC` LP; optional USDC Blend supply |
| Beta | `USDC` | `USDC` only | `USDC`, `XLM`, `PYUSD`, `EURC`, `AQUA`; optional `USTRY` | USDC Blend supply; controlled XLM/USDC, AQUA/XLM, PYUSD/USDC, EURC/USDC, and USTRY/USDC routes |
| Gamma | `USDC` | `USDC` only | `USDC`; optional `EURC` and `USTRY` | USDC Blend supply; optional stable-stable or USTRY sleeve only |

The initial production deposit surface should be `USDC` only. Multi-asset
deposits can be added later, but only when the UI and contracts enforce
slippage, oracle freshness, and minimum shares received in a way users can
understand before signing.

### Strategy Authorization

The vault manager should authorize only the exact functions needed by each
strategy. Do not authorize generic lifecycle, view, initialization, pause, or
administrative functions through `vault.execute_op`.

Alpha:

- DEX trade guard: `swap` only for approved pairs involving `USDC`, `XLM`,
  `BTC`, or `ETH`.
- LP guards: `add_liquidity` and `remove_liquidity` only for approved
  `XLM/USDC`, `BTC/USDC`, and `ETH/USDC` pools.
- Blend guard: `supply` and `withdraw` only for `USDC`.

Beta:

- DEX trade guard: `swap` only for approved pairs involving `USDC`, `XLM`,
  `PYUSD`, `EURC`, `AQUA`, or `USTRY`.
- LP guards: `add_liquidity` and `remove_liquidity` only for `XLM/USDC`,
  `AQUA/XLM`, `PYUSD/USDC`, and `EURC/USDC` after live liquidity checks.
- Blend guard: `supply` and `withdraw` only for `USDC`.

Gamma:

- Blend guard: `supply` and `withdraw` only for `USDC`.
- LP guards are disabled by default. Enable stable-stable LP only after a
  separate pool review.
- DEX trade guard is disabled by default except for emergency conversion back to
  `USDC`.

### Oracle Requirements

Every configured asset must have a working `AssetHandler` price before deposits
open. The production oracle rule is:

- Primary oracle: Reflector adapter where a reliable feed exists.
- Fallback oracle: DIA adapter with an explicit asset to pair-key mapping.
- Per-asset override: allowed for assets where the primary source is not
  available, but the override must be documented and monitored.
- No asset may be added to `PortfolioAssets` if `get_price(asset)` returns zero,
  stale, missing, or non-fallbackable data.
- LP strategy valuation must decompose reserves and price the underlying assets;
  do not rely on opaque LP-token pricing.

Suggested DIA pair keys:

| Asset | DIA Pair Key |
|---|---|
| XLM | `XLM/USD` |
| BTC | `BTC/USD` |
| ETH | `ETH/USD` |
| USDC | `USDC/USD` |
| EURC | `EURC/USD` |
| PYUSD | `PYUSD/USD` |
| AQUA | `AQUA/USD` |
| USTRY | `USTRY/USD` |

### Liquidity Gates

Before a vault is opened for user deposits, governance or the manager should
record a liquidity snapshot for every route used by the vault.

Minimum checks:

- The route must be tradable on a Stellar DEX/AMM used by the strategy.
- A normal rebalance trade must execute within the configured `max_loss_bps`.
- A stressed withdrawal trade must execute without breaking the withdrawal
  buffer assumptions.
- The pool must not be dependent on a single temporary incentive campaign.
- For LP positions, both underlying assets must be in `PortfolioAssets` and
  priced by `AssetHandler`.

Initial recommended risk parameters:

| Vault | Rebalance Band | Max LP Exposure | Suggested `max_loss_bps` |
|---|---:|---:|---:|
| Alpha | 7.5% absolute drift | 35% of NAV | 100 to 150 bps |
| Beta | 5.0% absolute drift | 25% of NAV | 50 to 100 bps |
| Gamma | 2.5% absolute drift | 10% of NAV | 25 to 50 bps |

These limits are intentionally conservative starting points. They should be
tightened when live routes show stable execution, and loosened only through an
explicit governance or manager review.

### Live DEX Liquidity Snapshot

Snapshot date: `2026-05-12`

Source: Stellar Horizon public order books and native constant-product liquidity
pools. This snapshot covers Stellar SDEX order books and Stellar native AMM
pools. Soroban AMM routes such as Soroswap, Phoenix, and Aquarius should be
checked again with an aggregator quote before production because route quality
can change quickly.

| Pair | Native AMM Reserves | Approx AMM TVL | Top-Book Spread | Near-Market Orderbook Depth | Production Read |
|---|---|---:|---:|---|---|
| `XLM/USDC` | `12,692,884.6837280 XLM` / `2,117,492.3175444 USDC` | `4,234,984.64 USDC` | `0.100%` | within 1%: sell `13,075.04 USDC`, buy `26,815.08 USDC`; within 5% buy `242,972.65 USDC` | Strongest production route. Suitable as the main volatile/liquidity pair. |
| `BTC/USDC` | `0.6596689 BTC` / `53,415.0602566 USDC` | `106,830.12 USDC` | `0.200%` | within 1%: sell `1,408,226.29 USDC`, buy `428.33 USDC`; within 5% buy `2,352.19 USDC` | Tradable, but buy-side DEX depth is weak. Use spot exposure only, cap size, and avoid LP as a core strategy. |
| `ETH/USDC` | `0.5070689 ETH` / `1,176.0635537 USDC` | `2,352.13 USDC` | `2.868%` | within 1%: sell `3,048.25 USDC`, buy `4,611.69 USDC`; within 5% buy `18,138.93 USDC` | Too thin for a core production vault sleeve today. Keep on watchlist until liquidity improves. |
| `EURC/USDC` | `2,061.0007486 EURC` / `2,427.9639209 USDC` | `4,855.93 USDC` | `0.189%` | within 1%: sell `39,360.19 USDC`, buy `221.39 USDC`; within 2% buy `19,480.43 USDC` | Useful stable asset, but pool depth is thin and orderbook depth is asymmetric. Use only with small caps. |
| `AQUA/USDC` | `53,179,895.9714821 AQUA` / `18,808.8720024 USDC` | `37,617.74 USDC` | `0.939%` | within 1%: sell `2.91 USDC`, buy `0.34 USDC`; within 5% buy `27,467.18 USDC` | Tradable but asymmetric. Do not rely on this as the only AQUA route. |
| `AQUA/XLM` | `270,579,654.4321280 AQUA` / `572,122.4979440 XLM` | about `190,000 USDC` at snapshot XLM price | `0.251%` | within 1%: sell `411.31 XLM`, buy `186,186.93 XLM`; within 5% buy `525,143.29 XLM` | Strongest route for a capped AQUA sleeve. Better Beta ecosystem fit than ETH. |
| `PYUSD/USDC` | native AMM only `4.2053048 PYUSD` / `4.1927967 USDC`; PYUSD/XLM pool `51.2085503 PYUSD` / `306.3857236 XLM` | orderbook-led | `0.080%` | within 1%: sell `44,623.01 USDC`, buy `13,311.87 USDC` | Good enough for a small stable ecosystem sleeve if oracle and issuer checks pass. |
| `BLND/USDC` | `2,489.9001327 BLND` / `124.0903271 USDC` | `248.18 USDC` | `54.602%` | within 1%: sell `1.92 USDC`, buy `88.67 USDC` | Important protocol token, but not investable for the vault today. Represent Blend through lending, not BLND holdings. |
| `USTRY/USDC` | native AMM only `15.5114229 USTRY` / `16.4956311 USDC`; route is orderbook-led | orderbook-led | `0.090%` | within 1%: sell `227,648.34 USDC`, buy `219,950.92 USDC` | Best current Stellar RWA route for a small vault sleeve. Require legal/oracle review before use. |
| `USDY/USDC` | native AMM only `1.6965635 USDY` / `1.8992659 USDC` | orderbook-led | `0.748%` | within 1%: sell `1,542.15 USDC`, buy `1,283.39 USDC` | Good issuer, but Stellar DEX liquidity is too thin for default vault use today. |
| `BENJI/USDC` | none | none | no book | no live orderbook or AMM route found | High-quality permissioned product, but not usable as a permissionless DEX sleeve. |

### Aquarius Liquidity Snapshot

Snapshot date: `2026-05-12`

Source: Aquarius AMM API and DeFiLlama protocol data. DeFiLlama ranked
`Aquarius Stellar` as the largest Stellar DEX by TVL at snapshot time, ahead of
Stellar DEX, LumenSwap, Phoenix, Soroswap, and Scopuly.

Aquarius API reserves are reported in 7-decimal Stellar token units; the amounts
below are normalized by `1e7`.

| Pair | Best Aquarius Route | Normalized Reserves | Route Read |
|---|---|---|---|
| `XLM/USDC` | constant-product, `0.0010` fee, gauge enabled | `13,552,530.2704614 XLM` / `2,252,255.5288185 USDC` | Very strong. This confirms XLM/USDC is the main production route across both native pools and Aquarius. |
| `PYUSD/USDC` | stable pool, `0.0010` fee, gauge enabled | `4,128,734.7322288 PYUSD` / `3,900,378.4895238 USDC` | Strong on Aquarius. PYUSD is viable for Beta if oracle/issuer checks pass. |
| `AQUA/XLM` | constant-product, `0.0030` fee, gauge enabled | `2,140,630,634.363503 AQUA` / `4,527,169.9192923 XLM` | Strongest AQUA route. Supports capped Beta AQUA exposure. |
| `AQUA/USDC` | constant-product, `0.0030` fee, gauge enabled | `782,088,096.5950393 AQUA` / `276,194.8640418 USDC` | Strong secondary AQUA route. |
| `USTRY/USDC` | constant-product, `0.0030` fee | `1,024,527.9024710 USTRY` / `1,089,405.3494025 USDC` | Strong RWA route. This upgrades USTRY from optional watchlist to the preferred RWA sleeve, subject to legal/oracle approval. |
| `EURC/USDC` | concentrated, `0.0030` fee, gauge enabled | `9,516.4934311 EURC` / `14,526.1488892 USDC` | Tradable but much smaller than PYUSD/USDC and USTRY/USDC. Keep capped. |
| `BTC/USDC` | concentrated, `0.0030` fee, gauge enabled | `0.3654146 BTC` / `37,927.9293684 USDC` | Usable only for small Alpha sizing. Not a Beta ecosystem asset. |
| `ETH/USDC` | constant-product, `0.0030` fee, gauge enabled | `37.6268150 ETH` / `87,302.6707225 USDC` | Better than native Horizon pool, but still an Alpha/watchlist asset rather than Beta. |
| `XAU/USDC` | constant-product, `0.0030` fee | `375,249,401.2598363 XAU` / `879.2177209 USDC` | Tradable, but not approved as RWA until issuer/backing/redemption/oracle review is complete. |
| `PALL/XLM` | constant-product, `0.0010` fee | `23,763,446.6669510 PALL` / `1.3189117 XLM` | Route exists but no meaningful volume; not vault-ready. |

Aquarius-adjusted portfolio impact:

- Beta can keep `PYUSD` as a real ecosystem sleeve because the Aquarius
  `PYUSD/USDC` route is deep enough for a capped production allocation.
- Beta can include `USTRY` as the preferred RWA sleeve, capped at `5%`, after
  legal, issuer, and oracle checks.
- Gamma can include `USTRY` as the preferred conservative RWA sleeve, capped at
  `10%`, after the same RWA checks.
- `EURC` should remain capped because Aquarius liquidity is present but not deep.
- `ETH` can be reconsidered for Alpha if execution uses Aquarius, but it should
  not be added to Beta because Beta is a Stellar DeFi ecosystem vault.

Phase-1 liquidity-gated deployment:

- Alpha should launch with `XLM`, `BTC`, and `USDC` only. Keep `ETH` and `AQUA`
  on the watchlist until DEX/aggregator depth supports the vault size.
- Beta should launch as a Stellar DeFi ecosystem vault: `USDC`, `XLM`, `PYUSD`,
  `EURC`, and capped `AQUA`. It may add up to `5% USTRY` after RWA legal/oracle
  review. It should not hold BTC or ETH by default.
- Gamma should remain `USDC` only at launch. Add `USTRY` only after stable
  liquidity, oracle, and direct redemption checks are completed.

Phase-1 target allocations:

| Vault | Allocation |
|---|---|
| Alpha | `50% XLM`, `25% BTC`, `25% USDC` |
| Beta | `35% USDC`, `25% XLM`, `15% PYUSD`, `10% EURC`, `10% AQUA`, optional `5% USTRY` if all live route, legal, and oracle checks pass; otherwise keep that 5% in `USDC` |
| Gamma | `100% USDC`; optional `90% USDC`, `10% USTRY` after RWA review |

The earlier target allocations remain the long-term investment policy, but this
phase-1 configuration better matches current on-chain DEX liquidity.

## Deployed Asset Set

- USDC (`USD Coin`): `CCIC3B2ATUPEBMQRJYOO624II6S4AKPWORKD4Z47LHXLLR5NC333H5XS`
- WETH (`Wrapped Ether`): `CA5GAWXTV5SGPW3FPS2HAUHS4JXU2VPYRFMLYMCXWCA4VPPSNX4OG7ZR`
- WBTC (`Wrapped Bitcoin`): `CBKVTLR2UG5OVU5AYYEYHJ4NJ7QMTTP2UHCD6ZT5IOXC7FP22GFBDD7B`
- XAU (`PAX Gold`): `CDH5H5Q4HFSRS3MNQQENK2TFME3VR5VZKRJVU7O7OE4ZUDMJCBUKSSPT`
- EURC (`Euro Coin`): `CDGSNI4AA5K2TU6W3HUSJQE4O4ZJ4U4OV4XCB6J2UIJO5UNKEIPL7SZ6`

## Vault Alpha (High Risk)

Objective: aggressive crypto portfolio universe.

- Asset universe (configured): `USDC`, `WETH`, `WBTC`
- Production target: Ultra Capital `BTC`, Ultra Capital `ETH`, `XLM`, Circle
  `USDC`
- Vault: `CCQALBEUASFSI5EWUYVC2E2FVH7FL4GGJK6QMJRY7EGDDY2FDVTM5SQJ`
- Share token: `CABQ3RF7ATCOKAD6S5ROKXPOIUXTS65HJNZJEAA25OTPOUMQHEKWIWYL`
- Guard: `CB5BXH3HVKHQJNOGYPVPOBTEI2TBDVZ7HSSGSFAZR7J4CJ3EQLE5RPXE`

User flow transactions:

- Deposit: https://stellar.expert/explorer/testnet/tx/8a4e97a1e8b4be88091b2515453297edd5fa7c0bbbdf1f4085b465b200ddc47b
- Withdraw: https://stellar.expert/explorer/testnet/tx/c1c8eddcbd1dc59616ad6718c9774bac2bed9457a87ba873dbc442ccb5a0e08f

## Vault Beta (Market Risk)

Objective: diversified macro market universe.

- Asset universe (configured): `USDC`, `WETH`, `WBTC`, `XAU`, `EURC`
- Production target: Circle `USDC`, `XLM`, PayPal/Paxos `PYUSD`, Circle
  `EURC`, capped Aquarius `AQUA`
- Vault: `CBYAJNS3UDIDKRXQFTN2HXURCC3L3KH462T4PH5IKUJHHQRCPHQ7CBCC`
- Share token: `CCC3UT3IFEUW5UDKK3NAQ6KF6YF6PQ5O45QCCA2SUIZKGQNCQ6S4RVV4`
- Guard: `CDV2GLKX3OUPWRUV2KSEHRD2HNPDGPBXUMT6S7N5C6YZGD2HT5P7X6BM`

User flow transactions:

- Deposit: https://stellar.expert/explorer/testnet/tx/54d28b6da2baf85f5ed9a5876c759c35c6d5929842ba9805ebaeb3d3c9c8aa4b
- Withdraw: https://stellar.expert/explorer/testnet/tx/9155501471a016b7ee18c5591b0586eacaccd59efaf62f30213e6d29222222ac

## Vault Gamma (Low Risk, USDC)

Objective: conservative single-asset vault (`USDC`).

- Production target: `USDC` first, with optional `EURC`, stable-stable LP, and
  approved RWA/treasury sleeves after oracle/liquidity review
- Vault: `CAYM2DPPNWTHLTC7EKD3UTNVPZUOYGOIZUCH2O3CNL6BAIH53H5JBG5Z`
- Share token: `CC6G2E7H25THCLR6GAEGGOC3PP22LEQMNHGYPI7CVSDZHMQ4ZWO3EK4A`
- Guard: `CDGAO6IMPE25WFWPTEGQQG63TO2MAGPXJZV2K5YMNUXBB2KPSYUM3DJN`
- Lending protocol adapter contracts:
  - Mock lending pool: `CCWMDBOVH543PGAEMCXQD4JB55L5JABV4PS6AHH72HT6BCBTSRI4AA2F`
  - Blend strategy: `CD6AE2TVUA4MZHF7TMG2GCOGYENJZQOSPSFQMGXC2M2OF3XZFSC5JRU2`
  - Blend strategy (auth-fix redeploy): `CC272UCB36RHYCOMLNAPJZVFYSJNLYUEYYRZWO5I627PLMMOTJ6WRAQJ`

User flow transactions:

- Deposit: https://stellar.expert/explorer/testnet/tx/ea226b7590d406a7e47c22905091c235d2b958070ed70761994d66b95ddf3165
- Withdraw: https://stellar.expert/explorer/testnet/tx/0a013ff7c13581555fd00f677138248029d46497399510d84484998c1db100d0

Gamma full lending lifecycle validation (fresh run on `2026-04-16`):

- Demo user: `GAYTDEZXAHEFVJYECMD3KZ2G3RXMX6ARGTEBE4EBXFDMT6LEKYNQWDQ5`
- Active strategy used: `CCUNZPO7ETFQP37I6WNBVRZDSPO2RU23G6GXBIOY6JZCU6ZNGXM7DCUM`
- Active lending pool used: `CDLQ3RLAMINPJTVE7NHSXEFLFFF5PRVJF6KLIZ6Z7GRNAQ5HMII2442Q`
- Deposit: https://stellar.expert/explorer/testnet/tx/5351388809691a14ee6aef3f1916bffc3eed6adc48949c4f17d56f2da4be2d37
- Invest: https://stellar.expert/explorer/testnet/tx/82da7caebb2fd92a6ac21d184e79e3b540c67e775ee00b17d29f113819bd7f68
- Yield injection: https://stellar.expert/explorer/testnet/tx/40da16e3058556af55acbdcfb6c1d4469edace38450d93b247f7864f11211940
- Withdraw: https://stellar.expert/explorer/testnet/tx/46826d6cc979ddfc111af6f4458643d40aa361a8be5b521894004d8550bdfeea

## Explorer Shortcuts (Contracts)

- Alpha Vault: https://stellar.expert/explorer/testnet/contract/CCQALBEUASFSI5EWUYVC2E2FVH7FL4GGJK6QMJRY7EGDDY2FDVTM5SQJ
- Beta Vault: https://stellar.expert/explorer/testnet/contract/CBYAJNS3UDIDKRXQFTN2HXURCC3L3KH462T4PH5IKUJHHQRCPHQ7CBCC
- Gamma Vault: https://stellar.expert/explorer/testnet/contract/CAYM2DPPNWTHLTC7EKD3UTNVPZUOYGOIZUCH2O3CNL6BAIH53H5JBG5Z
