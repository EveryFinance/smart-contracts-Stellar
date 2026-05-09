#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl, contracttype, testutils::Address as _, Address, Env, String,
};

use crate::{SoroswapLpStrategy, SoroswapLpStrategyClient};

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
}
#[contract]
pub struct MockSoroswapRouter;
#[contractimpl]
impl MockSoroswapRouter {
    pub fn init(env: Env, lp_token: Address) {
        env.storage().instance().set(&RouterKey::LpToken, &lp_token);
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
        let lp_minted = amount_a.min(amount_b); // simplified
        let lp: Address = env.storage().instance().get(&RouterKey::LpToken).unwrap();
        MockToken2Client::new(&env, &lp).mint(&to, &lp_minted);
        (amount_a, amount_b, lp_minted)
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
    manager: Address,
    user: Address,
}

fn setup() -> T {
    let env = Env::default();
    env.mock_all_auths();

    let token_a = env.register(MockToken2, ());
    let token_b = env.register(MockToken2, ());
    let lp_token = env.register(MockToken2, ());
    let router_id = env.register(MockSoroswapRouter, ());
    MockSoroswapRouterClient::new(&env, &router_id).init(&lp_token);

    let manager = Address::generate(&env);
    // Register a mock vault so strategy.initialize() can cross-call
    // vault.get_manager() to derive the authoritative initializer.
    let vault = env.register(MockVault, ());
    MockVaultClient::new(&env, &vault).set_manager(&manager);
    let user = Address::generate(&env);

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
        &manager,
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
        manager,
        user,
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
    assert!(!t.strategy.is_paused());
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
        &t.manager,
        &String::from_str(&t.env, "x"),
    );
}

#[test]
fn test_deposit_liquidity_tracks_lp() {
    let t = setup();
    let lp = t
        .strategy
        .deposit_liquidity(&100_0000000i128, &100_0000000i128, &0, &0, &t.vault);
    assert!(lp > 0);
    assert_eq!(t.strategy.get_lp_balance(), lp);
}

#[test]
#[should_panic]
fn test_deposit_not_vault_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.strategy
        .deposit_liquidity(&100i128, &100i128, &0, &0, &rogue);
}

#[test]
#[should_panic]
fn test_deposit_zero_panics() {
    let t = setup();
    t.strategy
        .deposit_liquidity(&0i128, &100i128, &0, &0, &t.vault);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_deposit_paused_panics() {
    let t = setup();
    t.strategy.pause(&t.manager);
    t.strategy
        .deposit_liquidity(&100i128, &100i128, &0, &0, &t.vault);
}

#[test]
fn test_withdraw_sends_to_user() {
    let t = setup();
    let lp = t
        .strategy
        .deposit_liquidity(&200_0000000i128, &200_0000000i128, &0, &0, &t.vault);
    let (a, b) = t.strategy.withdraw(&lp, &0, &0, &t.vault, &t.user);
    assert!(a > 0 || b > 0);
    assert_eq!(t.strategy.get_lp_balance(), 0i128);
}

#[test]
#[should_panic]
fn test_withdraw_exceeds_balance_panics() {
    let t = setup();
    t.strategy
        .deposit_liquidity(&100i128, &100i128, &0, &0, &t.vault);
    t.strategy
        .withdraw(&999_999_999i128, &0, &0, &t.vault, &t.user);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_withdraw_paused_panics() {
    let t = setup();
    t.strategy
        .deposit_liquidity(&100i128, &100i128, &0, &0, &t.vault);
    t.strategy.pause(&t.manager);
    t.strategy.withdraw(&50i128, &0, &0, &t.vault, &t.user);
}

#[test]
fn test_pause_unpause_cycle() {
    let t = setup();
    t.strategy.pause(&t.manager);
    assert!(t.strategy.is_paused());
    t.strategy.unpause(&t.manager);
    assert!(!t.strategy.is_paused());
}

#[test]
fn test_full_lifecycle() {
    let t = setup();
    let lp = t
        .strategy
        .deposit_liquidity(&500_0000000i128, &500_0000000i128, &0, &0, &t.vault);
    assert_eq!(t.strategy.get_lp_balance(), lp);
    let half = lp / 2;
    t.strategy.withdraw(&half, &0, &0, &t.vault, &t.user);
    assert_eq!(t.strategy.get_lp_balance(), lp - half);
    t.strategy.withdraw(&(lp - half), &0, &0, &t.vault, &t.user);
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
fn test_not_initialized_deposit_liquidity_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(SoroswapLpStrategy, ());
    let client = SoroswapLpStrategyClient::new(&env, &id);
    let vault = Address::generate(&env);
    client.deposit_liquidity(&100i128, &100i128, &0, &0, &vault);
}

#[test]
#[should_panic]
fn test_not_initialized_withdraw_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(SoroswapLpStrategy, ());
    let client = SoroswapLpStrategyClient::new(&env, &id);
    let vault = Address::generate(&env);
    let user = Address::generate(&env);
    client.withdraw(&100i128, &0, &0, &vault, &user);
}

// ---------------------------------------------------------------------------
// NotManager — unpause requires manager
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_pause_not_manager_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.strategy.pause(&rogue);
}

