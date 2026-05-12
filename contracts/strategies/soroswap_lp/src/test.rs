#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl, contracttype, testutils::Address as _, Address, Env, String,
};

use crate::{
    interfaces::{OracleAdapter, PairAdapter},
    SoroswapLpStrategy, SoroswapLpStrategyClient,
};

// ---------------------------------------------------------------------------
// MockToken (same pattern as blend tests)
// ---------------------------------------------------------------------------

#[contracttype]
enum TKey {
    Balance(Address),
    TotalSupply,
}
#[contract]
pub struct MockToken2;
#[contractimpl]
impl MockToken2 {
    pub fn initialize(_env: Env, _admin: Address) {}
    pub fn mint(env: Env, to: Address, amount: i128) {
        let b: i128 = env
            .storage()
            .persistent()
            .get(&TKey::Balance(to.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&TKey::Balance(to.clone()), &(b + amount));
        let s: i128 = env
            .storage()
            .persistent()
            .get(&TKey::TotalSupply)
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&TKey::TotalSupply, &(s + amount));
    }
    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&TKey::Balance(id))
            .unwrap_or(0)
    }
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        let fb: i128 = env
            .storage()
            .persistent()
            .get(&TKey::Balance(from.clone()))
            .unwrap_or(0);
        assert!(fb >= amount, "insufficient balance");
        env.storage()
            .persistent()
            .set(&TKey::Balance(from), &(fb - amount));
        let tb: i128 = env
            .storage()
            .persistent()
            .get(&TKey::Balance(to.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&TKey::Balance(to), &(tb + amount));
    }
    pub fn approve(_env: Env, _f: Address, _s: Address, _a: i128, _e: u32) {}
    pub fn allowance(_env: Env, _f: Address, _s: Address) -> i128 {
        i128::MAX
    }
    pub fn transfer_from(env: Env, _sp: Address, from: Address, to: Address, amount: i128) {
        let fb: i128 = env
            .storage()
            .persistent()
            .get(&TKey::Balance(from.clone()))
            .unwrap_or(0);
        assert!(fb >= amount);
        env.storage()
            .persistent()
            .set(&TKey::Balance(from), &(fb - amount));
        let tb: i128 = env
            .storage()
            .persistent()
            .get(&TKey::Balance(to.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&TKey::Balance(to), &(tb + amount));
    }
    pub fn burn(env: Env, from: Address, amount: i128) {
        from.require_auth();
        let b: i128 = env
            .storage()
            .persistent()
            .get(&TKey::Balance(from.clone()))
            .unwrap_or(0);
        assert!(b >= amount);
        env.storage()
            .persistent()
            .set(&TKey::Balance(from), &(b - amount));
    }
    pub fn total_supply(env: Env) -> i128 {
        env.storage()
            .persistent()
            .get(&TKey::TotalSupply)
            .unwrap_or(0)
    }
    pub fn decimals(_env: Env) -> u32 {
        7
    }
    pub fn name(env: Env) -> String {
        String::from_str(&env, "Mock")
    }
    pub fn symbol(env: Env) -> String {
        String::from_str(&env, "MCK")
    }
}

// ---------------------------------------------------------------------------
// MockSoroswapRouter
// ---------------------------------------------------------------------------

/// Simplified router: add_liquidity mints LP 1:1 with min(a,b), remove_liquidity
/// burns LP and returns equal amounts of token_a and token_b to recipient.
#[contracttype]
enum RouterKey {
    LpToken,
    Overuse,
    NoMint,
    BurnLp,
}
#[contract]
pub struct MockSoroswapRouter;
#[contractimpl]
impl MockSoroswapRouter {
    pub fn init(env: Env, lp_token: Address) {
        env.storage().instance().set(&RouterKey::LpToken, &lp_token);
    }

    pub fn set_overuse(env: Env, enabled: bool) {
        env.storage().instance().set(&RouterKey::Overuse, &enabled);
    }

    pub fn set_no_mint(env: Env, enabled: bool) {
        env.storage().instance().set(&RouterKey::NoMint, &enabled);
    }

    pub fn set_burn_lp(env: Env, enabled: bool) {
        env.storage().instance().set(&RouterKey::BurnLp, &enabled);
    }

