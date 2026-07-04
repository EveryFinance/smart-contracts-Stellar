# Elyx v2 — Technical Integration Architecture

Status: proposal / pre-implementation
Branch: `feature/elyx-v2-ecosystem-architecture`
Date: 2026-07-03

Companion document: [Elyx v2 Vision, Product & Services](elyx-v2-vision-and-product.md)
holds the product narrative — what each integration is for and why it
matters to Elyx and to Stellar. This document is the engineering reference:
mechanism, contract addresses, feasibility evidence, protocol flow diagrams,
and the full audit trail behind every integration named there.

**Will be done vs. nice to have**, in one line: §2.1–2.6 and §4's ✅-marked
rows are committed (Aquarius, Anchor Platform, StellarBroker, CCTP,
MoneyGram/Mercuryo/BlindPay, and Allbridge as a product decision outside the
4-month rollout clock). §2.7 (Templar) and §3 (RWA — Ondo/USDY, Spiko) are
audited here with the same rigor but are explicitly *not* committed — real,
evaluated, nice to have. Full framing: vision document, top section.

---

## 1. Existing Stellar Integrations (Baseline)

Nothing below changes. Everything in §2 is additive.

| Integration | Role | Stellar TVL (2026-07-03, DefiLlama-verified) |
|---|---|---|
| Blend Protocol | Lending strategy | $140.95M |
| Soroswap | AMM LP strategy | $1.24M |
| Phoenix Protocol | AMM LP strategy | $547K |
| Reflector | Primary oracle | — |
| DIA | Fallback oracle | — |

Note for context carried into §2: Soroswap and Phoenix's own on-chain
liquidity is thin. That's a direct input into why StellarBroker (§2.6) and
Aquarius (§2.1) matter more than they might otherwise.

---

## 2. New Stellar Integrations — Audited One by One

Each entry states the mechanism, the feasibility verdict with the evidence
behind it, why it's useful to Elyx specifically, and the impact on the wider
Stellar ecosystem — the two justifications a product stakeholder and a
protocol engineer each need, answered separately because they're different
questions.

§2.1–2.6 are the **selected six** — the vision document's committed list.
§2.7 (Templar) is audited here with the same rigor but is **not selected**:
real and useful, but with a thinner track record and an unconfirmed
contract ABI, so it's covered for completeness and future reference, not as
part of the current selection.

### 2.1 Aquarius (AQUA) — new AMM strategy

**Feasibility: High.** CoinFabrik + Certora audited. $46.1M TVL, DefiLlama-verified
2026-07-03 — more than 25× Soroswap and Phoenix's combined on-chain liquidity.
Exposes the same guard shape the vault already wraps twice — verified by
reading `contracts/strategies/soroswap_lp/src/lib.rs` directly, not assumed:
`get_total_value(vault) -> i128`, `withdraw_fraction(vault, numerator,
denominator, to)`, `asset_in_use(vault, asset) -> bool`, plus
`add_liquidity`/`remove_liquidity`/`swap`.

**On timeline: a commonly-cited estimate for this class of Stellar
integration ("under 1 day") is not the real engineering estimate for this
one, and shouldn't be treated as such.** This is a *new smart contract*
handling real deposits, not an API wrapper. The existing `SoroswapLpStrategy`
guard it's modeled on is 650 lines of contract code backed by 1,345 lines of
tests (43 test functions) — checked directly in this repo. Building
`AquariusStrategy` to the same standard realistically needs **1–2 weeks**
for design, implementation, and testing, not under a day. The thinner
figure likely reflects a wallet/API-style integration effort — it doesn't
apply cleanly to a new audited Soroban contract handling vault funds.

**Utility to Elyx:** the single highest-leverage strategy addition available —
real liquidity depth, zero new engineering pattern, zero new risk category.

**Impact on Stellar:** routes vault capital into Stellar's largest DEX by TVL,
deepening core AMM liquidity instead of fragmenting it across more venues —
composing with an existing building block rather than reinventing one.

**Technical integration:** new `AquariusStrategy` guard contract implementing
`get_total_value` / `withdraw_fraction` / `asset_in_use`, two-step activation
matching how Blend/Soroswap/Phoenix are already wired in: factory admin adds
it to `factory.AuthorizedGuards` (global whitelist), then each vault's
manager calls `vault.add_active_guard` and `vault.set_authorized_ops` to turn
it on for that specific vault. No vault core changes.

