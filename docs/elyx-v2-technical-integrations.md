# Elyx v2 — Technical Integration Architecture

Status: proposal / pre-implementation
Branch: `feature/elyx-v2-ecosystem-architecture`
Date: 2026-07-03

Companion document: [Elyx v2 Vision, Product & Services](elyx-v2-vision-and-product.md)
holds the product narrative — what each integration is for and why it
matters to Elyx and to Stellar. This document is the engineering reference:
mechanism, contract addresses, feasibility evidence, protocol flow diagrams,
and the full audit trail behind every integration named there.

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
Stellar ecosystem — the two justifications an accelerator reviewer and a
protocol engineer each need, answered separately because they're different
questions.

### 2.1 Aquarius (AQUA) — new AMM strategy

**Feasibility: High.** CoinFabrik + Certora audited. $46.1M TVL, DefiLlama-verified
2026-07-03 — more than 25× Soroswap and Phoenix's combined on-chain liquidity.
Exposes the same guard shape (`deposit`/`withdraw`) the vault already wraps
twice.

**Utility to Elyx:** the single highest-leverage strategy addition available —
real liquidity depth, zero new engineering pattern, zero new risk category.

**Impact on Stellar:** routes vault capital into Stellar's largest DEX by TVL,
deepening core AMM liquidity instead of fragmenting it across more venues —
literally the "combine existing building blocks" mandate the SCF Integration
Track exists to fund.

**Technical integration:** new `AquariusStrategy` guard contract implementing
`get_total_value` / `withdraw_fraction` / `asset_in_use`, added to
`factory.AuthorizedGuards`, wired into `vault.set_authorized_ops`. No vault
core changes.

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
gate is exactly the composable pattern other SCF teams can copy.

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
at the moment SDF is pushing CCTP as flagship 2026 infrastructure.

**Technical integration:** front-end/relayer calls `mint_and_forward` with a
valid Circle attestation; USDC lands directly in the depositor's Stellar
account, deposited into a vault normally from there. No vault contract
changes; TypeScript client bindings exist in `github.com/circlefin/stellar-cctp`.

### 2.4 On/off-ramp — multi-provider, not single-vendor

**Feasibility: High, but the vendor mix matters more than any single pick.**
Comparing every officially-recognized SCF on-ramp partner side by side:

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
                          ┌───────────────────────────┐
   Depositor (fiat) ─────►│   SEP-24 Interactive UI     │
                          └─────────────┬───────────────┘
                                        │ routes to selected anchor
                    ┌───────────────────┼───────────────────┐
                    ▼                   ▼                   ▼
             MoneyGram Ramps       Mercuryo             BlindPay
             cash, 170+            card / Apple /       Pix / SPEI / PSE
             countries              Google Pay           (LatAm rails)
                    │                   │                   │
                    └───────────────────┼───────────────────┘
                                        ▼
                          depositor's Stellar account
                                        │
                                        ▼
                                vault.deposit(...)
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

### 2.6 StellarBroker — execution router

**Feasibility: Medium-High.** Runtime Verification-audited (on-chain
settlement leg — the routing/matcher server is off-chain infrastructure, a
normal and expected split for a DEX aggregator, same architecture 1inch and
Jupiter use). Already routes across Soroswap, Aquarius, Phoenix, and the
classic Stellar DEX/SDEX.

**Utility to Elyx:** given how thin Soroswap's and Phoenix's own liquidity
turned out to be ($1.24M and $547K respectively), routing trades through a
router that automatically splits a single order across multiple pools for
best execution matters more than a naive single-venue guard would deliver.
One integration instead of three separate AMM guards to build and maintain.

**Impact on Stellar:** composability over reinvention — the SCF explicitly
prefers funding integration of a shared router over N bespoke ones.

**Technical integration:** one guard contract calling StellarBroker's
on-chain settlement contract in place of calling Soroswap/Aquarius/Phoenix
pair contracts directly.

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