    pub fn add_liquidity(
        env: Env,
        _token_a: Address,
        _token_b: Address,
        amount_a: i128,
        amount_b: i128,
        _min_a: i128,
        _min_b: i128,
        to: Address,
        _deadline: u64,
    ) -> (i128, i128, i128) {
        let overuse: bool = env
            .storage()
            .instance()
            .get(&RouterKey::Overuse)
            .unwrap_or(false);
        if overuse {
            return (amount_a + 1, amount_b, 0);
        }

        let lp_minted = amount_a.min(amount_b); // simplified
        let lp: Address = env.storage().instance().get(&RouterKey::LpToken).unwrap();
        let lp_client = MockToken2Client::new(&env, &lp);
        let burn_lp: bool = env
            .storage()
            .instance()
            .get(&RouterKey::BurnLp)
            .unwrap_or(false);
        let no_mint: bool = env
            .storage()
            .instance()
            .get(&RouterKey::NoMint)
            .unwrap_or(false);
        if burn_lp {
            let balance = lp_client.balance(&to);
            if balance > 0 {
                lp_client.burn(&to, &balance);
            }
        } else if !no_mint {
            lp_client.mint(&to, &lp_minted);
        }
        (lp_minted, lp_minted, lp_minted)
    }
    pub fn remove_liquidity(
        env: Env,
        token_a: Address,
        token_b: Address,
        liquidity: i128,
        _min_a: i128,
        _min_b: i128,
        to: Address,
        _deadline: u64,
    ) -> (i128, i128) {
        let half = liquidity / 2;
        // Burn LP from router allowance (mock: just mint tokens to recipient)
        MockToken2Client::new(&env, &token_a).mint(&to, &half);
        MockToken2Client::new(&env, &token_b).mint(&to, &half);
        (half, half)
    }
}

// ---------------------------------------------------------------------------
// MockVault — satisfies vault.get_manager() cross-contract call in initialize
// ---------------------------------------------------------------------------

#[contracttype]
enum VaultKey {
    Manager,
    Factory,
}

#[contract]
pub struct MockVault;

#[contractimpl]
impl MockVault {
    pub fn set_manager(env: Env, manager: Address) {
        env.storage().instance().set(&VaultKey::Manager, &manager);
    }
    pub fn get_manager(env: Env) -> Address {
        env.storage().instance().get(&VaultKey::Manager).unwrap()
    }
    pub fn set_factory(env: Env, factory: Address) {
        env.storage().instance().set(&VaultKey::Factory, &factory);
    }
    pub fn get_factory(env: Env) -> Option<Address> {
        env.storage().instance().get(&VaultKey::Factory)
    }
}

// MockFactory — satisfies factory.get_asset_handler() in compute_lp_value

#[contracttype]
enum FactoryKey {
    AssetHandler,
}

#[contract]
pub struct MockFactory;

#[contractimpl]
impl MockFactory {
    pub fn set_asset_handler(env: Env, ah: Address) {
        env.storage().instance().set(&FactoryKey::AssetHandler, &ah);
    }
    pub fn get_asset_handler(env: Env) -> Option<Address> {
        env.storage().instance().get(&FactoryKey::AssetHandler)
    }
}

// ---------------------------------------------------------------------------
// Test setup
// ---------------------------------------------------------------------------

struct T {
    env: Env,
    strategy: SoroswapLpStrategyClient<'static>,
    token_a: Address,
    token_b: Address,
    lp_token: Address,
    router: Address,
    vault: Address,
    factory: Address,
}

