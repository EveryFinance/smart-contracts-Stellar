# Oracle Contracts

## Overview

The protocol uses a **three-tier oracle resolution** inside `AssetHandler` to price any registered asset:

```
AssetHandler::get_price(asset)
  │
  ├─ Tier 1: Per-asset oracle (optional override)
  │     try_invoke_contract — gracefully catches reverts
  │     Returns > 0 → use it; reverts or 0 → fall through
  │
  ├─ Tier 2: Primary global oracle (ReflectorAdapter)
  │     try_invoke_contract — gracefully catches reverts
  │     Returns > 0 → use it; reverts or 0 → fall through
  │
  ├─ Tier 3: Fallback global oracle (DIAAdapter)
  │     invoke_contract — hard fail on revert
  │     Returns > 0 → use it; 0 → panic PriceNotAvailable
  │
  └─ Panic PriceNotAvailable
```

Every oracle — per-asset, primary, or fallback — must expose:

```rust
fn get_price(asset: Address) -> i128   // PRICE_PRECISION-scaled (10_000_000); 0 = unavailable
```

---

## 1. ReflectorAdapter

Path: `contracts/oracle_adapters/reflector`

Wraps the [Reflector](https://reflector.network) decentralized oracle network.

### How it works

Calls `Reflector::lastprice(Asset::Stellar(asset)) -> Option<PriceData>` and normalizes from Reflector's decimal precision (typically 8) to `PRICE_PRECISION` (7):

```
normalized = reflector_price * 10_000_000 / 10^decimals
```

Decimal precision is read from Reflector at initialization and cached — no extra cross-contract call per price query.

### Public Methods

**Admin:**
- `__constructor(admin, reflector)` — init, queries and stores `reflector.decimals()`
- `set_reflector(caller, reflector)` — update Reflector address + re-query decimals
- `refresh_decimals(caller)` — re-read decimals if Reflector ever changes them
- `set_pending_admin(caller, new_admin)` / `accept_admin(caller)` — two-step transfer

**Oracle interface:**
- `get_price(asset) -> i128` — returns 0 if Reflector has no data for the asset

**Views:**
- `get_admin`, `get_reflector`, `get_decimals`

### Deployment

```
asset_handler.set_primary_oracle(admin, reflector_adapter_address)
```

---

## 2. DIAAdapter

Path: `contracts/oracle_adapters/dia`

Wraps the [DIA](https://diadata.org) cross-chain oracle.

### How it works

DIA identifies assets by string pair key (e.g. `"BTC/USD"`, `"XLM/USD"`), not by address. The adapter maintains an admin-managed `Address → String` mapping and calls `DIA::read_oracle_value(key) -> OracleValue`.

DIA uses 8 fixed decimal places. Normalization:

```
normalized = dia_price * 10_000_000 / 100_000_000
```

The call uses `try_invoke_contract` — if DIA reverts, `get_price` returns 0 gracefully.

### Asset Key Registration

Before `get_price` can return a non-zero value for an asset, the admin must register its DIA key:

```
dia_adapter.set_asset_key(admin, usdc_address, "USDC/USD")
dia_adapter.set_asset_key(admin, xlm_address,  "XLM/USD")
dia_adapter.set_asset_key(admin, btc_address,  "BTC/USD")
```

### Public Methods

**Admin:**
- `__constructor(admin, dia_contract)` — init
- `set_asset_key(caller, asset, key)` — register Address → DIA key mapping
- `remove_asset_key(caller, asset)` — remove mapping
- `set_dia_contract(caller, dia)` — update DIA contract address
- `set_pending_admin(caller, new_admin)` / `accept_admin(caller)` — two-step transfer

**Oracle interface:**
- `get_price(asset) -> i128` — returns 0 if no key registered or DIA reverts

**Views:**
- `get_admin`, `get_dia_contract`, `get_asset_key(asset) -> Option<String>`

### Deployment

```
asset_handler.set_fallback_oracle(admin, dia_adapter_address)
```

---

## 3. Oracle (Dev/Test Mock)

Path: `contracts/oracle`

> **DO NOT deploy in production.** Prices are set manually by an admin — centralized, no staleness guarantees.

Used in local tests and testnet deployments where Reflector/DIA are unavailable or deterministic prices are needed.

### Public Methods

**Admin:**
- `__constructor(admin, max_age_ledgers)`
- `set_admin(new_admin)`
- `set_max_age_ledgers(max_age_ledgers)`
- `set_price(asset, price)`

**Oracle interface:**
- `get_price(asset) -> i128`
- `get_prices(assets) -> Map<Address, i128>`

**Views:**
- `get_admin`, `get_max_age_ledgers`

### Freshness

- Reads enforce `current_ledger - updated_ledger <= max_age_ledgers` when `max_age_ledgers > 0`.
- Violation raises `StalePrice`.
- `max_age_ledgers = 0` disables staleness checks.

---

## 4. AssetHandler

Path: `contracts/asset_handler`

Registry of assets and the three-tier oracle configuration. Used by strategies and the vault to price portfolio assets.

### Public Methods

**Asset registry (admin):**
- `add_asset(caller, asset)` / `remove_asset(caller, asset)`
- `is_registered(asset) -> bool`, `get_all_assets() -> Vec<Address>`

**Oracle configuration (admin):**
- `set_primary_oracle(caller, oracle)` / `get_primary_oracle() -> Option<Address>`
- `set_fallback_oracle(caller, oracle)` / `get_fallback_oracle() -> Option<Address>`
- `set_asset_oracle(caller, asset, oracle)` — per-asset override
- `remove_asset_oracle(caller, asset)`
- `get_asset_oracle(asset) -> Option<Address>`

**Admin transfer:**
- `set_pending_admin(caller, new_admin)` / `accept_admin(caller)`

**Price query:**
- `get_price(asset) -> i128` — three-tier resolution described above
