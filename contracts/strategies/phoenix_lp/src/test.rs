#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl, contracttype, testutils::Address as _, Address, Env, String,
};

use crate::{PhoenixLpStrategy, PhoenixLpStrategyClient};

// ---------------------------------------------------------------------------
// MockToken — minimal SEP-41 token for testing
// ---------------------------------------------------------------------------

#[contracttype]
enum TKey {
    Balance(Address),
    TotalSupply,
}
#[contract]
pub struct MockToken;
#[contractimpl]
impl MockToken {
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
// MockPhoenixPool
//
// * `query_share_token_address` — returns the registered share token.
// * `get_reserves` — returns mock pool reserves.
// * `provide_liquidity` — mints share tokens (1:1 with min(a,b)) to depositor.
// * `withdraw_liquidity` — burns shares, sends underlying tokens to recipient.
// ---------------------------------------------------------------------------

#[contracttype]
enum PKey {
    ShareToken,
    UnderlyingA,
    UnderlyingB,
    ReserveA,
    ReserveB,
}
#[contract]
pub struct MockPhoenixPool;
#[contractimpl]
impl MockPhoenixPool {
    /// Register the pool with its three token addresses.
    pub fn init(env: Env, share_token: Address, token_a: Address, token_b: Address) {
        env.storage()
            .instance()
            .set(&PKey::ShareToken, &share_token);
        env.storage().instance().set(&PKey::UnderlyingA, &token_a);
        env.storage().instance().set(&PKey::UnderlyingB, &token_b);
        env.storage().instance().set(&PKey::ReserveA, &0i128);
        env.storage().instance().set(&PKey::ReserveB, &0i128);
    }

    /// Called by strategy during `initialize`.
    pub fn query_share_token_address(env: Env) -> Address {
        env.storage().instance().get(&PKey::ShareToken).unwrap()
    }

    pub fn get_reserves(env: Env) -> (i128, i128) {
        let a: i128 = env.storage().instance().get(&PKey::ReserveA).unwrap_or(0);
        let b: i128 = env.storage().instance().get(&PKey::ReserveB).unwrap_or(0);
        (a, b)
    }

    pub fn set_reserves(env: Env, a: i128, b: i128) {
        env.storage().instance().set(&PKey::ReserveA, &a);
        env.storage().instance().set(&PKey::ReserveB, &b);
    }

    /// Phoenix provide_liquidity signature (simplified).
    ///
    /// `auto_stake` is the last bool argument; we ignore it.
    #[allow(clippy::too_many_arguments)]
    pub fn provide_liquidity(
        env: Env,
        depositor: Address,
        desired_a: Option<i128>,
        _min_a: Option<i128>,
        desired_b: Option<i128>,
        _min_b: Option<i128>,
        _custom_slippage_bps: Option<i64>,
        _deadline: Option<u64>,
        _auto_stake: bool,
    ) {
        let a = desired_a.unwrap_or(0);
        let b = desired_b.unwrap_or(0);
        let shares = a.min(b); // simplified 1:1
        let share_token: Address = env.storage().instance().get(&PKey::ShareToken).unwrap();
        MockTokenClient::new(&env, &share_token).mint(&depositor, &shares);
        let ra: i128 = env.storage().instance().get(&PKey::ReserveA).unwrap_or(0);
        let rb: i128 = env.storage().instance().get(&PKey::ReserveB).unwrap_or(0);
        env.storage().instance().set(&PKey::ReserveA, &(ra + a));
        env.storage().instance().set(&PKey::ReserveB, &(rb + b));
    }

