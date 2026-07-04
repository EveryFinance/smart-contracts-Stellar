# Elyx v2 — Milestones, Deliverables, and Budget

Status: proposal / pre-implementation
Branch: feature/elyx-v2-ecosystem-architecture
Date: 2026-07-03

This document lays out the delivery plan for the Elyx v2 work as three
milestones. Each milestone is split into several distinct deliverables, and
each deliverable is split into its component sub-deliverables — design,
implementation, and testing are priced and scheduled separately wherever
that split is meaningful. Every sub-deliverable states its duration, its
budget, and a straightforward way to tell it's done.

Total duration across all three milestones: 18 weeks, approximately 4.5
months.
Total budget across all three milestones: 193,200 EUR.

## A number worth being upfront about

An earlier version of this plan totaled 147,600 EUR, covering six Stellar
ecosystem integrations only. This version adds the permissionless vault
factory and the vault setup/rebalancing agent layer, both genuinely part of
the Elyx v2 product, and both real engineering scope that wasn't priced
before. Adding them honestly costs 45,600 EUR more, not less — that
increase is shown here rather than absorbed by quietly thinning other
deliverables. Duration stays at 18 weeks because both additions run as
parallel tracks alongside existing milestone work rather than extending the
calendar, which does mean more people working at once, not fewer.

## Rate assumptions and an audit note, stated up front

Two blended day rates are used throughout, applied consistently:

Senior Soroban smart-contract engineering — roughly 1,600 to 1,800 EUR per
day. Applies to work that designs, writes, or tests a smart contract
handling real depositor funds or the platform's own registry logic.

Off-chain integration, coordination, agent/orchestration, and documentation
work — roughly 600 to 1,000 EUR per day. Applies to relayer services,
third-party vendor integrations, partner-approval coordination, the
setup/rebalancing agent layer, and technical writing.

One exception: cross-integration testing in Milestone 3 is priced at 2,600
EUR per day, the highest rate in this document, because verifying six
integrations behave correctly together is treated as the single
highest-value activity in the plan, not a formality at the end.

An audit finding worth stating plainly: this budget funds an internal
security review and a written audit-readiness handoff for the new smart
contracts (the Aquarius and StellarBroker strategy guards, and the factory
changes enabling permissionless vault creation), not a full third-party
security audit of the kind already performed on the rest of this codebase.
That is a real residual risk, not a theoretical one. The recommendation is
either to commission an external audit as a follow-on cost before any of
these contracts are exposed to significant value, or to keep exposure
capped low using the vault's existing deposit-cap and value-guard
mechanisms until that audit is done.

A staffing note: several deliverables below run in parallel rather than one
after another. Milestone 1 needs two people working concurrently for four
of its five weeks; Milestone 2 needs two to three people working
concurrently for six of its eight weeks. The budget and schedule assume
that staffing level, not a single person moving between tasks.

---

## Milestone 1 — New Strategy Contracts, First Bridge Integration, and the Permissionless Factory

Duration: 5 weeks
Budget: 74,000 EUR

Two engineering tracks run in parallel this milestone: one builds the
Aquarius and StellarBroker strategy contracts plus the CCTP bridge
integration, the other builds the permissionless vault factory changes.
Neither depends on the other, so they don't add to each other's calendar
time, only to the milestone's total budget and staffing.

### Deliverable 1.1 — Aquarius strategy contract

A new guard contract letting the vault deposit into and withdraw from
Aquarius, Stellar's largest decentralized exchange by total value locked,
audited by CoinFabrik and Certora. It exposes the same functions the
vault's existing strategy contracts already use — a value-reporting
function, a proportional-withdrawal function, an asset-in-use check, and
the liquidity-management functions themselves — and underneath calls
Aquarius's own pool contract.

Sub-deliverable 1.1.a — Interface research and contract design.
Duration: 0.5 week. Budget: 4,500 EUR.
What it is: confirm Aquarius's actual pool contract interface directly,
and finalize this contract's design against it.
Done when: the design approach is written down and agreed before
implementation starts.

Sub-deliverable 1.1.b — Implementation.
Duration: 1 week. Budget: 9,000 EUR.
What it is: write the contract. The comparable contract already live in
this codebase is 650 lines of code — the realistic size baseline.
Done when: the contract builds cleanly and passes the project's standard
lint checks.

