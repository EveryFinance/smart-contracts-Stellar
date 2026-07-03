# Elyx v2 — Vision, Product & Services

Status: proposal / pre-implementation
Branch: `feature/elyx-v2-ecosystem-architecture`
Date: 2026-07-03
Grant target: Stellar Community Fund (SCF) Build Award, Integration Track —
the six integrations selected in §4 are the intended application, submitted
in phases (§6 of the technical document)

Companion document: [Elyx v2 Technical Integration Architecture](elyx-v2-technical-integrations.md)
holds the engineering-level detail behind every integration named here —
contract addresses, feasibility evidence, protocol flow diagrams, and the
full audit trail. This document is the product and ecosystem narrative: what
Elyx v2 is, why each integration matters, and what it does for Stellar.

---

## 1. Vision

Elyx today is three manager-run vaults (Alpha, Beta, Gamma) on a shared, audited
accounting engine. v2 turns that engine into an open platform: **anyone can
permissionlessly deploy a vault, pick a strategy mix from a curated building-block
list, and set their own rebalancing policy** — the same relationship Morpho Blue
has to individual lending markets. Elyx stops being three funds and becomes the
protocol three (and eventually many more) funds are built on.

Alongside that shift: **deeper Stellar ecosystem integration** — more
strategies, a compliant institutional onboarding path, and real
fiat/cross-chain rails, all selected against a single bar: does
independently-verifiable evidence support integrating this today, not just
"is it a good idea."

### Figure 1 — v2 platform topology

```
                          ┌─────────────────────────────┐
                          │      Factory (Registry)       │
                          │                                │
                          │  create_vault(...)             │  ← permissionless in v2
                          │  AuthorizedAssets               │     (admin-only today)
                          │  AuthorizedGuards                │
                          └───────────────┬───────────────┘
                                          │ deploys, whitelist-bounded
                    ┌─────────────────────┼─────────────────────┐
                    ▼                     ▼                     ▼
             ┌─────────────┐       ┌─────────────┐       ┌─────────────┐
             │ Vault Alpha  │       │ Vault Beta   │  ...  │  Vault N     │
             │ (Elyx-run)   │       │ (Elyx-run)   │       │ (community-  │
             │              │       │              │       │  created)    │
             └──────┬───────┘       └──────┬───────┘       └──────┬───────┘
                    │   deposit / withdraw / execute_op            │
                    └─────────────────────┬───────────────────────┘
                                          ▼
                     ┌─────────────────────────────────────────┐
                     │              Guard Contracts               │
                     │  Blend · Soroswap · Phoenix (existing)     │
                     │  Aquarius · StellarBroker (selected, §4)   │
                     └─────────────────────┬─────────────────────┘
                                          ▼
                         External Stellar / Soroban Protocols

  Off-chain / front-end layer (no vault contract changes):
  Anchor Platform (SEP-12 KYC)  ·  MoneyGram / Mercuryo / BlindPay (SEP-24)  ·  Circle CCTP relayer
```

---

## 2. Product & Services

### 2.1 Permissionless Vault Factory

The factory contract already has the right primitives to build this on —
`AuthorizedAssets`, `AuthorizedGuards`, and a `create_vault` entry point all
exist today — but this needed verifying against the actual contract, not just
the design intent, and the real change is bigger than flipping one
permission check. Today `create_vault` (and its siblings `register_vault` and
`verify_and_register_vault`) are admin-only, and none of them deploy a vault
— they register and seed a vault contract that was already deployed
separately. Making creation genuinely self-serve means the factory has to
take over vault deployment itself (deploying new instances of one fixed,
audited vault WASM on the creator's behalf, rather than letting a creator
supply their own bytecode), not just relax who's allowed to call an existing
function. Existing vaults (Alpha, Beta, Gamma) aren't touched by any of this
— it's a new, parallel path into the same registry.

*Full contract-level breakdown of what has to change: see the Technical
Integration Architecture document, §5.1.*

What does need new work:
- **Protocol fee on permissionless vaults** — a take-rate decision (Morpho Blue
  takes none at the protocol layer and lets curators compete on fees; Elyx can
  choose either model).
- **Curator discovery/reputation** — an off-chain indexing/UI layer, since the
  seed-deposit inflation-attack protection still requires a real seed and
  nothing on-chain stops a low-quality creator from launching a thin vault.

