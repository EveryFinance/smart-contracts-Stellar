# Mainnet Post-Deployment Fixes — 2026-05-14 to 2026-05-18

Documents the two issues discovered after the initial mainnet deployment
and the on-chain transactions that resolved them.

---

## Fix 1 — USDC Fixed-Price Oracle

### Problem

Reflector provides no USDC/USD price feed. USDC is a USD stablecoin — its
price is always $1.00 by definition — so Reflector simply does not list it.
This caused every vault NAV computation to fail whenever USDC was part of
the portfolio, blocking all deposits and withdrawals.

### Solution

A **fixed-price oracle** was deployed and registered in AssetHandler as a
per-asset override specifically for USDC. It always returns
`10,000,000` (≡ $1.00 with 7-decimal precision).

The oracle's `max_age_ledgers` staleness guard was subsequently set to
`2147483647` (effectively unlimited) to prevent the 24-hour freshness
check from blocking NAV snapshots. The oracle's `set_price` still needs
to be called periodically to keep the Soroban storage entry alive and
avoid TTL expiry.

### On-chain transactions

| Step | Tx hash | Explorer |
|------|---------|----------|
| Deploy fixed-price oracle | `5b88bc9f...` | [view tx](https://stellar.expert/explorer/public/tx/5b88bc9f78ed71d31ebad932d93b4d80bb3d96826b72a9bb3ebdd12b773de652) |
| Set USDC price = 10,000,000 ($1.00) | `06ef1eb2...` | [view tx](https://stellar.expert/explorer/public/tx/06ef1eb2f94290f199b01cf5f9f619f2e8a596ae5d93a8c2a6f38897f5e388ea) |
| Register oracle in AssetHandler for USDC | `1bbf7ca3...` | [view tx](https://stellar.expert/explorer/public/tx/1bbf7ca36ee1d6c8414d7d5ae399cbffbddb5fd9452238633f6c7064aec020f5) |
| Set `max_age_ledgers` to 2,147,483,647 | `cc0ee60f...` | [view tx](https://stellar.expert/explorer/public/tx/cc0ee60fc501a2a96b46c2b5d67f920af6fb1754ec93856a519f7595a6a77996) |

**Fixed-price oracle address:** `CCLT42BFIS6FX6V7KYDJV7Y4G2JAY7KKA65NABN7FBXNEQBJHW4PQTZJ`

---

## Fix 2 — Blend Strategy V2 API Incompatibilities

### Problem

The original Blend strategy contracts were written against the Blend V1 API.
After deployment on mainnet, four bugs were discovered when the manager
attempted to execute `supply` and `withdraw_from_lending` operations:

1. **`blend_submit` return type mismatch** — Blend V2's `submit` function
   returns a `Positions` struct, not `void`. Calling it with the V1 signature
   caused a deserialization panic on every invocation.

2. **Non-existent `get_supply` function** — The strategy called
   `pool.get_supply(account)` to read the current lending position. Blend V2
   removed this function. It must be replaced by:
   - `pool.get_positions(account)` — returns the b-token balance map
   - `pool.get_reserve(asset)` — returns the reserve config including the
     current b-rate
   - Formula: `underlying = b_tokens × b_rate / 10¹²`

3. **NAV computation aborted on USDC oracle missing** — After a supply or
   withdrawal, the vault re-computes NAV by calling
   `asset_handler.get_prices(assets)`. When USDC was in the asset list and
   the Reflector oracle had no USDC feed, this cross-contract call panicked
   and reverted the entire strategy call. Fixed by wrapping the
   `get_prices` call with `try_invoke_contract` and falling back to
   `PRICE_PRECISION` (≡ $1.00) for any asset whose price cannot be fetched.

4. **Withdrawal clamp for b-rate rounding** — Blend V2 issues slightly
   fewer b-tokens than the deposited amount implies due to pool utilization
   rounding. This means the redeemable underlying is typically 1–2 stroops
   less than the amount originally deposited. Passing the original deposit
   amount as the withdrawal amount caused an `InsufficientPosition` error.
   Fixed by clamping: `withdraw_amount = min(requested, current_position)`.

### Resolution

All four bugs were fixed in the Blend strategy source code and three new
contracts were deployed to mainnet on 2026-05-18.

**WASM hash:** `46cdf5a1ad8658da782bd7229f815455dd15fad5df345334596d5178bf0c5ef4`

| Vault | Old address (V1, decommissioned) | New address (V2, active) |
|-------|----------------------------------|--------------------------|
| Alpha | `CDVEOBXRSIKKK36F33A73AJQKUWYTM6UIA3ZKE2M7DYWRYJRMXMMWUSO` | `CBD7QEXZP2RVIEFD4OUWRDAXKB2BM4GKUME3EZQQEBWAABL3IEGXPGKI` |
| Beta  | `CCWTR2HZQTAJIZADEJ2DLZPYWX55H2KAZJJU4CE63347Z4HNNXX2TBHL` | `CBO5XSLPO4DCJJSWWWCPHZ6JDFKPFBPMQDLJWUCJJ3PEWRYO7V6JOJ7V` |
| Gamma | `CCI7UFFHB3GANQ2WU4DJ5I4EMBMVTULAG6KLIHQS6NGVDK4TOEY5LEYR` | `CDMPATIFU2P7JRRAQZZ3655IZSNON62V3EUZK2UZH33C7ACQF6EQ2HYM` |

The old contracts remain on-chain but are no longer registered as active
guards in any vault.