fn setup() -> T {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let token_a = env.register(MockToken2, ());
    let token_b = env.register(MockToken2, ());
    let lp_token = env.register(MockToken2, ());
    let router_id = env.register(MockSoroswapRouter, ());
    MockSoroswapRouterClient::new(&env, &router_id).init(&lp_token);

    let manager = Address::generate(&env);
    let factory = env.register(MockFactory, ());
    // Register a mock vault so strategy.initialize() can cross-call
    // vault.get_manager() and vault.get_factory().
    let vault = env.register(MockVault, ());
    MockVaultClient::new(&env, &vault).set_manager(&manager);
    MockVaultClient::new(&env, &vault).set_factory(&factory);

    MockToken2Client::new(&env, &token_a).mint(&vault, &10_000_0000000i128);
    MockToken2Client::new(&env, &token_b).mint(&vault, &10_000_0000000i128);

    let sid = env.register(SoroswapLpStrategy, ());
    let strategy = SoroswapLpStrategyClient::new(&env, &sid);
    strategy.initialize(
        &vault,
        &token_a,
        &token_b,
        &lp_token,
        &router_id,
        &String::from_str(&env, "Soroswap USDC/XLM LP"),
    );

    let strategy: SoroswapLpStrategyClient<'static> = unsafe { core::mem::transmute(strategy) };

    T {
        env,
        strategy,
        token_a,
        token_b,
        lp_token,
        router: router_id,
        vault,
        factory,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_initialize() {
    let t = setup();
    assert_eq!(t.strategy.asset_a(), t.token_a);
    assert_eq!(t.strategy.asset_b(), t.token_b);
    assert_eq!(t.strategy.lp_token(), t.lp_token);
    assert_eq!(t.strategy.get_lp_balance(), 0i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_double_initialize_panics() {
    let t = setup();
    t.strategy.initialize(
        &t.vault,
        &t.token_a,
        &t.token_b,
        &t.lp_token,
        &t.router,
        &String::from_str(&t.env, "x"),
    );
}

#[test]
fn test_add_liquidity_tracks_lp() {
    let t = setup();
    let lp_before = t.strategy.get_lp_balance();
    t.strategy
        .add_liquidity(&t.vault, &100_0000000i128, &100_0000000i128, &0, &0);
    let lp_after = t.strategy.get_lp_balance();
    assert!(lp_after > lp_before);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_add_liquidity_rejects_router_overusing_inputs() {
    let t = setup();
    MockSoroswapRouterClient::new(&t.env, &t.router).set_overuse(&true);

    t.strategy
        .add_liquidity(&t.vault, &100_0000000i128, &100_0000000i128, &0, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_add_liquidity_rejects_router_that_mints_no_lp() {
    let t = setup();
    MockSoroswapRouterClient::new(&t.env, &t.router).set_no_mint(&true);

    t.strategy
        .add_liquidity(&t.vault, &100_0000000i128, &100_0000000i128, &0, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_add_liquidity_rejects_router_that_burns_existing_lp() {
    let t = setup();
    t.strategy
        .add_liquidity(&t.vault, &100_0000000i128, &100_0000000i128, &0, &0);
    MockSoroswapRouterClient::new(&t.env, &t.router).set_burn_lp(&true);

    t.strategy
        .add_liquidity(&t.vault, &100_0000000i128, &100_0000000i128, &0, &0);
}

#[test]
#[should_panic]
fn test_add_liquidity_not_vault_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.strategy.add_liquidity(&rogue, &100i128, &100i128, &0, &0);
}

#[test]
#[should_panic]
fn test_add_liquidity_zero_panics() {
    let t = setup();
    t.strategy.add_liquidity(&t.vault, &0i128, &100i128, &0, &0);
}

#[test]
fn test_remove_liquidity_returns_to_vault() {
    let t = setup();
    t.strategy
        .add_liquidity(&t.vault, &200_0000000i128, &200_0000000i128, &0, &0);
    let lp = t.strategy.get_lp_balance();
    assert!(lp > 0);
    t.strategy.remove_liquidity(&t.vault, &lp, &0, &0);
    assert_eq!(t.strategy.get_lp_balance(), 0i128);
}

#[test]
#[should_panic]
fn test_remove_liquidity_exceeds_balance_panics() {
    let t = setup();
    t.strategy
        .add_liquidity(&t.vault, &100i128, &100i128, &0, &0);
    t.strategy
        .remove_liquidity(&t.vault, &999_999_999i128, &0, &0);
}

#[test]
fn test_full_lifecycle() {
    let t = setup();
    t.strategy
        .add_liquidity(&t.vault, &500_0000000i128, &500_0000000i128, &0, &0);
    let lp = t.strategy.get_lp_balance();
    assert!(lp > 0);
    let half = lp / 2;
    t.strategy.remove_liquidity(&t.vault, &half, &0, &0);
    assert_eq!(t.strategy.get_lp_balance(), lp - half);
    t.strategy.remove_liquidity(&t.vault, &(lp - half), &0, &0);
    assert_eq!(t.strategy.get_lp_balance(), 0i128);
}

// ---------------------------------------------------------------------------
// NotInitialized — calling functions before initialize() panics
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_not_initialized_asset_a_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(SoroswapLpStrategy, ());
    let client = SoroswapLpStrategyClient::new(&env, &id);
    client.asset_a();
}

#[test]
#[should_panic]
fn test_not_initialized_add_liquidity_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(SoroswapLpStrategy, ());
    let client = SoroswapLpStrategyClient::new(&env, &id);
    let vault = Address::generate(&env);
    client.add_liquidity(&vault, &100i128, &100i128, &0, &0);
}

#[test]
#[should_panic]
fn test_not_initialized_remove_liquidity_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(SoroswapLpStrategy, ());
    let client = SoroswapLpStrategyClient::new(&env, &id);
    let vault = Address::generate(&env);
    client.remove_liquidity(&vault, &100i128, &0, &0);
}

// ---------------------------------------------------------------------------
// Withdraw edge cases
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_remove_liquidity_zero_panics() {
    let t = setup();
    t.strategy
        .add_liquidity(&t.vault, &100_0000000i128, &100_0000000i128, &0, &0);
    t.strategy.remove_liquidity(&t.vault, &0i128, &0, &0);
}

#[test]
#[should_panic]
fn test_remove_liquidity_not_vault_panics() {
    let t = setup();
    t.strategy
        .add_liquidity(&t.vault, &100_0000000i128, &100_0000000i128, &0, &0);
    let lp = t.strategy.get_lp_balance();
    let rogue = Address::generate(&t.env);
    t.strategy.remove_liquidity(&rogue, &lp, &0, &0);
}

// ---------------------------------------------------------------------------
// Negative amounts
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_add_liquidity_negative_a_panics() {
    let t = setup();
    t.strategy
        .add_liquidity(&t.vault, &-1i128, &100i128, &0, &0);
}

#[test]
#[should_panic]
fn test_add_liquidity_negative_b_panics() {
    let t = setup();
    t.strategy
        .add_liquidity(&t.vault, &100i128, &-1i128, &0, &0);
}

// ---------------------------------------------------------------------------
// View functions
// ---------------------------------------------------------------------------

#[test]
fn test_get_router() {
    let t = setup();
    assert_eq!(t.strategy.get_router(), t.router);
}

#[test]
fn test_get_name() {
    let t = setup();
    assert_eq!(
        t.strategy.get_name(),
        soroban_sdk::String::from_str(&t.env, "Soroswap USDC/XLM LP")
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_get_value_no_oracle_panics() {
    let t = setup();
    t.strategy
        .add_liquidity(&t.vault, &200_0000000i128, &200_0000000i128, &0, &0);
    t.strategy.get_value(&t.vault);
}

// ---------------------------------------------------------------------------
// Multiple deposits accumulate correctly
// ---------------------------------------------------------------------------

#[test]
fn test_multiple_deposits_lp_accumulate() {
    let t = setup();
    t.strategy
        .add_liquidity(&t.vault, &100_0000000i128, &100_0000000i128, &0, &0);
    let lp1 = t.strategy.get_lp_balance();
    t.strategy
        .add_liquidity(&t.vault, &200_0000000i128, &200_0000000i128, &0, &0);
    let lp2 = t.strategy.get_lp_balance();
    assert!(lp2 > lp1);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_swap_rejects_non_pair_asset() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.strategy
        .swap(&t.vault, &rogue, &t.token_b, &100i128, &0i128);
}

// ---------------------------------------------------------------------------
// GAP C — Reserve decomposition NAV (dHedge V2 §3)
//
// The LP token IS the Soroswap pair contract; it must expose `get_reserves()`
// in addition to the standard token interface.  We replace MockToken2 with a
// MockPairToken that tracks reserves independently from its own balance.
// ---------------------------------------------------------------------------

/// A mock Soroswap pair contract that acts as both the LP token and the
/// reserve tracker.  Supports `get_reserves()` and `set_reserves()` so tests
/// can inject arbitrary pool state without touching the router.
mod mock_pair_token_mod {
    use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};
    #[contracttype]
    pub enum PairKey {
        Balance(Address),
        TotalSupply,
        ReserveA,
        ReserveB,
        Token0,
    }
    #[contract]
    pub struct MockPairToken;
    #[contractimpl]
    impl MockPairToken {
        /// Set the token0 address (simulates Soroswap pair ordering).
        /// When not set, token0() returns a default (zero bytes) address.
        pub fn set_token0(env: Env, token: Address) {
            env.storage().instance().set(&PairKey::Token0, &token);
        }
        pub fn token0(env: Env) -> Address {
            env.storage()
                .instance()
                .get(&PairKey::Token0)
                // Return a sentinel if not set (should never happen in tests that call set_token0).
                .unwrap_or_else(|| panic!("token0 not set on MockPairToken"))
        }
        pub fn token1(env: Env) -> Address {
            // token1 is the complement of token0; not needed in current tests.
            // Returning token0 here would be wrong but token1 is not called in tests.
            env.storage()
                .instance()
                .get(&PairKey::Token0)
                .unwrap_or_else(|| panic!("token0 not set on MockPairToken"))
        }
        pub fn mint(env: Env, to: Address, amount: i128) {
            let b: i128 = env
                .storage()
                .persistent()
                .get(&PairKey::Balance(to.clone()))
                .unwrap_or(0);
            env.storage()
                .persistent()
                .set(&PairKey::Balance(to), &(b + amount));
            let s: i128 = env
                .storage()
                .persistent()
                .get(&PairKey::TotalSupply)
                .unwrap_or(0);
            env.storage()
                .persistent()
                .set(&PairKey::TotalSupply, &(s + amount));
        }
        pub fn burn(env: Env, from: Address, amount: i128) {
            from.require_auth();
            let b: i128 = env
                .storage()
                .persistent()
                .get(&PairKey::Balance(from.clone()))
                .unwrap_or(0);
            env.storage()
                .persistent()
                .set(&PairKey::Balance(from), &(b - amount));
            let s: i128 = env
                .storage()
                .persistent()
                .get(&PairKey::TotalSupply)
                .unwrap_or(0);
            env.storage()
                .persistent()
                .set(&PairKey::TotalSupply, &(s - amount));
        }
        pub fn balance(env: Env, id: Address) -> i128 {
            env.storage()
                .persistent()
                .get(&PairKey::Balance(id))
                .unwrap_or(0)
        }
        pub fn total_supply(env: Env) -> i128 {
            env.storage()
                .persistent()
                .get(&PairKey::TotalSupply)
                .unwrap_or(0)
        }
        pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
            from.require_auth();
            let fb: i128 = env
                .storage()
                .persistent()
                .get(&PairKey::Balance(from.clone()))
                .unwrap_or(0);
            assert!(fb >= amount, "insufficient balance");
            env.storage()
                .persistent()
                .set(&PairKey::Balance(from), &(fb - amount));
            let tb: i128 = env
                .storage()
                .persistent()
                .get(&PairKey::Balance(to.clone()))
                .unwrap_or(0);
            env.storage()
                .persistent()
                .set(&PairKey::Balance(to), &(tb + amount));
        }
        pub fn approve(_env: Env, _f: Address, _s: Address, _a: i128, _e: u32) {}
        pub fn allowance(_env: Env, _f: Address, _s: Address) -> i128 {
            i128::MAX
        }
        pub fn transfer_from(env: Env, _sp: Address, from: Address, to: Address, amount: i128) {
            let fb: i128 = env
                .storage()
                .persistent()
                .get(&PairKey::Balance(from.clone()))
                .unwrap_or(0);
            assert!(fb >= amount);
            env.storage()
                .persistent()
                .set(&PairKey::Balance(from), &(fb - amount));
            let tb: i128 = env
                .storage()
                .persistent()
                .get(&PairKey::Balance(to.clone()))
                .unwrap_or(0);
            env.storage()
                .persistent()
                .set(&PairKey::Balance(to), &(tb + amount));
        }
        pub fn decimals(_env: Env) -> u32 {
            7
        }
        pub fn name(env: Env) -> soroban_sdk::String {
            soroban_sdk::String::from_str(&env, "Pair")
        }
        pub fn symbol(env: Env) -> soroban_sdk::String {
            soroban_sdk::String::from_str(&env, "PAIR")
        }
        pub fn set_reserves(env: Env, reserve_a: i128, reserve_b: i128) {
            env.storage()
                .persistent()
                .set(&PairKey::ReserveA, &reserve_a);
            env.storage()
                .persistent()
                .set(&PairKey::ReserveB, &reserve_b);
        }
        pub fn get_reserves(env: Env) -> (i128, i128) {
            let a: i128 = env
                .storage()
                .persistent()
                .get(&PairKey::ReserveA)
                .unwrap_or(0);
            let b: i128 = env
                .storage()
                .persistent()
                .get(&PairKey::ReserveB)
                .unwrap_or(0);
            (a, b)
        }
    }
}
use mock_pair_token_mod::MockPairToken;

/// Minimal oracle: maps asset address → PRICE_PRECISION-scaled price.
mod mock_oracle2_mod {
    use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};
    #[contracttype]
    pub enum OKey2 {
        Price(Address),
    }
    #[contract]
    pub struct MockOracle2;
    #[contractimpl]
    impl MockOracle2 {
        pub fn set_price(env: Env, asset: Address, price: i128) {
            env.storage().instance().set(&OKey2::Price(asset), &price);
        }
        pub fn get_price(env: Env, asset: Address) -> i128 {
            env.storage()
                .instance()
                .get(&OKey2::Price(asset))
                .unwrap_or(0)
        }
    }
}
use mock_oracle2_mod::MockOracle2;

/// Build a T whose LP token IS a MockPairToken (with get_reserves support).
/// The router is still MockSoroswapRouter but is wired to the pair LP token.
fn setup_with_pair_token() -> (T, Address /* oracle */) {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let token_a = env.register(MockToken2, ());
    let token_b = env.register(MockToken2, ());
    // The LP *is* the pair: use MockPairToken instead of MockToken2.
    let lp_token = env.register(MockPairToken, ());
    let router_id = env.register(MockSoroswapRouter, ());
    MockSoroswapRouterClient::new(&env, &router_id).init(&lp_token);

    let manager = Address::generate(&env);
    let factory = env.register(MockFactory, ());
    let vault = env.register(MockVault, ());
    MockVaultClient::new(&env, &vault).set_manager(&manager);
    MockVaultClient::new(&env, &vault).set_factory(&factory);

    MockToken2Client::new(&env, &token_a).mint(&vault, &10_000_0000000i128);
    MockToken2Client::new(&env, &token_b).mint(&vault, &10_000_0000000i128);

    let sid = env.register(SoroswapLpStrategy, ());
    let strategy = SoroswapLpStrategyClient::new(&env, &sid);
    strategy.initialize(
        &vault,
        &token_a,
        &token_b,
        &lp_token,
        &router_id,
        &soroban_sdk::String::from_str(&env, "Pair LP"),
    );

    let oracle_id = env.register(MockOracle2, ());

    // Set token0 to token_a so get_reserves() ordering matches asset_a/asset_b.
    pair_client(&env, &lp_token).set_token0(&token_a);

    let strategy: SoroswapLpStrategyClient<'static> = unsafe { core::mem::transmute(strategy) };

    let t = T {
        env,
        strategy,
        token_a,
        token_b,
        lp_token,
        router: router_id,
        vault,
        factory,
    };
    (t, oracle_id)
}

fn pair_client<'a>(
    env: &'a soroban_sdk::Env,
    addr: &'a Address,
) -> mock_pair_token_mod::MockPairTokenClient<'a> {
    mock_pair_token_mod::MockPairTokenClient::new(env, addr)
}

fn oracle2_client<'a>(
    env: &'a soroban_sdk::Env,
    addr: &'a Address,
) -> mock_oracle2_mod::MockOracle2Client<'a> {
    mock_oracle2_mod::MockOracle2Client::new(env, addr)
}