Sub-deliverable 1.1.c — Testing and internal security review.
Duration: 0.5 week. Budget: 4,500 EUR.
What it is: unit and integration tests, plus an internal review.
Done when: test coverage is at least 85 percent, and a deposit,
valuation read, and withdrawal are shown working against Aquarius in a
test environment.

Deliverable 1.1 total: 2 weeks, 18,000 EUR.

### Deliverable 1.2 — StellarBroker execution router contract

A new guard contract letting the vault route a single trade across
multiple Stellar liquidity venues at once through StellarBroker's on-chain
settlement contract, reusing the design and testing patterns from
Deliverable 1.1.

Sub-deliverable 1.2.a — Design and interface mapping.
Duration: 0.5 week. Budget: 4,000 EUR.
Done when: the design approach is written down and agreed.

Sub-deliverable 1.2.b — Implementation.
Duration: 1 week. Budget: 8,000 EUR.
Done when: the contract builds cleanly and passes standard lint checks.

Sub-deliverable 1.2.c — Testing and internal security review.
Duration: 0.5 week. Budget: 4,000 EUR.
Done when: test coverage is at least 85 percent, and a trade is shown
executing across at least two liquidity venues in one transaction.

Deliverable 1.2 total: 2 weeks, 16,000 EUR.

### Deliverable 1.3 — Circle CCTP cross-chain deposit integration

A front-end and relayer integration letting a depositor bring USDC from
another blockchain into their Stellar account via Circle's already-deployed,
already-audited Cross-Chain Transfer Protocol contracts. No new contract is
needed on Elyx's side.

Sub-deliverable 1.3.a — Relayer implementation.
Duration: 0.5 week. Budget: 3,500 EUR.
What it is: build the relayer call, with particular care that the
destination-address fields are set to the bridge's own forwarding contract
rather than the end recipient — getting this wrong on the source chain
loses funds permanently, with no retry path.
Done when: a code review confirms those fields are handled correctly.

Sub-deliverable 1.3.b — Verification.
Duration: 0.5 week. Budget: 2,500 EUR.
Done when: a real cross-chain transfer is shown moving funds from another
chain into a Stellar account and into a vault deposit, on mainnet or
testnet.

Deliverable 1.3 total: 1 week, 6,000 EUR.

### Deliverable 1.4 — Permissionless vault factory

The factory contract today only lets an administrator register and seed a
vault that someone already deployed separately; it doesn't deploy vaults
itself. This deliverable makes vault creation genuinely self-serve: the
factory deploys new vault instances itself, from one fixed, already-audited
vault contract template, so a creator supplies parameters — manager, base
asset, fee settings, initial strategy selection from the approved list —
and never touches raw contract bytecode. This also resolves a real
technical wrinkle: the vault's share token has to be deployed with the
vault's own address already set as its administrator, which means the
vault's address has to be predictable before either contract exists, which
only a factory-driven deployment can do cleanly.

Sub-deliverable 1.4.a — Design and deployment-approach confirmation.
Duration: 1 week. Budget: 8,500 EUR.
What it is: confirm the exact contract-deployment approach against the
Soroban SDK version this project uses, and finalize the new registration
entry point's design.
Done when: the design approach is written down and agreed, including how
the existing asset/guard whitelist checks apply to self-serve vaults.

Sub-deliverable 1.4.b — Implementation.
Duration: 2 weeks. Budget: 17,000 EUR.
What it is: build the new permissionless registration path and the
factory-driven deployment logic, without changing how existing vaults are
created or how they operate.
Done when: the new code builds cleanly and passes standard lint checks,
and existing vault creation still works exactly as before.

Sub-deliverable 1.4.c — Testing.
Duration: 1 week. Budget: 8,500 EUR.
What it is: verify the new path end to end.
Done when: a new vault is created permissionlessly, seeded, and shown
accepting a deposit, without any administrator action.

Deliverable 1.4 total: 4 weeks, 34,000 EUR, running in parallel with
Deliverables 1.1–1.3 on a second engineering track.

Milestone 1 total: 5 weeks, 74,000 EUR.

---