```
      ┌───────────────────────────────────────────────────────────┐
      │ Vault                                                     │
      │ execute_op(caller, guard=AquariusStrategy, fn_name, args) │
      └───────────────────────────────────────────────────────────┘
                           │
                           │  factory-whitelisted, then vault.add_active_guard
                           │  (same two-step as Blend/Soroswap/Phoenix today)
                           ▼
      ┌───────────────────────────────────────────────────────────┐
      │ AquariusStrategy  (new guard — verified same shape as     │
      │ the existing SoroswapLpStrategy contract, checked against │
      │ contracts/strategies/soroswap_lp/src/lib.rs)              │
      │                                                           │
      │ get_total_value(vault) -> i128                            │
      │ withdraw_fraction(vault, numerator, denominator, to)      │
      │ asset_in_use(vault, asset) -> bool                        │
      │ add_liquidity / remove_liquidity / swap (vault, ...)      │
      └───────────────────────────────────────────────────────────┘
                           │
                           │  calls Aquarius's own pool contract
                           ▼
      ┌────────────────────────────────────────────────────────┐
      │ Aquarius AMM pool contract                             │
      │ deposit / withdraw (per docs.aqua.network — Aquarius's │
      │ own interface, not independently confirmed against     │
      │ their source the way the guard shape above is)         │
      └────────────────────────────────────────────────────────┘
```

### 2.2 Anchor Platform / SEP-12 — institutional KYC pathway

**Feasibility: High, zero contract surface.** `add_member`/`remove_member` and
`set_private_pool` already exist in the deployed vault. The gap is entirely
off-chain: an SEP-12 KYC/KYB provider behind SDF's Anchor Platform, with a
permissioned relayer calling `add_member` on approval.

**Utility to Elyx:** the actual unlock behind every institutional capital
figure in this document — see §3's RWA discussion. Also directly answers the
standing final-audit note that admin/manager/trader roles are powerful, by
making custody-backed key management (Fireblocks/Anchorage/BitGo all confirm
live Stellar support) operationally real rather than theoretical.

**Impact on Stellar:** a reusable reference — a non-custodial Soroban vault
wiring real KYC through Stellar's own Anchor Platform rather than a bespoke
gate is exactly the composable pattern other Stellar builders can point to.

```
 Institution               SEP-12 KYC Provider        Anchor Platform          Vault
      │                          │                          │                    │
      │  submit KYC/KYB docs      │                          │                    │
      ├─────────────────────────►│                          │                    │
      │                          │  verify + approve         │                    │
      │                          ├─────────────────────────►│                    │
      │                          │                          │  relayer calls      │
      │                          │                          ├───────────────────►│  add_member(institution)
      │                          │                          │                    │
      │  deposit into private-pool vault ────────────────────────────────────────►│
```

**Technical integration:** deploy Anchor Platform, select an SEP-12 KYC
provider, build the relayer service that calls `add_member` post-approval. No
vault contract changes.

### 2.3 Circle CCTP — lead cross-chain bridge

**Feasibility: High, contract addresses confirmed on mainnet.** Live since
May 19, 2026. Verified mainnet contracts (`developers.circle.com/cctp/references/stellar-contracts`,
domain ID 27):

```
TokenMessengerMinter  CAE2G5Z77UP7GYPYGFOWFGW7C7J6I4YP2AFGSADRKQY62SYUFLPNFTXL
MessageTransmitter     CACMENFFJPJMSDAJQLX4R7K3SFZIW2LJSE3R2UMLGSWHFHS353FVXAZV
CctpForwarder          CBZL2IH7F6BIDAA3WBNXYKIXSATJGMSW7K5P5MJ6STX5RXN47TZJDF5T
```

`CctpForwarder.mint_and_forward(message, attestation)` is publicly callable by
any account — no special receiving-contract logic required on Stellar's side.

**Critical technical detail, must be correct before go-live:** on the
*source*-chain burn, both `mintRecipient` and `destinationCaller` must be set
to the `CctpForwarder` contract address, **not** the end recipient. Per
Circle's own docs, funds sent with the wrong recipient are permanently stuck
and unrecoverable. This is a one-time integration detail, not an ongoing risk,
but it is a hard failure mode if a front-end gets it wrong once.

```
 Source chain (e.g. Ethereum)        Circle Attestation Service        Stellar (destination)
        │                                      │                              │
        │ burn USDC                             │                              │
        │ mintRecipient      = CctpForwarder     │                              │
        │ destinationCaller  = CctpForwarder     │                              │
        ├───────────────────────────────────────►│                              │
        │                                       │  signs attestation           │
        │                                       ├──────────────────────────────►│
        │                                       │                              │
        │            anyone calls mint_and_forward(message, attestation)        │
        │                                       │                              ▼
        │                                       │                     CctpForwarder contract
        │                                       │                              │ mints via MessageTransmitter
        │                                       │                              ▼
        │                                       │                   depositor's Stellar account
        │                                       │                              │
        │                                       │                              ▼
        │                                       │                       vault.deposit(...)
```