| Integration | Feasibility | Evidence | Blocking dependency |
|---|---|---|---|
| Aquarius | **High** | $46.1M TVL verified, audited, same guard shape live twice already | None |
| Anchor Platform / SEP-12 | **High** | Zero contract surface, primitive already deployed | KYC provider selection |
| Circle CCTP | **High** | Mainnet contracts confirmed, publicly callable | Front-end must set `mintRecipient`/`destinationCaller` correctly — one-shot failure mode |
| MoneyGram Ramps | **High**, admin overhead | SEP-24, live, SDF-partnered | Partner approval process (unpublished timeline) |
| Mercuryo | **High**, near-zero cost | Already live via SEP-24 elsewhere on Stellar | Requires SEP-24 support to exist first |
| BlindPay | **High** | Self-serve API, public SDK, confirmed Stellar support | None significant |
| Allbridge Core | **Medium** | Live, audited — risk fenced by scope choice, not by protocol maturity | Must never become a guard position |
| StellarBroker | **Medium-High** | Audited settlement leg, live routing across 3 AMMs + SDEX | Confirm audit scope vs off-chain matcher component |
| Templar Protocol | **Medium** | $6.2M verified TVL, Halborn-audited, Blend-forked codebase | Exact ABI confirmation needed |
| Ondo / USDY | **Medium** (nice to have) | Freely transferable, $528M verified TVL, tradeable today | Reg S depositor-eligibility legal review |
| Spiko | **Low / blocked** (nice to have) | Allowlist-gated contract, no DEX pool, $524M TVL but inaccessible | Direct allowlisting agreement with Spiko required |
| alfredpay | **Unverified** | Self-serve claims, current Stellar routing unconfirmed | Re-verify chain/compliance status |

---

## 5. Architecture Changes Required

**Factory contract:**
- Relax `create_vault` from admin-only to permissionless, retaining
  `AuthorizedAssets`/`AuthorizedGuards` as the safety rail.
- Decide and implement the protocol fee model for self-serve vaults.

**New guard contracts:**
- `AquariusStrategy` (AMM LP, same shape as existing Soroswap/Phoenix guards)
- `StellarBrokerRouter` (execution router guard)
- `TemplarStrategy` (conditional on ABI confirmation, §2.7)

**Off-chain services:**
- Anchor Platform deployment + SEP-12 KYC provider + `add_member` relayer
- CCTP attestation relayer / front-end integration (`mint_and_forward` caller)
- SEP-24 client supporting MoneyGram, Mercuryo, and (phase 3) BlindPay as
  anchors

**No changes required** to vault NAV computation, fee accrual, TVL guard, or
concentration-limit logic — every integration above is additive to the
existing audited core.

---

## 6. Timeline

Sequenced to match the SCF Build Award's three-tranche milestone structure
and to avoid the track's explicit overscoping warning (most integrations
should run under ~40 dev-hours per partner).

```
Phase 1  (0–3 mo)  ████████████████
                    Aquarius · Anchor Platform / SEP-12 KYC · StellarBroker
                    → submit now, Medium tier ($50K–$100K)

Phase 2  (3–6 mo)                  ████████████████
                                    Circle CCTP · MoneyGram + Mercuryo · Permissionless factory
                                    → natural second-round application

Phase 3  (6–12 mo)                                 ████████████████████████
                                                    Allbridge · Templar · BlindPay ·
                                                    Ondo/USDY legal review

Deferred, unscheduled:  Spiko · Noether / Rails perpetuals · alfredpay (re-verify)
```

**Phase 1 (0–3 months) — submit as the grant application, Medium tier
($50K–$100K):**
- Aquarius strategy guard
- Anchor Platform / SEP-12 institutional KYC pathway
- StellarBroker router (bundled in as low-cost connective tissue)

**Phase 2 (3–6 months) — natural second-round application:**
- Circle CCTP inbound bridging
- MoneyGram Ramps + Mercuryo on/off-ramp (SEP-24 built once, both layered on)
- Permissionless factory (`create_vault` opened up), first external
  vault creators onboarded

**Phase 3 (6–12 months):**
- Allbridge Core (scope-fenced, user-facing only)
- Templar Protocol, pending ABI confirmation
- BlindPay, if LatAm depositor traction justifies it
- Ondo/USDY reviewed for portfolio-asset addition, pending Reg S legal review

**Deferred, not scheduled:** Spiko (blocked pending direct business
relationship), Noether/Rails perpetuals (break the point-in-time
`max_loss_bps` assumption — needs a continuous liquidation-risk guard that
doesn't exist yet), alfredpay (pending chain/compliance re-verification).

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
