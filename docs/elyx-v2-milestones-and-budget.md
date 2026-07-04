# Elyx v2 — Milestones, Deliverables, and Budget

Status: proposal / pre-implementation
Branch: feature/elyx-v2-ecosystem-architecture
Date: 2026-07-03

This document lays out the delivery plan for the Elyx v2 Stellar ecosystem
integration work as three milestones, each with its own deliverables. Every
deliverable states what it is, its duration, its budget, and how completion
is measured. Durations and budgets are grounded in the actual engineering
effort each deliverable requires, not arbitrary figures — the two new
smart-contract deliverables in particular are estimated against the size and
test coverage of an existing, comparable contract already live in this
codebase, not guessed.

Total duration across all three milestones: 18 weeks, approximately 4.5
months.
Total budget across all three milestones: 147,600 EUR.

The six integrations covered are Aquarius (a new automated market maker
strategy), StellarBroker (an execution router across multiple Stellar
liquidity venues), Circle's Cross-Chain Transfer Protocol (a cross-chain
USDC bridge), Anchor Platform paired with the SEP-12 standard (an
institutional KYC onboarding pathway), and a three-provider fiat on and
off-ramp made up of MoneyGram Ramps, Mercuryo, and BlindPay.

---

## Milestone 1 — New Strategy Contracts and First Bridge Integration

Duration: 5 weeks
Budget: 40,000 EUR

This milestone covers the two integrations that require writing genuinely
new smart contracts, plus the one cross-chain integration that requires no
new contract at all, so that the fastest, lowest-risk piece ships first
while the two contract builds are underway.

Deliverable 1.1 — Aquarius strategy contract
Description: a new guard contract that lets the vault deposit into and
withdraw from Aquarius, currently Stellar's largest decentralized exchange
by total value locked. The contract is designed, implemented, and tested to
the same standard as the vault's existing strategy contracts, exposing the
same four functions those already use: a function to report the current
position value, a function to withdraw a proportional share of the
position, a function to check whether a given asset is in use, and the
liquidity-management functions themselves.
Duration: 2 weeks
Budget: 18,000 EUR
Budget and duration justification: this is a new contract handling real
depositor funds, not a simple API integration. The existing comparable
strategy contract already in production in this codebase is 650 lines of
contract code backed by 1,345 lines of automated tests across 43 individual
test cases. Matching that same standard for a new contract realistically
takes two weeks of senior smart-contract engineering time, and the budget
reflects that level of effort plus a scoped internal security review before
the contract is allowed to touch live funds.
How completion is measured: the contract is deployed and its full test
suite passes with coverage comparable to the existing strategy contracts
in this codebase; the contract is added to the platform's approved-contract
list and activated on at least one live vault; a real deposit, valuation
read, and partial withdrawal are demonstrated end to end against the live
Aquarius pool.

Deliverable 1.2 — StellarBroker execution router contract
Description: a new guard contract that lets the vault route a single trade
across multiple Stellar liquidity venues at once through StellarBroker,
rather than being limited to whichever single venue a simpler integration
would pick. This removes the need to build and maintain a separate contract
for every individual exchange venue.
Duration: 2 weeks
Budget: 16,000 EUR
Budget and duration justification: same reasoning as Deliverable 1.1 — this
is a new contract, not a wrapper — but the budget is set slightly lower
because the design and testing patterns from Deliverable 1.1 are directly
reusable, reducing the effort needed the second time.
How completion is measured: the contract is deployed and tested to the same
standard as Deliverable 1.1; a trade is demonstrated being split and
executed across at least two underlying liquidity venues in a single
transaction; the contract is activated on at least one live vault.

Deliverable 1.3 — Circle CCTP cross-chain deposit integration
Description: a front-end and relayer integration allowing a depositor to
bring USDC from another blockchain directly into their Stellar account
using Circle's Cross-Chain Transfer Protocol, which is already deployed and
audited on Stellar mainnet by Circle itself. No new smart contract is
required on Elyx's side — this integration calls an already-deployed,
already-audited public contract.
Duration: 1 week
Budget: 6,000 EUR
Budget and duration justification: because the underlying contract already
exists and is already audited, this is genuinely a lighter integration than
Deliverables 1.1 and 1.2 — the work is front-end and relayer logic, plus
careful handling of one specific failure mode: the destination address
fields in the cross-chain message must be set correctly, since funds sent
to the wrong address in this protocol are permanently unrecoverable. The
budget reflects careful, tested implementation of that detail rather than a
rushed integration.
How completion is measured: a real cross-chain transfer is demonstrated
moving USDC from an external chain into a Stellar account and then into a
vault deposit, end to end, on mainnet.

---

## Milestone 2 — Institutional Onboarding and Fiat On-Ramp Foundation

Duration: 8 weeks
Budget: 58,600 EUR

This milestone covers the two longest-running pieces of work, which can
proceed in parallel since neither depends on the other, plus one additional
on-ramp provider that is added once the shared infrastructure from the
larger of the two pieces of work exists.