**Utility to Elyx:** widens the USDC deposit funnel — Elyx's base asset — from
23+ external chains, with no new trust root, since Circle is already
implicitly trusted as USDC's issuer.

**Impact on Stellar:** reinforces USDC as Stellar's primary settlement asset
at the moment CCTP is being pushed as flagship 2026 infrastructure.

**Technical integration:** front-end/relayer calls `mint_and_forward` with a
valid Circle attestation; USDC lands directly in the depositor's Stellar
account, deposited into a vault normally from there. No vault contract
changes; TypeScript client bindings exist in `github.com/circlefin/stellar-cctp`.

### 2.4 On/off-ramp — multi-provider, not single-vendor

**Feasibility: High, but the vendor mix matters more than any single pick.**
Comparing the major Stellar on-ramp partners side by side:

| Provider | Mechanism | Access model | Sweet spot |
|---|---|---|---|
| **MoneyGram Ramps** | SEP-10 + SEP-24, cash | Partner-gated — domain allowlisting required, email application | Cash-based reach, 170+ countries, no bank account needed |
| **Mercuryo** | SEP-24, card/Apple Pay/Google Pay | Already live on Stellar (LOBSTR) — adding it once SEP-24 is supported at all is close to config-only | Global card/wallet coverage, near-zero marginal integration cost |
| **BlindPay** | REST API + webhooks, local bank rails | Self-serve — sandbox/production keys from a dashboard, public Node SDK | Brazil (Pix), Mexico (SPEI), Colombia (PSE) — LatAm B2B/instant settlement |
| **alfredpay** | Unclear current settlement chain | Appears self-serve, compliance gating likely | LatAm consumer/remittance — **flagged unverified**, confirm current Stellar routing before committing |

**Justification for not picking just one:** once SEP-24 support exists for one
anchor, adding a second is mostly TOML/anchor-registry configuration and
per-anchor QA, not new protocol work. That makes Mercuryo close to a free
addition on top of MoneyGram rather than a competing choice — MoneyGram wins
on cash reach, Mercuryo wins on cost-to-add and card coverage, BlindPay wins
specifically if LatAm depositors are a target segment. **Recommendation: build
SEP-24 support once, then layer MoneyGram + Mercuryo immediately, add BlindPay
if/when LatAm traction justifies it, and leave alfredpay pending verification
of its current chain.**

```
      ┌──────────────────────────┐
      │ Depositor (fiat)         │
      │ SEP-24 Interactive UI    │
      └────────────┬─────────────┘
                    │  routes to whichever anchor the depositor picks:
                    │
                    │   MoneyGram Ramps — cash, 170+ countries
                    │   Mercuryo        — card / Apple Pay / Google Pay
                    │   BlindPay        — Pix / SPEI / PSE (LatAm rails)
                    ▼
      ┌──────────────────────────┐
      │ Depositor's Stellar      │
      │ account (funded)         │
      └────────────┬─────────────┘
                    │
                    ▼
      ┌──────────────────────────┐
      │ vault.deposit(...)       │
      └──────────────────────────┘
```

**Impact on Stellar:** channels Elyx's on/off-ramp traffic through Stellar's
existing anchor ecosystem instead of a bespoke parallel rail — visible
ecosystem value creation, and specifically for MoneyGram, use of SDF's own
flagship real-world payments partnership.

### 2.5 Allbridge Core — secondary bridge, scope-fenced

**Feasibility: Medium — real and live, but risk-fenced by design, not by
protocol immaturity.** Pool-based bridges are DeFi's most-exploited category
historically (Ronin, Wormhole, Nomad, Poly Network). Allbridge Core is a real,
audited, live Soroban integration — the constraint is entirely on Elyx's side:
**bridged assets stay user-facing only and are never held as a guard position
inside vault NAV**, so a bridge-side exploit can never become an Elyx
depositor loss.

**Utility to Elyx:** broadens which chains can fund a vault beyond CCTP's
USDC-only path.

**Impact on Stellar:** diversifies which bridges route liquidity into Stellar
rather than concentrating all cross-chain trust in one provider.

**Technical integration:** front-end integration only — a depositor bridges
into their own Stellar account via Allbridge, then deposits normally. No
guard contract, no vault change.