```
 Vault Creator                    Factory                        Vault (new)
     │                               │                                │
     │  create_vault_permissionless( │                                │
     │    manager=self, base_asset,  │                                │
     │    fee params, seed)          │                                │
     ├──────────────────────────────►│                                │
     │                               │ deploys new instance of the      │
     │                               │ one fixed, audited vault WASM     │
     │                               │ (creator never supplies bytecode) │
     │                               ├───────────────────────────────►│
     │                               │ seed_deposit(seed)              │
     │                               ├───────────────────────────────►│
     │                               │                                │ total_supply > 0
     │◄──────────────────────────────┤   vault address                │ (inflation-attack safe)
     │                               │                                │
  Asset/guard whitelist (AuthorizedAssets / AuthorizedGuards) is still
  enforced — but later, when the new vault's manager calls
  add_portfolio_asset / add_active_guard, exactly as it is for Alpha/Beta/
  Gamma today. Moving that check earlier, into creation itself, is an open
  design decision (Technical doc §5.1).
```

### 2.2 Institutional Onboarding

`set_private_pool` and `add_member`/`remove_member` already exist and already
gate deposits to an allowlist. v2 wires a real SEP-12 KYC pipeline through
Stellar's Anchor Platform in front of that allowlist, and pairs it with
institutional custody (Fireblocks/Anchorage/BitGo all confirm live Stellar
support) for the `manager` role itself — turning the standing audit note that
"admin/manager/trader roles are powerful" into an operationally-managed risk
instead of a theoretical one.

*Flow diagram and off-chain relayer detail: see the Technical Integration
Architecture document, §2.2.*

### 2.3 Retail On/Off-Ramp

A front-end-only layer — no vault contract touches this — letting a
non-crypto-native depositor fund or exit a vault via card or cash, without
first acquiring USDC on another exchange. v2 leans toward a multi-provider mix
(cash reach, card coverage, and regional bank rails) rather than a single
vendor, since the marginal cost of adding a second SEP-24 anchor once the
first is built is small.

*Full provider comparison, access model, and routing diagram: see the
Technical Integration Architecture document, §2.4.*

---

## 3. Existing Stellar Integrations (Baseline)

Nothing below changes. Everything in §4 is additive.

| Integration | Role | Stellar TVL (2026-07-03, DefiLlama-verified) |
|---|---|---|
| Blend Protocol | Lending strategy | $140.95M |
| Soroswap | AMM LP strategy | $1.24M |
| Phoenix Protocol | AMM LP strategy | $547K |
| Reflector | Primary oracle | — |
| DIA | Fallback oracle | — |

Worth carrying forward: Soroswap and Phoenix's own on-chain liquidity is
thin relative to the rest of the Stellar DEX landscape. That's a direct input
into why Aquarius and StellarBroker matter more to this roadmap than they
might otherwise (§4).

---

## 4. Selected Stellar Integrations

These six are the actual selection — not a menu of candidates, the decision.
Each cleared one bar: independently-verifiable evidence of real usage or a
real, callable contract (not a roadmap announcement), a clean fit inside the
vault's existing risk containment, and — since this list doubles as the
intended SCF Build Award Integration Track application — a place on the
official SCF Integration List. (Other protocols were evaluated and are
covered too, in §5, but explicitly *outside* this selection: either not yet
proven enough, not on the SCF list, or both.)

| # | Integration | Category | On SCF list | Grant phase |
|---|---|---|---|---|
| 1 | Aquarius | AMM strategy | ✅ | Phase 1 |
| 2 | Anchor Platform / SEP-12 | Institutional KYC | ✅ | Phase 1 |
| 3 | StellarBroker | Execution router | ✅ | Phase 1 (bundled) |
| 4 | Circle CCTP | Cross-chain bridge | ✅ | Phase 2 |
| 5 | MoneyGram + Mercuryo + BlindPay | On/off-ramp | ✅ | Phase 2 (MoneyGram/Mercuryo) / 3 (BlindPay) |
| 6 | Allbridge Core | Secondary bridge | ✅ | Phase 3 |

The engineering mechanics, contract addresses, and feasibility evidence for
each live in the companion technical document; this is the "what it is and
why it matters" version.

### 4.1 Aquarius (AQUA) — a second AMM strategy

Aquarius is Stellar's largest DEX by TVL ($46.1M, more than 25× Soroswap and
Phoenix's combined on-chain liquidity), audited by CoinFabrik and Certora, and
exposes the same deposit/withdraw shape the vault already wraps twice. It's
the single highest-leverage strategy addition available — real depth, zero
new engineering pattern. For Stellar, it means vault capital routes into the
ecosystem's deepest AMM instead of fragmenting further across venues.

### 4.2 Institutional KYC — Anchor Platform / SEP-12

This is the actual unlock behind every institutional capital figure this
research turned up, not a compliance checkbox. It wires SDF's own reference
KYC infrastructure in front of the vault's existing member allowlist. For
Elyx, it's the precondition for private, permissioned vaults serving
regulated capital. For Stellar, a non-custodial vault using the ecosystem's
own Anchor Platform rather than inventing a bespoke gate is exactly the kind
of reusable pattern other builders can point to.