    /// Phoenix withdraw_liquidity signature (simplified).
    pub fn withdraw_liquidity(
        env: Env,
        recipient: Address,
        share_amount: i128,
        _min_a: i128,
        _min_b: i128,
        _deadline: Option<u64>,
    ) -> (i128, i128) {
        let half = share_amount / 2;
        let token_a: Address = env.storage().instance().get(&PKey::UnderlyingA).unwrap();
        let token_b: Address = env.storage().instance().get(&PKey::UnderlyingB).unwrap();
        MockTokenClient::new(&env, &token_a).mint(&recipient, &half);
        MockTokenClient::new(&env, &token_b).mint(&recipient, &half);
        let ra: i128 = env.storage().instance().get(&PKey::ReserveA).unwrap_or(0);
        let rb: i128 = env.storage().instance().get(&PKey::ReserveB).unwrap_or(0);
        env.storage().instance().set(&PKey::ReserveA, &(ra - half));
        env.storage().instance().set(&PKey::ReserveB, &(rb - half));
        (half, half)
    }
}

// ---------------------------------------------------------------------------
// MockOracle — simple price feed
// ---------------------------------------------------------------------------

mod mock_oracle {
    use super::*;

    #[contracttype]
    enum OKey {
        Admin,
        Price(Address),
    }
    #[contract]
    pub struct MockOracle;
    #[contractimpl]
    impl MockOracle {
        pub fn init(env: Env, admin: Address) {
            env.storage().instance().set(&OKey::Admin, &admin);
        }
        pub fn set_price(env: Env, asset: Address, price: i128) {
            let admin: Address = env.storage().instance().get(&OKey::Admin).unwrap();
            admin.require_auth();
            env.storage().instance().set(&OKey::Price(asset), &price);
        }
        pub fn get_price(env: Env, asset: Address) -> i128 {
            env.storage()
                .instance()
                .get(&OKey::Price(asset))
                .unwrap_or(0)
        }
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
    strategy: PhoenixLpStrategyClient<'static>,
    token_a: Address,
    token_b: Address,
    share_token: Address,
    pool: Address,
    vault: Address,
    manager: Address,
    user: Address,
}

fn setup() -> T {
    let env = Env::default();
    env.mock_all_auths();

    let token_a = env.register(MockToken, ());
    let token_b = env.register(MockToken, ());
    let share_token = env.register(MockToken, ());
    let pool = env.register(MockPhoenixPool, ());

    MockPhoenixPoolClient::new(&env, &pool).init(&share_token, &token_a, &token_b);

    let manager = Address::generate(&env);
    // Register a mock vault so strategy.initialize() can cross-call
    // vault.get_manager() to derive the authoritative initializer.
    let vault = env.register(MockVault, ());
    MockVaultClient::new(&env, &vault).set_manager(&manager);
    let user = Address::generate(&env);

    // Fund vault with underlying tokens.
    MockTokenClient::new(&env, &token_a).mint(&vault, &10_000_0000000i128);
    MockTokenClient::new(&env, &token_b).mint(&vault, &10_000_0000000i128);

    let sid = env.register(PhoenixLpStrategy, ());
    let strategy = PhoenixLpStrategyClient::new(&env, &sid);
    strategy.initialize(
        &vault,
        &token_a,
        &token_b,
        &pool,
        &manager,
        &String::from_str(&env, "Phoenix USDC/XLM LP"),
    );

    let strategy: PhoenixLpStrategyClient<'static> = unsafe { core::mem::transmute(strategy) };

    T {
        env,
        strategy,
        token_a,
        token_b,
        share_token,
        pool,
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
    assert_eq!(t.strategy.share_token(), t.share_token);
    assert_eq!(t.strategy.get_share_balance(), 0i128);
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
        &t.pool,
        &t.manager,
        &String::from_str(&t.env, "x"),
    );
}

#[test]
fn test_deposit_liquidity_tracks_shares() {
    let t = setup();
    let shares = t
        .strategy
        .deposit_liquidity(&100_0000000i128, &100_0000000i128, &0, &0, &t.vault);
    assert!(shares > 0);
    assert_eq!(t.strategy.get_share_balance(), shares);
}

#[test]
fn test_get_value_with_oracle_uses_reserve_decomposition() {
    let t = setup();
    let oracle = t.env.register(mock_oracle::MockOracle, ());
    mock_oracle::MockOracleClient::new(&t.env, &oracle).init(&t.manager);

    // Provide liquidity to create shares and reserves.
    let shares = t
        .strategy
        .deposit_liquidity(&100_0000000i128, &100_0000000i128, &0, &0, &t.vault);
    assert!(shares > 0);

    // Set oracle prices to 1.0 for both assets (PRICE_PRECISION = 1e7).
    mock_oracle::MockOracleClient::new(&t.env, &oracle).set_price(&t.token_a, &10_000_000i128);
    mock_oracle::MockOracleClient::new(&t.env, &oracle).set_price(&t.token_b, &10_000_000i128);

    // Configure oracle.
    t.strategy.set_oracle(&t.manager, &oracle);

    // With reserves = 1_000000000 each, pool value = 2_000000000.
    // Strategy owns all shares in this mock, so get_value should be ~2_000000000.
    let v = t.strategy.get_value(&t.vault);
    assert!(v >= 1_900_000000i128);
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
fn test_deposit_zero_amount_a_panics() {
    let t = setup();
    t.strategy
        .deposit_liquidity(&0i128, &100i128, &0, &0, &t.vault);
}

#[test]
#[should_panic]
fn test_deposit_zero_amount_b_panics() {
    let t = setup();
    t.strategy
        .deposit_liquidity(&100i128, &0i128, &0, &0, &t.vault);
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
    let shares = t
        .strategy
        .deposit_liquidity(&200_0000000i128, &200_0000000i128, &0, &0, &t.vault);
    let (a, b) = t.strategy.withdraw(&shares, &0, &0, &t.vault, &t.user);
    // Pool mints half/half of share_amount back to user.
    assert!(a > 0 || b > 0);
    assert_eq!(t.strategy.get_share_balance(), 0i128);
    // Tokens should be at user address (minted there by mock pool).
    let bal_a = MockTokenClient::new(&t.env, &t.token_a).balance(&t.user);
    let bal_b = MockTokenClient::new(&t.env, &t.token_b).balance(&t.user);
    assert!(bal_a > 0 || bal_b > 0);
}

#[test]
#[should_panic]
fn test_withdraw_exceeds_balance_panics() {
    let t = setup();
    t.strategy
        .deposit_liquidity(&100_0000000i128, &100_0000000i128, &0, &0, &t.vault);
    t.strategy
        .withdraw(&999_999_999_9999i128, &0, &0, &t.vault, &t.user);
}

#[test]
#[should_panic]
fn test_withdraw_zero_panics() {
    let t = setup();
    t.strategy.withdraw(&0i128, &0, &0, &t.vault, &t.user);
}

#[test]
#[should_panic]
fn test_withdraw_not_vault_panics() {
    let t = setup();
    t.strategy
        .deposit_liquidity(&100_0000000i128, &100_0000000i128, &0, &0, &t.vault);
    let rogue = Address::generate(&t.env);
    t.strategy
        .withdraw(&50_0000000i128, &0, &0, &rogue, &t.user);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_withdraw_paused_panics() {
    let t = setup();
    t.strategy
        .deposit_liquidity(&100_0000000i128, &100_0000000i128, &0, &0, &t.vault);
    t.strategy.pause(&t.manager);
    t.strategy
        .withdraw(&50_0000000i128, &0, &0, &t.vault, &t.user);
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

#[test]
fn test_get_value_no_oracle_returns_zero() {
    let t = setup();
    let shares = t
        .strategy
        .deposit_liquidity(&300_0000000i128, &300_0000000i128, &0, &0, &t.vault);
    // Without an oracle, share units are not base-asset-denominated; returns 0.
    assert_eq!(t.strategy.get_value(&t.vault), 0);
    assert_eq!(t.strategy.get_share_balance(), shares);
}

#[test]
fn test_partial_withdraw() {
    let t = setup();
    let shares = t
        .strategy
        .deposit_liquidity(&400_0000000i128, &400_0000000i128, &0, &0, &t.vault);
    let half = shares / 2;
    t.strategy.withdraw(&half, &0, &0, &t.vault, &t.user);
    assert_eq!(t.strategy.get_share_balance(), shares - half);
}

#[test]
fn test_full_lifecycle() {
    let t = setup();
    // Deposit.
    let shares = t
        .strategy
        .deposit_liquidity(&500_0000000i128, &500_0000000i128, &0, &0, &t.vault);
    assert_eq!(t.strategy.get_share_balance(), shares);

    // Partial withdraw.
    let first = shares / 3;
    t.strategy.withdraw(&first, &0, &0, &t.vault, &t.user);
    assert_eq!(t.strategy.get_share_balance(), shares - first);

    // Full remaining withdraw.
    let remaining = shares - first;
    t.strategy.withdraw(&remaining, &0, &0, &t.vault, &t.user);
    assert_eq!(t.strategy.get_share_balance(), 0i128);
}

#[test]
fn test_multiple_deposits_accumulate() {
    let t = setup();
    let s1 = t
        .strategy
        .deposit_liquidity(&100_0000000i128, &100_0000000i128, &0, &0, &t.vault);
    let s2 = t
        .strategy
        .deposit_liquidity(&200_0000000i128, &200_0000000i128, &0, &0, &t.vault);
    assert_eq!(t.strategy.get_share_balance(), s1 + s2);
}

// ---------------------------------------------------------------------------
// NotInitialized — calling functions before initialize() panics
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_not_initialized_asset_a_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(PhoenixLpStrategy, ());
    let client = PhoenixLpStrategyClient::new(&env, &id);
    client.asset_a();
}

#[test]
#[should_panic]
fn test_not_initialized_asset_b_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(PhoenixLpStrategy, ());
    let client = PhoenixLpStrategyClient::new(&env, &id);
    client.asset_b();
}

#[test]
#[should_panic]
fn test_not_initialized_deposit_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(PhoenixLpStrategy, ());
    let client = PhoenixLpStrategyClient::new(&env, &id);
    let vault = Address::generate(&env);
    client.deposit_liquidity(&100i128, &100i128, &0, &0, &vault);
}

#[test]
#[should_panic]
fn test_not_initialized_withdraw_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(PhoenixLpStrategy, ());
    let client = PhoenixLpStrategyClient::new(&env, &id);
    let vault = Address::generate(&env);
    let user = Address::generate(&env);
    client.withdraw(&100i128, &0, &0, &vault, &user);
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
fn test_get_name() {
    let t = setup();
    assert_eq!(
        t.strategy.get_name(),
        soroban_sdk::String::from_str(&t.env, "Phoenix USDC/XLM LP")
    );
}

#[test]
fn test_share_token_address_returned() {
    let t = setup();
    assert_eq!(t.strategy.share_token(), t.share_token);
}

// ---------------------------------------------------------------------------
// Withdraw: user receives tokens
// ---------------------------------------------------------------------------

#[test]
fn test_withdraw_user_receives_both_tokens() {
    let t = setup();
    let shares = t
        .strategy
        .deposit_liquidity(&100_0000000i128, &100_0000000i128, &0, &0, &t.vault);
    let (a, b) = t.strategy.withdraw(&shares, &0, &0, &t.vault, &t.user);
    // MockPhoenixPool returns half/half of share_amount.
    let half = shares / 2;
    assert_eq!(a, half);
    assert_eq!(b, half);
}

// ---------------------------------------------------------------------------
// Paused state persists across calls
// ---------------------------------------------------------------------------

#[test]
fn test_paused_persists() {
    let t = setup();
    assert!(!t.strategy.is_paused());
    t.strategy.pause(&t.manager);
    assert!(t.strategy.is_paused());
    // Multiple reads return same state.
    assert!(t.strategy.is_paused());
    t.strategy.unpause(&t.manager);
    assert!(!t.strategy.is_paused());
}