Deliverable 2.1 — Institutional KYC onboarding pathway
Description: deployment of Stellar's Anchor Platform, integration with a
third-party KYC and business-verification provider, and a relayer service
that adds an approved institution to the vault's existing membership
allowlist once that institution clears verification. The vault's contracts
already support a private, membership-gated deposit mode; this deliverable
builds the compliance pipeline that feeds it, not new contract logic.
Duration: 6 weeks
Budget: 28,000 EUR
Budget and duration justification: although no new smart contract is
required, this deliverable involves selecting and integrating a third-party
verification provider, building and testing the relayer service, and
coordinating custody arrangements for the institutional-facing account
roles. Six weeks and this budget level reflect genuine vendor integration
and coordination work, not a simple configuration change.
How completion is measured: a test institution is taken through the full
verification flow and is successfully added to a live vault's allowlist by
the relayer service without manual intervention; a deposit from that
institution's account is demonstrated succeeding, and a deposit attempt
from a non-approved account is demonstrated correctly failing.

Deliverable 2.2 — MoneyGram Ramps fiat on and off-ramp integration
Description: partner onboarding with MoneyGram and integration of their
Ramps product, which allows a depositor to convert cash to a Stellar-based
asset and back at a physical MoneyGram location, in over 170 countries,
without needing a bank account. This is a front-end and partner-integration
deliverable; no vault contract changes are required.
Duration: 6 weeks, run in parallel with Deliverable 2.1
Budget: 24,000 EUR
Budget and duration justification: the technical integration itself is
moderate in size, but the partner approval process for this specific
provider has no publicly committed turnaround time, which is the single
largest schedule risk in this entire plan. The duration and budget include
a realistic buffer for that approval process rather than assuming a
best-case turnaround.
How completion is measured: a real cash-to-asset deposit and a real
asset-to-cash withdrawal are demonstrated end to end through the live
partner integration, and the resulting funds are shown moving into and out
of a live vault.

Deliverable 2.3 — Mercuryo card-based on-ramp addition
Description: addition of a second, card-based on-ramp provider using the
same interactive deposit-and-withdrawal standard already being integrated
for Deliverable 2.2, allowing depositors without cash access to fund a
vault using a debit or credit card, or a mobile wallet such as Apple Pay or
Google Pay.
Duration: 2 weeks, beginning once the shared infrastructure from
Deliverable 2.2 exists, finishing within this milestone's overall window
Budget: 6,600 EUR
Budget and duration justification: because this provider uses the same
underlying standard as Deliverable 2.2, adding it is close to configuration
and testing work rather than a second full integration, which is reflected
in the smaller budget and duration relative to Deliverables 2.1 and 2.2.
How completion is measured: a real card-based deposit is demonstrated
succeeding end to end into a live vault.

Milestone-level note on duration: Deliverables 2.1 and 2.2 run in parallel
across the first six weeks of this milestone. The milestone's eight-week
total duration includes a two-week buffer specifically to absorb schedule
risk from Deliverable 2.2's partner-approval process, which is the plan's
single biggest known risk. Deliverable 2.3 begins around week five and
completes inside the same eight-week window.

---

## Milestone 3 — Additional On-Ramp Coverage, Full Integration Testing, and Closeout

Duration: 5 weeks
Budget: 49,000 EUR

This milestone adds the final on-ramp provider, verifies every integration
delivered across all three milestones works correctly together rather than
only in isolation, and closes out the work with documentation and a final
review.

Deliverable 3.1 — BlindPay regional on-ramp addition
Description: integration of a third fiat on and off-ramp provider
specializing in local bank-transfer rails in Latin America, specifically
Brazil, Mexico, and Colombia, for depositors who are best served by local
settlement rails rather than cash pickup or card payment.
Duration: 1.5 weeks
Budget: 12,000 EUR
Budget and duration justification: this provider offers a self-serve
integration path with its own publicly documented API and software
development kit, which is reflected in a shorter duration and smaller
budget than either on-ramp deliverable in Milestone 2.
How completion is measured: a real deposit and withdrawal are demonstrated
end to end through this provider into and out of a live vault.

Deliverable 3.2 — Full cross-integration testing
Description: end-to-end testing of all six integrations operating together
against live vaults, rather than each integration being verified only in
isolation as in the prior two milestones. This includes verifying that the
vault's existing risk controls correctly govern the two new strategy
contracts from Milestone 1 under realistic conditions, and that deposits
arriving through any of the on-ramp or bridge integrations correctly flow
through to vault accounting.
Duration: 2 weeks
Budget: 22,000 EUR
Budget and duration justification: this deliverable is deliberately
budgeted at a similar level to the individual contract-build deliverables
in Milestone 1, because verifying six integrations working correctly
together, including under adverse and edge-case conditions, is
substantively its own body of work, not a formality performed after the
real work is done.
How completion is measured: a documented test report covering all six
integrations operating together, run against a live or live-equivalent
environment, with any issues found during this testing resolved before
sign-off.

Deliverable 3.3 — Documentation and closeout
Description: final technical and user-facing documentation covering all six
integrations, a final internal review of the completed work against the
original scope, and formal closeout of the milestone plan.
Duration: 1.5 weeks
Budget: 15,000 EUR
Budget and duration justification: documentation and final review work for
a body of work this size — six integrations, two new smart contracts, and
one new compliance pathway — realistically requires dedicated time rather
than being compressed into the margins of other deliverables, which the
budget and duration reflect.
How completion is measured: complete technical documentation is delivered
and published, a final review sign-off is recorded, and every completion
measure listed for every deliverable across all three milestones is
confirmed satisfied.

---

## Summary

Milestone 1 — 5 weeks, 40,000 EUR
Milestone 2 — 8 weeks, 58,600 EUR
Milestone 3 — 5 weeks, 49,000 EUR

Total duration — 18 weeks, approximately 4.5 months
Total budget — 147,600 EUR