## Milestone 2 — Institutional Onboarding, Fiat On-Ramp Foundation, and the Agent Layer

Duration: 8 weeks
Budget: 70,200 EUR

Three tracks run for at least part of this milestone: institutional KYC
onboarding, the MoneyGram integration, and the vault setup/rebalancing
agent layer. None of the three blocks the others.

### Deliverable 2.1 — Institutional KYC onboarding pathway

Deployment of Stellar's Anchor Platform, integration with a third-party
verification provider, and a relayer service that adds an approved
institution to the vault's existing membership allowlist. The vault
already supports a private, membership-gated deposit mode — this builds
the pipeline that feeds it.

Sub-deliverable 2.1.a — Provider selection and integration.
Duration: 2 weeks. Budget: 9,000 EUR.
Done when: a working sandbox connection to a selected verification
provider's API is in place.

Sub-deliverable 2.1.b — Anchor Platform deployment.
Duration: 2 weeks. Budget: 9,500 EUR.
Done when: Anchor Platform is deployed and configured, running
successfully in a test environment.

Sub-deliverable 2.1.c — Relayer implementation and testing.
Duration: 2 weeks. Budget: 9,500 EUR.
Done when: a test institution is added to a vault's allowlist through the
relayer without manual steps, and a deposit from that account is shown
working.

Deliverable 2.1 total: 6 weeks, 28,000 EUR.

### Deliverable 2.2 — MoneyGram Ramps fiat on and off-ramp integration

Partner onboarding with MoneyGram, letting a depositor convert cash to a
Stellar-based asset and back at a physical location in over 170 countries,
no bank account required.

Sub-deliverable 2.2.a — Partner application.
Duration: 3 weeks (includes buffer time, since this provider has no
published approval turnaround). Budget: 10,000 EUR.
Done when: the application is submitted with all required materials.

Sub-deliverable 2.2.b — Client implementation.
Duration: 2 weeks. Budget: 9,000 EUR.
Done when: the interactive deposit/withdrawal flow works in the
provider's sandbox or staging environment.

Sub-deliverable 2.2.c — Verification.
Duration: 1 week. Budget: 5,000 EUR.
Done when: a deposit and a withdrawal are shown working through the
integration, in production if partner approval has landed by this point,
otherwise in the provider's staging environment.

Deliverable 2.2 total: 6 weeks (parallel with Deliverable 2.1), 24,000 EUR.

### Deliverable 2.3 — Mercuryo card-based on-ramp addition

A second, card-based on-ramp using the same standard as Deliverable 2.2,
mostly configuration and testing work rather than a second full
integration.

Sub-deliverable 2.3.a — Configuration.
Duration: 1 week, starting once Deliverable 2.2's shared client exists.
Budget: 3,600 EUR.
Done when: the provider is selectable and returns a valid quote.

Sub-deliverable 2.3.b — Testing.
Duration: 1 week. Budget: 3,000 EUR.
Done when: a card-based deposit is shown working into a vault.

Deliverable 2.3 total: 2 weeks, 6,600 EUR.

### Deliverable 2.4 — Vault setup and rebalancing agent layer

An optional agent layer with two uses: helping a creator configure a new
vault in plain conversation instead of raw contract calls, and proposing
day-to-day rebalancing once a vault is running. Both uses operate through
the vault's existing `trader` role, which can already only call whitelisted
strategy operations and never touch fees, guards, or asset lists — the
agent gets that same limited key, never a manager key, so every proposal it
makes still has to clear the vault's existing checks. No new smart contract
is required; this is an off-chain orchestration layer sitting in front of
functionality the vault already exposes.

Sub-deliverable 2.4.a — Vault setup agent.
Duration: 1 week. Budget: 4,500 EUR.
What it is: a conversational flow that helps a creator choose an asset mix,
select strategies from the approved list, and set fee parameters, then
submits the resulting configuration calls.
Done when: a new vault is configured end to end through the conversational
flow instead of raw contract calls.

Sub-deliverable 2.4.b — Rebalancing agent.
Duration: 1 week. Budget: 4,500 EUR.
What it is: an agent that observes a vault's positions and proposes a
rebalancing transaction, submitted through the trader role.
Done when: the agent proposes a transaction and it is correctly accepted
or rejected by the vault's existing checks, the same way a human trader's
transaction would be.