const PRICE_PRECISION: i128 = 10_000_000;

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_get_value_no_oracle_panics_pair_token() {
    let (t, _oracle) = setup_with_pair_token();
    t.strategy
        .add_liquidity(&t.vault, &300_0000000i128, &300_0000000i128, &0, &0);
    t.strategy.get_value(&t.vault);
}

/// Core GAP C test: with an oracle and known reserves the value equals
/// `lp_balance / total_supply × (reserveA × priceA + reserveB × priceB)`.
#[test]
fn test_get_value_with_oracle_uses_reserve_decomposition() {
    let (t, oracle_id) = setup_with_pair_token();

    // Deposit — router mints LP equal to min(amount_a, amount_b).
    let deposit_amount = 500_0000000i128;
    t.strategy
        .add_liquidity(&t.vault, &deposit_amount, &deposit_amount, &0, &0);
    let lp = t.strategy.get_lp_balance();
    // lp == deposit_amount (router: lp = min(a, b))

    // Inject reserves directly into the pair token.
    // Pool: 1000 of token_a, 2000 of token_b.
    let reserve_a = 1_000_0000000i128;
    let reserve_b = 2_000_0000000i128;
    pair_client(&t.env, &t.lp_token).set_reserves(&reserve_a, &reserve_b);

    // Set oracle prices: token_a = 1.0, token_b = 0.5 (both PRICE_PRECISION-scaled).
    let price_a = PRICE_PRECISION; // 1.0
    let price_b = PRICE_PRECISION / 2; // 0.5
    oracle2_client(&t.env, &oracle_id).set_price(&t.token_a, &price_a);
    oracle2_client(&t.env, &oracle_id).set_price(&t.token_b, &price_b);

    // Wire oracle into vault's asset handler.
    MockFactoryClient::new(&t.env, &t.factory).set_asset_handler(&oracle_id);

    // Expected value:
    //   total_lp   = lp  (strategy is the only LP holder, router minted to it)
    //   share      = lp / total_lp = 1.0  (strategy holds 100% of pool)
    //   value_a    = reserve_a × price_a / PREC = 1000 × 1.0 = 1000
    //   value_b    = reserve_b × price_b / PREC = 2000 × 0.5 = 1000
    //   pool_value = 1000 + 1000 = 2000
    //   get_value  = pool_value × lp / total_lp = 2000 × 1.0 = 2000
    let total_lp = pair_client(&t.env, &t.lp_token).total_supply();
    let value_a = reserve_a * price_a / PRICE_PRECISION;
    let value_b = reserve_b * price_b / PRICE_PRECISION;
    let pool_value = value_a + value_b;
    let expected = pool_value * lp / total_lp;

    assert_eq!(t.strategy.get_value(&t.vault), expected);
}