```
      ┌─────────────────────────────────────────────────┐
      │ External chain  (e.g. Base, Arbitrum, Optimism) │
      │ user bridges their own funds via Allbridge Core │
      └─────────────────────────────────────────────────┘
                           │
                           │  liquidity-pool + cross-chain messaging
                           │  (the mechanism itself, not an Elyx contract)
                           ▼
      ┌─────────────────────────────────┐
      │ Depositor's own Stellar account │
      │ receives bridged asset directly │
      └─────────────────────────────────┘
                           │
                           │  depositor then deposits normally, like any other funding source
                           ▼
      ┌───────────────────────────────────────┐
      │ vault.deposit(amount, from=depositor) │
      └───────────────────────────────────────┘

No guard contract exists for Allbridge, deliberately: bridged assets
never sit inside the vault's own accounting boundary. Contrast with
Aquarius/StellarBroker (this document), which do get a guard because the
vault holds a position there — Allbridge never does.
```

### 2.6 StellarBroker — execution router

**Feasibility: Medium-High.** Runtime Verification-audited (on-chain
settlement leg — the routing/matcher server is off-chain infrastructure, a
normal and expected split for a DEX aggregator, same architecture 1inch and
Jupiter use). Already routes across Soroswap, Aquarius, Phoenix, and the
classic Stellar DEX/SDEX.

**On timeline: same correction as Aquarius (§2.1) applies here.** A
commonly-cited estimate of "1–5 days" is plausible for wiring a client
against StellarBroker's existing API, but `StellarBrokerRouter` is still a
new Soroban guard contract that has to be written, unit-tested, and
integration-tested against the vault's own guard whitelist before it can
touch real funds — realistically **1–2 weeks**, using the same
`SoroswapLpStrategy` size/test-coverage baseline as the reference point.

**Utility to Elyx:** given how thin Soroswap's and Phoenix's own liquidity
turned out to be ($1.24M and $547K respectively), routing trades through a
router that automatically splits a single order across multiple pools for
best execution matters more than a naive single-venue guard would deliver.
One integration instead of three separate AMM guards to build and maintain.

**Impact on Stellar:** composability over reinvention — integrating a shared
router benefits every protocol using it, instead of every team building its
own bespoke version.

**Technical integration:** one guard contract calling StellarBroker's
on-chain settlement contract in place of calling Soroswap/Aquarius/Phoenix
pair contracts directly — same two-step activation as Aquarius (§2.1):
factory-whitelisted, then activated per-vault via `add_active_guard`.

```
      ┌──────────────────────────────────────────────────────────────┐
      │ Vault                                                        │
      │ execute_op(caller, guard=StellarBrokerRouter, fn_name, args) │
      └──────────────────────────────────────────────────────────────┘
                           │
                           │  factory-whitelisted, then vault.add_active_guard
                           ▼
      ┌─────────────────────────────────────────────────────┐
      │ StellarBrokerRouter  (new guard)                    │
      │                                                     │
      │ swap(vault, amount_in, min_out, path) -> amount_out │
      └─────────────────────────────────────────────────────┘
                           │
                           │  on-chain settlement call, single transaction
                           ▼
      ┌─────────────────────────────────────────────────┐
      │ StellarBroker on-chain settlement contract      │
      │ (the piece Runtime Verification's audit covers) │
      └─────────────────────────────────────────────────┘
                           │
                           │  route computed off-chain beforehand by StellarBroker's
                           │  matcher engine — a server watching ledger state, not a
                           │  Soroban contract; splits one order across venues below
                           ▼
      ┌──────────┐   ┌──────────┐   ┌─────────┐
      │ Soroswap │   │ Aquarius │   │ Phoenix │
      │ $1.24M   │   │ $46.1M   │   │ $547K   │
      └──────────┘   └──────────┘   └─────────┘
```

### 2.7 Templar Protocol — secondary lending (conditional)

**Feasibility: Medium — real and audited, needs one more diligence step
before committing engineering.** Halborn-audited ("Templar Soroban Vault"),
live since Nov 2025, **$6.2M Stellar-specific TVL** (of $22.8M across Bitcoin,
Ethereum, NEAR, and Stellar — one multi-chain protocol, not two colliding
projects, confirmed via direct DefiLlama API pull). Its GitHub is a fork of
`blend-contracts-v2`, meaning its interface likely mirrors Blend's — but exact
function signatures haven't been confirmed against the audited ABI.