### 4.3 Circle CCTP — the lead cross-chain bridge

Live on Stellar mainnet since May 2026, with Circle's own contracts publicly
callable — no wrapped-asset risk, no new trust root, since Circle is already
implicitly trusted as USDC's issuer (Elyx's own base asset). It widens the
deposit funnel to 23+ external chains. For Stellar, an early, real CCTP
integration reinforces USDC's role as the network's primary settlement asset
at exactly the moment SDF is pushing CCTP as flagship infrastructure.

### 4.4 On/off-ramp — MoneyGram, Mercuryo, and BlindPay together

Three complementary providers rather than one: **MoneyGram Ramps** for cash
reach (170+ countries, no bank account required — SDF's own flagship
real-world payments partnership), **Mercuryo** for card/Apple Pay/Google Pay
coverage (already live elsewhere on Stellar, close to a free addition once
the underlying SEP-24 protocol is supported at all), and **BlindPay** for
LatAm local bank rails (Pix, SPEI, PSE). For Elyx, this removes the single
biggest UX barrier for a non-crypto-native depositor without touching a vault
contract. For Stellar, it channels on/off-ramp traffic through the
ecosystem's existing anchor network instead of building a parallel one.

### 4.5 Allbridge Core — a second bridge, deliberately fenced

Real, audited, and live on Stellar — but pool-based bridges are historically
DeFi's most-exploited category, so Allbridge is scoped to user-facing funding
only and never held as a vault position. For Elyx, it broadens which chains
can fund a vault beyond CCTP's USDC-only path. For Stellar, it diversifies
which bridges route liquidity into the network rather than concentrating all
cross-chain trust in one provider.

### 4.6 StellarBroker — one router instead of three guards

An execution router that already splits trades across Soroswap, Aquarius,
Phoenix, and the classic Stellar DEX to get the best combined price. Given
how thin Soroswap's and Phoenix's own liquidity turned out to be, routing
through an aggregator matters more than a naive single-venue guard would
deliver — and it's one integration instead of three separate AMM guards to
build and maintain. For Stellar, using a shared router is composability over
reinvention, which is what the ecosystem's own funding programs prefer.

---

## 5. Considered, Not Selected

Real protocols, genuinely evaluated, deliberately kept out of §4's selection
— either the evidence isn't there yet, or they don't count toward the SCF
grant this roadmap is built around, or both. Listed here so each omission
reads as a decision, not a gap in the research.

### 5.1 Templar Protocol — a second lending market, not yet

A real, Halborn-audited lending protocol with $6.2M in verified Stellar TVL,
built on a fork of Blend's own codebase — meaning it should be close to
"shape-compatible" with the guard Elyx already runs for Blend, and unlike
Blend (supply-only by Elyx's own design), Templar is expanding into
RWA-collateralized borrowing. Genuinely useful as a future diversification
move away from 100% Blend lending exposure. Kept out of §4 for one concrete
reason: **it isn't on the official SCF Integration List**, so it wouldn't
count toward this grant regardless of its technical merit — it's a product
roadmap item to revisit on its own timeline, not a grant deliverable.

### 5.2 RWA Tokenized Funds — nice to have

Two regulated fund tokens were evaluated as portfolio assets a vault could
simply hold. Neither is on the official SCF Integration Track partner list
either, so like Templar, this is pursued for product reasons only, on its
own timeline — never bundled into the grant application in §4.

**Ondo Finance / USDY** is, perhaps surprisingly, freely transferable on
Stellar today with no allowlist and real secondary-market trading volume —
technically accessible now, gated only by a legal question (Reg S restricts
it to non-US persons) rather than an engineering one. At ~$528M
Stellar-specific TVL, it's larger than Blend's entire lending book on
Stellar.

**Spiko** looks similar from a distance but isn't: its token contract has a
built-in allowlist, so the vault itself would need Spiko's direct sign-off
before it could hold a single unit. That's a business conversation, not an
integration task, and isn't scoped into this roadmap.

*Full verification detail for both: see the Technical Integration
Architecture document, §3.*

---

## 6. Closing Note

The thread running through this whole roadmap is the same one that got Elyx
through its first audit clean: don't add a capability because it's available,
add it because it's verified, fits inside what's already been proven safe,
and does something specific for the people using the platform. The
permissionless factory is the biggest product bet in this document — and it
works specifically because it reuses containment Elyx already built and had
audited, rather than asking for new trust. The six integrations in §4 hold to
the same standard, and are, concretely, the Stellar Community Fund Build
Award Integration Track application this roadmap exists to support — phased
per the timeline in the technical document (§6), starting with Aquarius and
Anchor Platform as the first submission. Everything in §5 is real work Elyx
still wants to do; it's just funded and justified on its own terms, not as
part of this grant.