Sub-deliverable 2.4.c — Testing.
Duration: 0.5 week. Budget: 2,600 EUR.
Done when: at least one proposal that should be rejected by the vault's
existing loss-guard is shown being correctly rejected, confirming the
agent has no way around it.

Deliverable 2.4 total: 2.5 weeks, 11,600 EUR, running in parallel with
Deliverables 2.1–2.3.

Milestone 2 total: 8 weeks, 70,200 EUR. (Deliverables 2.1, 2.2, and 2.4 all
run across the first six to seven weeks; the milestone's eight-week total
includes a buffer for Deliverable 2.2's partner-approval risk, and
Deliverable 2.3 completes inside the same window.)

---

## Milestone 3 — Additional On-Ramp Coverage, Full Integration Testing, and Closeout

Duration: 5 weeks
Budget: 49,000 EUR

This milestone adds the final on-ramp provider, verifies every piece
delivered across all three milestones works correctly together, and closes
out the work.

### Deliverable 3.1 — BlindPay regional on-ramp addition

A third fiat on and off-ramp provider covering local bank-transfer rails in
Brazil, Mexico, and Colombia, integrated against its own self-serve API and
software development kit.

Sub-deliverable 3.1.a — API integration.
Duration: 1 week. Budget: 5,000 EUR.
Done when: a successful sandbox transaction is confirmed.

Sub-deliverable 3.1.b — Testing.
Duration: 0.5 week. Budget: 3,000 EUR.
Done when: a deposit and a withdrawal are shown working through this
provider into and out of a vault.

Deliverable 3.1 total: 1.5 weeks, 8,000 EUR.

### Deliverable 3.2 — Full cross-integration testing

End-to-end testing of everything delivered across all three milestones
operating together, rather than each piece checked only on its own. This is
the highest-priced deliverable in the plan on purpose.

Sub-deliverable 3.2.a — On-chain integration testing.
Duration: 1 week. Budget: 13,000 EUR.
Done when: the vault's existing loss-guard is shown correctly governing
the Aquarius and StellarBroker strategies, and a permissionlessly-created
vault behaves the same way an administrator-created one does.

Sub-deliverable 3.2.b — Off-chain integration testing.
Duration: 1 week. Budget: 13,000 EUR.
Done when: deposits from every funding source — KYC-gated, MoneyGram,
Mercuryo, BlindPay, and CCTP — are shown landing correctly in vault
accounting, and any issues found are documented and tracked.

Deliverable 3.2 total: 2 weeks, 26,000 EUR.

### Deliverable 3.3 — Documentation and closeout

Final documentation, a closeout review, and a written audit-readiness
brief for the new contracts, meant to be handed to a third-party auditor in
the recommended follow-on engagement.

Sub-deliverable 3.3.a — Documentation.
Duration: 1 week. Budget: 7,000 EUR.
Done when: documentation covering every deliverable's mechanism and
operation is delivered.

Sub-deliverable 3.3.b — Closeout and audit-readiness brief.
Duration: 0.5 week. Budget: 8,000 EUR.
Done when: a closeout review is recorded and a standalone audit-readiness
document for the new contracts is delivered.

Deliverable 3.3 total: 1.5 weeks, 15,000 EUR.

Milestone 3 total: 5 weeks, 49,000 EUR.

---

## Summary

Milestone 1 — New Strategy Contracts, First Bridge Integration, and the
Permissionless Factory: 5 weeks, 74,000 EUR, 4 deliverables, 10
sub-deliverables.

Milestone 2 — Institutional Onboarding, Fiat On-Ramp Foundation, and the
Agent Layer: 8 weeks, 70,200 EUR, 4 deliverables, 11 sub-deliverables.

Milestone 3 — Additional On-Ramp Coverage, Full Integration Testing, and
Closeout: 5 weeks, 49,000 EUR, 3 deliverables, 6 sub-deliverables.

Total duration — 18 weeks, approximately 4.5 months.
Total budget — 193,200 EUR.
Total sub-deliverables — 27.

The one item this plan deliberately does not fund is a full third-party
security audit of the new contracts — flagged above as a residual risk and
a recommended follow-on, not silently omitted.