**Utility to Elyx:** a diversification move, not a scale move — reduces how
much of the vault's lending exposure sits inside Blend alone (currently
100%). Also, unlike Blend (supply-only by Elyx's own design), Templar is
expanding into RWA-collateralized borrowing, a genuinely new capability.

**Impact on Stellar:** supports a second real lending market rather than
funneling all Soroban lending activity into one protocol — a resilience
contribution.

**Note on trust model:** Templar is curator-driven — risk parameters
(collateral, liquidation terms) are set per-market by a curator, not fixed
protocol-wide. "Is Templar safe" depends on which curator's market Elyx would
deposit into; identify the curator for any Stellar market before integrating.

**Next step before committing:** pull the actual contract ABI from
`app.templarfi.org` or Templar's GitHub and confirm it against the guard
interface Elyx's Blend strategy already implements.

```
      ┌──────────────────────────────────────────────────────────┐
      │ Vault                                                    │
      │ execute_op(caller, guard=TemplarStrategy, fn_name, args) │
      └──────────────────────────────────────────────────────────┘
                           │
                           │  same shape as BlendStrategy — verified against
                           │  contracts/strategies/blend/src/lib.rs — IF Templar's own
                           │  ABI matches once confirmed (not yet done, see above)
                           ▼
      ┌─────────────────────────────────────────────────────────────────┐
      │ TemplarStrategy  (hypothetical guard, not yet built —           │
      │ Templar is 'nice to have', not selected, see vision doc)        │
      │                                                                 │
      │ get_total_value(vault) -> i128        [same names as Blend]     │
      │ withdraw_fraction(vault, numerator, denominator, to)            │
      │ asset_in_use(vault, asset) -> bool                              │
      │ supply(vault, pool, asset, amount) / withdraw_from_lending(...) │
      └─────────────────────────────────────────────────────────────────┘
                           │
                           │  calls Templar's own Soroban Vault contract
                           ▼
      ┌──────────────────────────────────────────────────────────┐
      │ Templar 'Cypher Lending' Soroban Vault contract          │
      │ curator-set risk params — collateral, liquidation terms  │
      │ set per-market, not fixed protocol-wide like Blend       │
      │ (app.templarfi.org — exact ABI not yet pulled and diffed │
      │ against the guard interface assumed above)               │
      └──────────────────────────────────────────────────────────┘
```

---

## 3. RWA Tokenized Funds — verification detail

Two funds were evaluated for direct portfolio-asset integration. Both are
"nice to have" for a future phase, not a v2 commitment — one is genuinely
accessible today with a real caveat, the other is not accessible without a
direct business relationship.

**Ondo Finance / USDY — feasible, gated by depositor eligibility, not by
contract engineering.** Verified directly against the live Stellar asset
record: `auth_required: false` — USDY is a freely transferable classic Stellar
asset, no allowlist required to hold it, already tradeable on Soroswap's
aggregator (5,274 trustlines, 284,897 trades to date). A vault could add it as
a priced portfolio asset with no KYC work on the vault's side. The open
question is legal, not technical: USDY is Reg-S restricted (barred for US
persons, EEA/UK limited to qualified investors) — Elyx would need a
depositor-eligibility check, and the issuer retains clawback/freeze rights.
**~$528M Stellar-specific TVL, DefiLlama-verified** — larger than Blend's
entire lending book, if this is ever prioritized.

**Spiko — not realistically accessible today.** Spiko's Soroban token
contract has a built-in allowlist ("Permission Manager"): only individually
KYC'd, allowlisted addresses can hold it at all. The vault's own contract
address would need Spiko's direct sign-off before receiving a single unit —
there's no DEX pool, and redemption is manual/batched through Spiko's
operators. This is an OTC/direct-relationship instrument, not an
open DeFi-composable asset. Revisit only if a direct conversation with Spiko's
team leads somewhere; don't scope engineering work against it.

Both stay explicitly out of the committed integration list in §4 — they're
tracked here as a later-phase option contingent on legal review (USDY) or a
business development outcome outside Elyx's control (Spiko), not as
near-term work.

---

## 4. Feasibility Audit — Summary

**Mainnet, not testnet, confirmed for every integration in this table.**
Every committed item and the nice-to-have row are live with real economic
activity today: Aquarius, Templar, Ondo, and Spiko all have real DefiLlama/
Messari-tracked TVL (which by construction only measures mainnet activity —
none of it is testnet or notional); Circle CCTP has explicit mainnet
contract addresses; MoneyGram, Mercuryo, and StellarBroker have real dollar
volume moving through them in production; BlindPay's own changelog
explicitly distinguishes its mainnet operational wallet from its separate
testnet issuer. Nothing here is a testnet-only announcement dressed up as a
live integration.

| Integration | Feasibility | Evidence | Selected? | Blocking dependency |
|---|---|---|---|---|
| Aquarius | **High** | $46.1M TVL verified, audited, same guard shape live twice already | ✅ | None |
| Anchor Platform / SEP-12 | **High** | Zero contract surface, primitive already deployed | ✅ | KYC provider selection |
| Circle CCTP | **High** | Mainnet contracts confirmed, publicly callable | ✅ | Front-end must set `mintRecipient`/`destinationCaller` correctly — one-shot failure mode |
| MoneyGram Ramps | **High**, admin overhead | SEP-24, live, SDF-partnered | ✅ | Partner approval process (unpublished timeline) |
| Mercuryo | **High**, near-zero cost | Already live via SEP-24 elsewhere on Stellar | ✅ | Requires SEP-24 support to exist first |
| BlindPay | **High** | Self-serve API, public SDK, confirmed Stellar support | ✅ | None significant |
| Allbridge Core | **Medium** | Live, audited — risk fenced by scope choice, not by protocol maturity | ✅ (product decision, held out of the 4-month rollout — §6) | Must never become a guard position |
| StellarBroker | **Medium-High** | Audited settlement leg, live routing across 3 AMMs + SDEX | ✅ | Confirm audit scope vs off-chain matcher component |
| Templar Protocol | **Medium** | $6.2M verified TVL, Halborn-audited, Blend-forked codebase | ❌ not selected | Exact ABI confirmation needed; thinner track record than Aquarius |
| DeFindex | *(not pursued)* | Real, audited yield-router protocol | ❌ deliberately excluded | Vault-of-vaults nesting creates recursive NAV computation risk against Elyx's own vault; listed here so the omission reads as a decision, not an oversight |
| Ondo / USDY | **Medium** (nice to have) | Freely transferable, $528M verified TVL, tradeable today | ❌ not selected | Reg S depositor-eligibility legal review |
| Spiko | **Low / blocked** (nice to have) | Allowlist-gated contract, no DEX pool, $563.1M TVL (Messari Q1 2026 — Stellar is Spiko's *largest* chain, ahead of Arbitrum) but inaccessible | ❌ not selected | Direct allowlisting agreement with Spiko required |
| alfredpay | **Unverified** | Self-serve claims, current Stellar routing unconfirmed | — under evaluation | Re-verify chain/compliance status |

Reading the table for the rollout specifically: **every integration inside
the 4-month rollout (§6) — Aquarius, StellarBroker, CCTP, MoneyGram,
Mercuryo, BlindPay — is fully committed.** Templar and the RWA "nice to
have" tier are real product work but shouldn't be described as part of
*this* rollout's deliverable — they're roadmap items pursued some other way,
or once they mature further.

---

## 5. Architecture Changes Required

### 5.1 Factory contract — what permissionless creation actually requires

This needed verifying against the deployed contract rather than the redesign
spec, and the real mechanics are more involved than "relax an auth check."
Confirmed directly against `contracts/factory/src/lib.rs`:

- **All three registration paths are admin-gated today**: `create_vault`,
  `register_vault`, and `verify_and_register_vault` each call
  `caller.require_auth()` and then hard-panic with `FactoryError::NotAdmin`
  unless `caller == get_admin(&env)`. There is no partial or role-based
  permissionless path today — it's a single flat admin check on every entry
  point that touches the registry.
- **None of them deploy a vault contract.** `create_vault(caller, vault,
  manager, base_asset, seed_amount)` takes an *already-deployed* vault
  address as an argument. It cross-checks `vault.get_manager()` against the
  `manager` argument, pulls `seed_amount` of `base_asset` from `caller` via
  `transfer_from`, calls `vault.seed_deposit(...)`, and registers the vault
  in the index. The actual vault WASM upload and constructor call happen
  entirely outside the factory, today via the operator-run deploy scripts
  (`scripts/deploy_mainnet.sh`).
- **Vault setup has a circular deployment dependency that a self-serve flow
  has to solve.** The vault's `__constructor` requires
  `params.share_token_admin == vault` — i.e. the share token must already
  exist on-chain with the vault's own (not-yet-deployed) address set as its
  admin, which means the vault's contract ID has to be deterministically
  predicted *before* either contract is deployed. Deploy scripts do this with
  Stellar CLI's deterministic contract-ID prediction; a self-serve UI would
  need to do the same thing programmatically, not just skip a permission
  check.
- **`AuthorizedAssets`/`AuthorizedGuards` are not checked at creation time at
  all today.** They're real (`get_authorized_assets`,
  `is_authorized_asset`, `get_authorized_guards`, `is_authorized_guard` all
  exist on the factory), but the vault only consults them later, when its
  manager calls `add_portfolio_asset` / `add_active_guard`. `create_vault`
  itself doesn't reference either list.

**What actually has to change, concretely:**

1. **New permissionless registration entry point** (either loosen the check
   on `create_vault` for a defined class of caller, or add a parallel
   `create_vault_permissionless` that skips the admin check but keeps every
   other invariant — manager/vault consistency check, seed amount > 0, not
   already registered).
2. **Factory-driven vault deployment**, not creator-driven. Self-serve
   deployment should not mean "anyone uploads their own vault WASM" — that
   would let a creator substitute unaudited vault code behind an identical
   UI. The safer pattern (the one dHedge and Morpho Blue both rely on:
   permissionless *parameters*, immutable *logic*) is for the factory to hold
   one audited vault WASM hash and deploy new instances of it itself,
   deterministically, using Soroban's on-chain contract-deployment host
   functions — this workspace pins `soroban-sdk = "22.0.1"`
   (`Cargo.toml`), which supports constructor-argument deployment; **the
   exact deployer API call (e.g. `deploy_v2` vs. a differently-named method
   at this SDK version) needs to be confirmed against the installed
   `soroban-sdk` docs at implementation time rather than assumed from this
   document** — so a creator supplies parameters — manager, base asset, fee
   bps, initial guards/assets from the whitelist — and never touches raw
   bytecode. This also resolves the share-token circular-dependency problem
   above, since the factory can predict its own deployment addresses
   deterministically before either contract exists.
3. **Decide whether asset/guard whitelist checks move earlier**, into the
   new creation entry point, rather than staying purely reactive at
   `add_portfolio_asset`/`add_active_guard` time — recommended, so a
   newly-created vault can't reference an unauthorized asset or guard even
   transiently between creation and its first config call.
4. **Protocol fee model decision** for self-serve vaults (take-rate or none
   — a business decision, not blocked on any of the above).

**One safety property that already holds and doesn't need new work:** the
vault's `factory` reference (`DataKey::Factory`) is set once, in the
constructor, with no `set_factory` mutator anywhere in the contract —
confirmed by its absence from the vault's public interface. A
factory-deployed vault can't be pointed at a different, malicious factory
after the fact, in either the current admin-gated flow or the proposed
permissionless one.

**Existing vaults (Alpha, Beta, Gamma) are unaffected and do not need
migration** — they're already registered via `create_vault`/`register_vault`
under the current admin-gated path, and nothing above changes how an already-
registered vault operates. The new permissionless path is additive: a new,
parallel way to reach the same registry, not a replacement for the existing
one.

### 5.2 New guard contracts

For the selected six (§2.1–2.6):
- `AquariusStrategy` (AMM LP, same shape as existing Soroswap/Phoenix guards)
- `StellarBrokerRouter` (execution router guard)

CCTP, the on/off-ramp trio, and Allbridge need no new guard contract — all
three are front-end/relayer integrations (§2.3–2.5), not vault strategies.

Not required for the current selection, listed for future reference only:
`TemplarStrategy` would follow the same guard shape if Templar (§2.7,
**not selected**) is picked up on its own timeline later.

### 5.3 Off-chain services

- Anchor Platform deployment + SEP-12 KYC provider + `add_member` relayer
- CCTP attestation relayer / front-end integration (`mint_and_forward` caller)
- SEP-24 client supporting MoneyGram and Mercuryo from Month 1, with BlindPay
  layered on in Tranche 3 (§6) — all three inside the same 4-month rollout

**No changes required** to vault NAV computation, fee accrual, or TVL-guard
(`max_loss_bps`) logic — every integration above is additive to the existing
audited core. One correction from an earlier pass of this document: it
previously referenced a "concentration guard" as an active, paired control
alongside the TVL guard. Verified against `contracts/vault/src/error.rs` and
`contracts/vault/src/lib.rs`: `ConcentrationLimitExceeded` is a defined error
code, but nothing in the current vault contract throws it — there is no
`max_concentration_bps` storage, setter, or check wired into `execute_op`.
It's a vestigial error variant from an earlier design iteration, not a live
control. `max_loss_bps` (the TVL guard) is the only post-operation value
check that actually runs today. If per-strategy concentration limits are
wanted for the permissionless-vault story — arguably more important once
vault creators, not just Elyx's own managers, are choosing strategy mixes —
that's new work, not something already shipped.

---

## 6. Timeline

**Hard constraint: this rollout must complete within 4 months total.**
Sequenced against commonly-cited per-partner integration-effort estimates
for these Stellar ecosystem partners, not an internal guess — with one
important correction applied throughout (see below).

| Integration | Commonly-cited estimate | Realistic engineering estimate |
|---|---|---|
| Aquarius | Under 1 day | **1–2 weeks** — new guard contract, design + implementation + testing (§2.1) |
| StellarBroker | 1–5 days | **1–2 weeks** — new guard contract, same reasoning (§2.6) |
| Circle CCTP | 1–5 days | 1–5 days — genuinely no new contract, calls an already-deployed, already-audited public contract |
| Mercuryo | 1–2 weeks | 1–2 weeks — SEP-24 anchor config, no new contract |
| BlindPay | 1–2 weeks | 1–2 weeks — REST API integration, no new contract |
| Anchor Platform | **1+ month** | 1+ month — unchanged, off-chain KYC infra |
| MoneyGram Ramps | **1+ month** | 1+ month — unchanged, partner-approval-gated |
| Allbridge Core | **TBD — no estimate exists anywhere** | N/A — moved out of this rollout |

**The commonly-cited figures for Aquarius and StellarBroker are not the real
engineering estimate, and this document no longer cites them as if they
were.** Both require a genuinely new Soroban smart contract handling real
vault funds, not an SDK/API wrapper — "under 1 day" and "1–5 days" almost
certainly reflect a thinner integration effort than "write, unit-test, and
integration-test a new guard contract before it can move money." The
existing `SoroswapLpStrategy` guard those two are modeled on is 650 lines of
contract code backed by 1,345 lines of tests (43 test functions), checked
directly against this repo — that's the realistic baseline, and it puts each
new guard at 1–2 weeks, not under a day.

Anchor Platform and MoneyGram remain the true long poles — both can run in
parallel with everything else and with each other, and neither depends on
the other. The corrected Aquarius/StellarBroker estimates still fit
comfortably inside the same 4-month window, they just consume real
engineering weeks in Months 1–2 rather than being treated as free.
**Allbridge still doesn't fit a 4-month hard cap responsibly**: no duration
has ever been published for it anywhere, and committing an unscoped item to
a fixed-length rollout is exactly the kind of overscoping risk worth
avoiding. It's moved out of this rollout (below), not dropped from the
roadmap.

```
Month 1   ████████████████
          Circle CCTP (1-5d) — shipped early, no new contract
          Aquarius guard: design + implementation + testing (1-2wk) — starts
          Anchor Platform KYC provider selection — started, parallel track
          MoneyGram partner application — started, parallel track

Month 2   ████████████████
          Aquarius guard — completed, deployed, activated on-vault
          StellarBroker guard: design + implementation + testing (1-2wk)
            — starts, reusing patterns from the Aquarius build
          Anchor Platform relayer build-out — parallel track continues
          MoneyGram partner approval — parallel track continues (unpublished
            turnaround is the single biggest schedule risk in this plan)
          Mercuryo — sequenced in once shared SEP-24 client work exists

Month 3   ████████████████
          StellarBroker guard — completed, deployed, activated on-vault
          Anchor Platform + MoneyGram — target completion
          BlindPay (1-2wk) — low-risk, slotted in as buffer-filler

Month 4   ████████████████
          Contingency buffer for MoneyGram's unpublished approval timeline
          Integration testing across all five · milestone closeout

Moved out of this rollout: Allbridge Core (duration: TBD — pursue once
scoped directly, likely a second-phase item)
```

**Tranche 1 (Month 1)** — CCTP ships immediately (no new contract). The
Aquarius guard contract's design/implementation/testing starts the same
month, in parallel with Anchor Platform and MoneyGram's long-pole tracks.

**Tranche 2 (Months 2–3)** — Aquarius guard completes and goes live;
StellarBroker guard build starts, informed by the Aquarius build. Anchor
Platform and MoneyGram land; Mercuryo layers on once the shared SEP-24
client exists (near-zero incremental cost, per §2.4).

**Tranche 3 (Month 3–4)** — StellarBroker guard completes and goes live.
BlindPay, final integration testing across all five, and closeout. Built-in
buffer against MoneyGram's partner-approval process, which has no published
turnaround time and remains the plan's biggest single risk.

**Explicitly out of this 4-month rollout, not out of the roadmap:**
- **Allbridge Core** — real and audited, but its duration is unscoped
  anywhere; get it scoped directly before committing it to any rollout, this
  one or the next.
- **Permissionless factory** (§5.1) — genuinely bigger than a 4-month,
  single-partner-style integration; it's platform architecture work, and
  shouldn't be squeezed into this rollout's scope or timeline.
- **Templar Protocol, Ondo/USDY** — as established in §4, neither is
  selected regardless of timeline; pursued on their own schedule.
- **Spiko, Noether/Rails perpetuals, alfredpay** — blocked on external
  dependencies (§4) independent of any timeline question.

---

## 7. Closing Note

Every integration above cleared the same three questions before landing in
this document: is there independently-verifiable evidence of real usage or a
real contract to call (not just a roadmap announcement), does it fit inside
the vault's existing containment without a new class of risk, and is the
integration effort actually proportionate to what it buys. The RWA section in
particular is a case study in why that discipline matters — Ondo and Spiko
look identical from a pitch-deck distance ("tokenized fund on Stellar") and
are completely different engineering and legal problems once verified
directly against their contracts. The rest of this roadmap holds to the same
standard.