/// When the strategy holds a fraction of the pool, value is proportional.
#[test]
fn test_get_value_partial_pool_share() {
    let (t, oracle_id) = setup_with_pair_token();

    // Strategy deposits 200 — router mints 200 LP to strategy.
    t.strategy
        .add_liquidity(&t.vault, &200_0000000i128, &200_0000000i128, &0, &0);
    let lp = t.strategy.get_lp_balance();

    // Mint an extra 800 LP directly to some other holder so total_supply = 1000.
    let other = soroban_sdk::Address::generate(&t.env);
    pair_client(&t.env, &t.lp_token).mint(&other, &800_0000000i128);

    // Pool reserves: 1000 of each token, both priced at 1.0.
    let reserve = 1_000_0000000i128;
    pair_client(&t.env, &t.lp_token).set_reserves(&reserve, &reserve);
    oracle2_client(&t.env, &oracle_id).set_price(&t.token_a, &PRICE_PRECISION);
    oracle2_client(&t.env, &oracle_id).set_price(&t.token_b, &PRICE_PRECISION);
    MockFactoryClient::new(&t.env, &t.factory).set_asset_handler(&oracle_id);

    // total_supply = 1000, strategy holds 200 (20%).
    // pool_value = 1000×1 + 1000×1 = 2000.
    // expected = 2000 × 200 / 1000 = 400.
    let total_lp = pair_client(&t.env, &t.lp_token).total_supply();
    let pool_value = reserve + reserve; // both at 1.0
    let expected = pool_value * lp / total_lp;

    assert_eq!(t.strategy.get_value(&t.vault), expected);
}