#[test]
#[should_panic]
fn test_unpause_not_manager_panics() {
    let t = setup();
    t.strategy.pause(&t.manager);
    let rogue = Address::generate(&t.env);
    t.strategy.unpause(&rogue);
}

// ---------------------------------------------------------------------------
// Withdraw edge cases
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_withdraw_zero_panics() {
    let t = setup();
    t.strategy
        .deposit_liquidity(&100_0000000i128, &100_0000000i128, &0, &0, &t.vault);
    t.strategy.withdraw(&0i128, &0, &0, &t.vault, &t.user);
}

#[test]
#[should_panic]
fn test_withdraw_not_vault_panics() {
    let t = setup();
    let lp = t
        .strategy
        .deposit_liquidity(&100_0000000i128, &100_0000000i128, &0, &0, &t.vault);
    let rogue = Address::generate(&t.env);
    t.strategy.withdraw(&lp, &0, &0, &rogue, &t.user);
}

// ---------------------------------------------------------------------------
// Deposit negative amounts
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_deposit_negative_a_panics() {
    let t = setup();
    t.strategy
        .deposit_liquidity(&-1i128, &100i128, &0, &0, &t.vault);
}

#[test]
#[should_panic]
fn test_deposit_negative_b_panics() {
    let t = setup();
    t.strategy
        .deposit_liquidity(&100i128, &-1i128, &0, &0, &t.vault);
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
fn test_get_value_no_oracle_returns_zero() {
    let t = setup();
    let _lp = t
        .strategy
        .deposit_liquidity(&200_0000000i128, &200_0000000i128, &0, &0, &t.vault);
    // Without an oracle, LP units are not base-asset-denominated; returns 0.
    assert_eq!(t.strategy.get_value(&t.vault), 0);
}

// ---------------------------------------------------------------------------
// Multiple deposits accumulate correctly
// ---------------------------------------------------------------------------

#[test]
fn test_multiple_deposits_lp_accumulate() {
    let t = setup();
    let lp1 = t
        .strategy
        .deposit_liquidity(&100_0000000i128, &100_0000000i128, &0, &0, &t.vault);
    let lp2 = t
        .strategy
        .deposit_liquidity(&200_0000000i128, &200_0000000i128, &0, &0, &t.vault);
    assert_eq!(t.strategy.get_lp_balance(), lp1 + lp2);
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
    }
    #[contract]
    pub struct MockPairToken;
    #[contractimpl]
    impl MockPairToken {
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
    env.mock_all_auths();

    let token_a = env.register(MockToken2, ());
    let token_b = env.register(MockToken2, ());
    // The LP *is* the pair: use MockPairToken instead of MockToken2.
    let lp_token = env.register(MockPairToken, ());
    let router_id = env.register(MockSoroswapRouter, ());
    MockSoroswapRouterClient::new(&env, &router_id).init(&lp_token);

    let manager = Address::generate(&env);
    let vault = env.register(MockVault, ());
    MockVaultClient::new(&env, &vault).set_manager(&manager);
    let user = Address::generate(&env);

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
        &manager,
        &soroban_sdk::String::from_str(&env, "Pair LP"),
    );

    let oracle_id = env.register(MockOracle2, ());

    let strategy: SoroswapLpStrategyClient<'static> = unsafe { core::mem::transmute(strategy) };

    let t = T {
        env,
        strategy,
        token_a,
        token_b,
        lp_token,
        router: router_id,
        vault,
        manager,
        user,
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

/// Without oracle, `get_value` returns 0 (LP units are not base-asset-denominated).
#[test]
fn test_get_value_no_oracle_returns_zero_pair_token() {
    let (t, _oracle) = setup_with_pair_token();
    let _lp = t
        .strategy
        .deposit_liquidity(&300_0000000i128, &300_0000000i128, &0, &0, &t.vault);
    assert_eq!(t.strategy.get_value(&t.vault), 0);
}

/// Core GAP C test: with an oracle and known reserves the value equals
/// `lp_balance / total_supply × (reserveA × priceA + reserveB × priceB)`.
#[test]
fn test_get_value_with_oracle_uses_reserve_decomposition() {
    let (t, oracle_id) = setup_with_pair_token();

    // Deposit — router mints LP equal to min(amount_a, amount_b).
    let deposit_amount = 500_0000000i128;
    let lp = t
        .strategy
        .deposit_liquidity(&deposit_amount, &deposit_amount, &0, &0, &t.vault);
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

    // Wire oracle into strategy.
    t.strategy.set_oracle(&t.manager, &oracle_id);

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
    let lp = t
        .strategy
        .deposit_liquidity(&200_0000000i128, &200_0000000i128, &0, &0, &t.vault);

    // Mint an extra 800 LP directly to some other holder so total_supply = 1000.
    let other = soroban_sdk::Address::generate(&t.env);
    pair_client(&t.env, &t.lp_token).mint(&other, &800_0000000i128);

    // Pool reserves: 1000 of each token, both priced at 1.0.
    let reserve = 1_000_0000000i128;
    pair_client(&t.env, &t.lp_token).set_reserves(&reserve, &reserve);
    oracle2_client(&t.env, &oracle_id).set_price(&t.token_a, &PRICE_PRECISION);
    oracle2_client(&t.env, &oracle_id).set_price(&t.token_b, &PRICE_PRECISION);
    t.strategy.set_oracle(&t.manager, &oracle_id);

    // total_supply = 1000, strategy holds 200 (20%).
    // pool_value = 1000×1 + 1000×1 = 2000.
    // expected = 2000 × 200 / 1000 = 400.
    let total_lp = pair_client(&t.env, &t.lp_token).total_supply();
    let pool_value = reserve + reserve; // both at 1.0
    let expected = pool_value * lp / total_lp;

    assert_eq!(t.strategy.get_value(&t.vault), expected);
}

/// Edge case: pool has zero reserves → returns 0 (cannot decompose empty pool).
#[test]
fn test_get_value_zero_reserves_returns_zero() {
    let (t, oracle_id) = setup_with_pair_token();
    let _lp = t
        .strategy
        .deposit_liquidity(&100_0000000i128, &100_0000000i128, &0, &0, &t.vault);
    // Reserves remain 0 (not set).
    oracle2_client(&t.env, &oracle_id).set_price(&t.token_a, &PRICE_PRECISION);
    oracle2_client(&t.env, &oracle_id).set_price(&t.token_b, &PRICE_PRECISION);
    t.strategy.set_oracle(&t.manager, &oracle_id);
    // reserve_a = 0 AND reserve_b = 0 → cannot decompose, return 0.
    assert_eq!(t.strategy.get_value(&t.vault), 0);
}

#[test]
#[should_panic]
fn test_set_oracle_not_manager_panics() {
    let (t, oracle_id) = setup_with_pair_token();
    let rogue = soroban_sdk::Address::generate(&t.env);
    t.strategy.set_oracle(&rogue, &oracle_id);
}