/// Edge case: pool has zero reserves → fail closed.
#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_get_value_zero_reserves_panics() {
    let (t, oracle_id) = setup_with_pair_token();
    t.strategy
        .add_liquidity(&t.vault, &100_0000000i128, &100_0000000i128, &0, &0);
    // Reserves remain 0 (not set).
    oracle2_client(&t.env, &oracle_id).set_price(&t.token_a, &PRICE_PRECISION);
    oracle2_client(&t.env, &oracle_id).set_price(&t.token_b, &PRICE_PRECISION);
    MockFactoryClient::new(&t.env, &t.factory).set_asset_handler(&oracle_id);
    t.strategy.get_value(&t.vault);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_get_value_non_positive_oracle_price_panics() {
    let (t, oracle_id) = setup_with_pair_token();
    t.strategy
        .add_liquidity(&t.vault, &100_0000000i128, &100_0000000i128, &0, &0);
    pair_client(&t.env, &t.lp_token).set_reserves(&100_0000000i128, &100_0000000i128);
    oracle2_client(&t.env, &oracle_id).set_price(&t.token_a, &PRICE_PRECISION);
    oracle2_client(&t.env, &oracle_id).set_price(&t.token_b, &0i128);
    MockFactoryClient::new(&t.env, &t.factory).set_asset_handler(&oracle_id);

    t.strategy.get_value(&t.vault);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_checked_mul_div_rejects_zero_denominator() {
    let env = Env::default();
    super::checked_mul_div(&env, 1, 1, 0);
}

#[test]
fn test_oracle_adapter_get_price() {
    let (t, oracle_id) = setup_with_pair_token();
    oracle2_client(&t.env, &oracle_id).set_price(&t.token_a, &456i128);

    assert_eq!(
        OracleAdapter::new(&t.env, &oracle_id).get_price(&t.token_a),
        456i128
    );
}

#[test]
fn test_pair_adapter_token1_passthrough() {
    let (t, _oracle_id) = setup_with_pair_token();

    assert_eq!(PairAdapter::new(&t.env, &t.lp_token).token1(), t.token_a);
}

#[test]
fn test_get_share_balance_alias_matches_lp_balance() {
    let t = setup();
    t.strategy
        .add_liquidity(&t.vault, &100_0000000i128, &100_0000000i128, &0, &0);
    assert_eq!(t.strategy.get_share_balance(), t.strategy.get_lp_balance());
}

#[test]
fn test_get_total_value_wrong_vault_returns_zero() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    assert_eq!(t.strategy.get_total_value(&rogue), 0i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_withdraw_fraction_not_vault_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    let to = Address::generate(&t.env);
    t.strategy.withdraw_fraction(&rogue, &1i128, &2i128, &to);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_withdraw_fraction_invalid_fraction_panics() {
    let t = setup();
    let to = Address::generate(&t.env);
    t.strategy.withdraw_fraction(&t.vault, &2i128, &1i128, &to);
}

#[test]
fn test_withdraw_fraction_no_position_is_noop() {
    let t = setup();
    let to = Address::generate(&t.env);
    t.strategy.withdraw_fraction(&t.vault, &1i128, &2i128, &to);
    assert_eq!(t.strategy.get_lp_balance(), 0i128);
}

#[test]
fn test_withdraw_fraction_rounds_to_zero_is_noop() {
    let t = setup();
    let to = Address::generate(&t.env);
    t.strategy.add_liquidity(&t.vault, &1i128, &1i128, &0, &0);

    t.strategy.withdraw_fraction(&t.vault, &1i128, &2i128, &to);

    assert_eq!(t.strategy.get_lp_balance(), 1i128);
}

#[test]
fn test_asset_in_use_edges() {
    let t = setup();
    let rogue_vault = Address::generate(&t.env);
    let rogue_asset = Address::generate(&t.env);

    assert!(!t.strategy.asset_in_use(&t.vault, &t.token_a));

    t.strategy
        .add_liquidity(&t.vault, &100_0000000i128, &100_0000000i128, &0, &0);

    assert!(!t.strategy.asset_in_use(&rogue_vault, &t.token_a));
    assert!(!t.strategy.asset_in_use(&t.vault, &rogue_asset));
    assert!(t.strategy.asset_in_use(&t.vault, &t.token_a));
    assert!(t.strategy.asset_in_use(&t.vault, &t.token_b));
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_add_liquidity_negative_min_panics() {
    let t = setup();
    t.strategy
        .add_liquidity(&t.vault, &100i128, &100i128, &-1, &0);
}

#[test]
fn test_add_liquidity_returns_dust_to_vault() {
    let t = setup();
    let vault_a_before = MockToken2Client::new(&t.env, &t.token_a).balance(&t.vault);
    let vault_b_before = MockToken2Client::new(&t.env, &t.token_b).balance(&t.vault);

    t.strategy
        .add_liquidity(&t.vault, &200i128, &100i128, &0, &0);

    let vault_a_after = MockToken2Client::new(&t.env, &t.token_a).balance(&t.vault);
    let vault_b_after = MockToken2Client::new(&t.env, &t.token_b).balance(&t.vault);
    assert_eq!(vault_a_before - vault_a_after, 100i128);
    assert_eq!(vault_b_before - vault_b_after, 100i128);
    assert_eq!(t.strategy.get_lp_balance(), 100i128);
}

#[test]
fn test_add_liquidity_returns_token_b_dust_to_vault() {
    let t = setup();
    let vault_a_before = MockToken2Client::new(&t.env, &t.token_a).balance(&t.vault);
    let vault_b_before = MockToken2Client::new(&t.env, &t.token_b).balance(&t.vault);

    t.strategy
        .add_liquidity(&t.vault, &100i128, &200i128, &0, &0);

    let vault_a_after = MockToken2Client::new(&t.env, &t.token_a).balance(&t.vault);
    let vault_b_after = MockToken2Client::new(&t.env, &t.token_b).balance(&t.vault);
    assert_eq!(vault_a_before - vault_a_after, 100i128);
    assert_eq!(vault_b_before - vault_b_after, 100i128);
    assert_eq!(t.strategy.get_lp_balance(), 100i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_remove_liquidity_negative_min_panics() {
    let t = setup();
    t.strategy.remove_liquidity(&t.vault, &1i128, &-1, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_swap_zero_amount_panics() {
    let t = setup();
    t.strategy
        .swap(&t.vault, &t.token_a, &t.token_b, &0i128, &0i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_swap_not_vault_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.strategy
        .swap(&rogue, &t.token_a, &t.token_b, &1i128, &0i128);
}
