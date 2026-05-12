#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl, contracttype, testutils::Address as _, Address, Env, Map, String, Vec,
};

use crate::{interfaces::OracleAdapter, PhoenixLpStrategy, PhoenixLpStrategyClient};

// ---------------------------------------------------------------------------
// MockToken
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
// ---------------------------------------------------------------------------

#[contracttype]
enum PKey {
    ShareToken,
    UnderlyingA,
    UnderlyingB,
    ReserveA,
    ReserveB,
    NoMint,
    BurnShares,
}
#[contract]
pub struct MockPhoenixPool;
#[contractimpl]
impl MockPhoenixPool {
    pub fn init(env: Env, share_token: Address, token_a: Address, token_b: Address) {
        env.storage()
            .instance()
            .set(&PKey::ShareToken, &share_token);
        env.storage().instance().set(&PKey::UnderlyingA, &token_a);
        env.storage().instance().set(&PKey::UnderlyingB, &token_b);
        env.storage().instance().set(&PKey::ReserveA, &0i128);
        env.storage().instance().set(&PKey::ReserveB, &0i128);
    }

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

    pub fn set_no_mint(env: Env, enabled: bool) {
        env.storage().instance().set(&PKey::NoMint, &enabled);
    }

    pub fn set_burn_shares(env: Env, enabled: bool) {
        env.storage().instance().set(&PKey::BurnShares, &enabled);
    }

    pub fn swap(
        env: Env,
        _offer_from: Address,
        ask_to: Address,
        sell_a: bool,
        amount: i128,
        _min_out: i128,
        _slippage: Option<i64>,
        _deadline: Option<u64>,
    ) {
        let token: Address = if sell_a {
            env.storage().instance().get(&PKey::UnderlyingB).unwrap()
        } else {
            env.storage().instance().get(&PKey::UnderlyingA).unwrap()
        };
        MockTokenClient::new(&env, &token).mint(&ask_to, &amount);
    }

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
        let shares = a.min(b);
        let share_token: Address = env.storage().instance().get(&PKey::ShareToken).unwrap();
        let share_client = MockTokenClient::new(&env, &share_token);
        let burn_shares: bool = env
            .storage()
            .instance()
            .get(&PKey::BurnShares)
            .unwrap_or(false);
        let no_mint: bool = env.storage().instance().get(&PKey::NoMint).unwrap_or(false);
        if burn_shares {
            let balance = share_client.balance(&depositor);
            if balance > 0 {
                share_client.burn(&depositor, &balance);
            }
        } else if !no_mint {
            share_client.mint(&depositor, &shares);
        }
        let ra: i128 = env.storage().instance().get(&PKey::ReserveA).unwrap_or(0);
        let rb: i128 = env.storage().instance().get(&PKey::ReserveB).unwrap_or(0);
        env.storage().instance().set(&PKey::ReserveA, &(ra + a));
        env.storage().instance().set(&PKey::ReserveB, &(rb + b));
    }

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
        let new_ra = if ra >= half { ra - half } else { 0 };
        let new_rb = if rb >= half { rb - half } else { 0 };
        env.storage().instance().set(&PKey::ReserveA, &new_ra);
        env.storage().instance().set(&PKey::ReserveB, &new_rb);
        (half, half)
    }
}

// ---------------------------------------------------------------------------
// MockVault
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

// ---------------------------------------------------------------------------
// MockFactory
// ---------------------------------------------------------------------------

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
// MockOracle — supports both get_price and get_prices (batch)
// ---------------------------------------------------------------------------

mod mock_oracle {
    use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Map, Vec};

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
        pub fn get_prices(env: Env, assets: Vec<Address>) -> Map<Address, i128> {
            let mut result = Map::new(&env);
            for asset in assets.iter() {
                let price: i128 = env
                    .storage()
                    .instance()
                    .get(&OKey::Price(asset.clone()))
                    .unwrap_or(0);
                result.set(asset, price);
            }
            result
        }
    }
}

// ---------------------------------------------------------------------------
// Test harness
// ---------------------------------------------------------------------------

struct T {
    env: Env,
    strategy: PhoenixLpStrategyClient<'static>,
    token_a: Address,
    token_b: Address,
    share_token: Address,
    pool: Address,
    vault: Address,
    factory: Address,
    manager: Address,
    user: Address,
}

fn setup() -> T {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let token_a = env.register(MockToken, ());
    let token_b = env.register(MockToken, ());
    let share_token = env.register(MockToken, ());
    let pool = env.register(MockPhoenixPool, ());

    MockPhoenixPoolClient::new(&env, &pool).init(&share_token, &token_a, &token_b);

    let manager = Address::generate(&env);
    let factory = env.register(MockFactory, ());
    let vault = env.register(MockVault, ());
    MockVaultClient::new(&env, &vault).set_manager(&manager);
    MockVaultClient::new(&env, &vault).set_factory(&factory);
    let user = Address::generate(&env);

    MockTokenClient::new(&env, &token_a).mint(&vault, &10_000_0000000i128);
    MockTokenClient::new(&env, &token_b).mint(&vault, &10_000_0000000i128);

    let sid = env.register(PhoenixLpStrategy, ());
    let strategy = PhoenixLpStrategyClient::new(&env, &sid);
    // Multi-position initialize: vault + name only, no pool/asset args.
    strategy.initialize(&vault, &String::from_str(&env, "Phoenix USDC/XLM LP"));

    let strategy: PhoenixLpStrategyClient<'static> = unsafe { core::mem::transmute(strategy) };

    T {
        env,
        strategy,
        token_a,
        token_b,
        share_token,
        pool,
        vault,
        factory,
        manager,
        user,
    }
}

// ---------------------------------------------------------------------------
// Core lifecycle tests
// ---------------------------------------------------------------------------

#[test]
fn test_initialize() {
    let t = setup();
    assert_eq!(t.strategy.get_active_positions().len(), 0);
    assert_eq!(t.strategy.get_share_balance(), 0i128);
    assert_eq!(
        t.strategy.get_name(),
        String::from_str(&t.env, "Phoenix USDC/XLM LP")
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_double_initialize_panics() {
    let t = setup();
    t.strategy
        .initialize(&t.vault, &String::from_str(&t.env, "x"));
}

#[test]
fn test_deposit_liquidity_tracks_shares() {
    let t = setup();
    let shares = t.strategy.deposit_liquidity(
        &100_0000000i128,
        &100_0000000i128,
        &0,
        &0,
        &t.pool,
        &t.token_a,
        &t.token_b,
        &t.vault,
    );
    assert!(shares > 0);
    assert_eq!(t.strategy.get_share_balance(), 1i128); // 1 active position
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_deposit_liquidity_rejects_pool_that_mints_no_shares() {
    let t = setup();
    MockPhoenixPoolClient::new(&t.env, &t.pool).set_no_mint(&true);
    t.strategy.deposit_liquidity(
        &100_0000000i128,
        &100_0000000i128,
        &0,
        &0,
        &t.pool,
        &t.token_a,
        &t.token_b,
        &t.vault,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_deposit_liquidity_rejects_pool_that_burns_existing_shares() {
    let t = setup();
    t.strategy.deposit_liquidity(
        &100_0000000i128,
        &100_0000000i128,
        &0,
        &0,
        &t.pool,
        &t.token_a,
        &t.token_b,
        &t.vault,
    );
    MockPhoenixPoolClient::new(&t.env, &t.pool).set_burn_shares(&true);
    t.strategy.deposit_liquidity(
        &100_0000000i128,
        &100_0000000i128,
        &0,
        &0,
        &t.pool,
        &t.token_a,
        &t.token_b,
        &t.vault,
    );
}

// ---------------------------------------------------------------------------
// Oracle / valuation tests
// ---------------------------------------------------------------------------

#[test]
fn test_get_value_with_oracle_uses_reserve_decomposition() {
    let t = setup();
    let oracle = t.env.register(mock_oracle::MockOracle, ());
    mock_oracle::MockOracleClient::new(&t.env, &oracle).init(&t.manager);

    t.strategy.deposit_liquidity(
        &100_0000000i128,
        &100_0000000i128,
        &0,
        &0,
        &t.pool,
        &t.token_a,
        &t.token_b,
        &t.vault,
    );

    mock_oracle::MockOracleClient::new(&t.env, &oracle).set_price(&t.token_a, &10_000_000i128);
    mock_oracle::MockOracleClient::new(&t.env, &oracle).set_price(&t.token_b, &10_000_000i128);
    MockFactoryClient::new(&t.env, &t.factory).set_asset_handler(&oracle);

    let v = t.strategy.get_value(&t.vault);
    assert!(v >= 1_900_000000i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_get_value_zero_reserves_panics() {
    let t = setup();
    let oracle = t.env.register(mock_oracle::MockOracle, ());
    mock_oracle::MockOracleClient::new(&t.env, &oracle).init(&t.manager);

    t.strategy.deposit_liquidity(
        &100_0000000i128,
        &100_0000000i128,
        &0,
        &0,
        &t.pool,
        &t.token_a,
        &t.token_b,
        &t.vault,
    );
    MockPhoenixPoolClient::new(&t.env, &t.pool).set_reserves(&0i128, &0i128);
    mock_oracle::MockOracleClient::new(&t.env, &oracle).set_price(&t.token_a, &10_000_000i128);
    mock_oracle::MockOracleClient::new(&t.env, &oracle).set_price(&t.token_b, &10_000_000i128);
    MockFactoryClient::new(&t.env, &t.factory).set_asset_handler(&oracle);

    t.strategy.get_value(&t.vault);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_get_value_non_positive_oracle_price_panics() {
    let t = setup();
    let oracle = t.env.register(mock_oracle::MockOracle, ());
    mock_oracle::MockOracleClient::new(&t.env, &oracle).init(&t.manager);

    t.strategy.deposit_liquidity(
        &100_0000000i128,
        &100_0000000i128,
        &0,
        &0,
        &t.pool,
        &t.token_a,
        &t.token_b,
        &t.vault,
    );
    mock_oracle::MockOracleClient::new(&t.env, &oracle).set_price(&t.token_a, &0i128);
    mock_oracle::MockOracleClient::new(&t.env, &oracle).set_price(&t.token_b, &10_000_000i128);
    MockFactoryClient::new(&t.env, &t.factory).set_asset_handler(&oracle);

    t.strategy.get_value(&t.vault);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_get_value_no_oracle_panics() {
    let t = setup();
    t.strategy.deposit_liquidity(
        &300_0000000i128,
        &300_0000000i128,
        &0,
        &0,
        &t.pool,
        &t.token_a,
        &t.token_b,
        &t.vault,
    );
    t.strategy.get_value(&t.vault);
}

// ---------------------------------------------------------------------------
// Deposit validation
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_deposit_not_vault_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.strategy.deposit_liquidity(
        &100i128, &100i128, &0, &0, &t.pool, &t.token_a, &t.token_b, &rogue,
    );
}

#[test]
#[should_panic]
fn test_deposit_zero_amount_a_panics() {
    let t = setup();
    t.strategy.deposit_liquidity(
        &0i128, &100i128, &0, &0, &t.pool, &t.token_a, &t.token_b, &t.vault,
    );
}

#[test]
#[should_panic]
fn test_deposit_zero_amount_b_panics() {
    let t = setup();
    t.strategy.deposit_liquidity(
        &100i128, &0i128, &0, &0, &t.pool, &t.token_a, &t.token_b, &t.vault,
    );
}

#[test]
#[should_panic]
fn test_deposit_negative_a_panics() {
    let t = setup();
    t.strategy.deposit_liquidity(
        &-1i128, &100i128, &0, &0, &t.pool, &t.token_a, &t.token_b, &t.vault,
    );
}

#[test]
#[should_panic]
fn test_deposit_negative_b_panics() {
    let t = setup();
    t.strategy.deposit_liquidity(
        &100i128, &-1i128, &0, &0, &t.pool, &t.token_a, &t.token_b, &t.vault,
    );
}

// ---------------------------------------------------------------------------
// Withdraw tests
// ---------------------------------------------------------------------------

#[test]
fn test_withdraw_sends_to_user() {
    let t = setup();
    let shares = t.strategy.deposit_liquidity(
        &200_0000000i128,
        &200_0000000i128,
        &0,
        &0,
        &t.pool,
        &t.token_a,
        &t.token_b,
        &t.vault,
    );
    let (a, b) = t
        .strategy
        .withdraw(&shares, &0, &0, &t.pool, &t.vault, &t.user);
    assert!(a > 0 || b > 0);
    assert_eq!(t.strategy.get_share_balance(), 0i128);
    let bal_a = MockTokenClient::new(&t.env, &t.token_a).balance(&t.user);
    let bal_b = MockTokenClient::new(&t.env, &t.token_b).balance(&t.user);
    assert!(bal_a > 0 || bal_b > 0);
}

#[test]
#[should_panic]
fn test_withdraw_exceeds_balance_panics() {
    let t = setup();
    t.strategy.deposit_liquidity(
        &100_0000000i128,
        &100_0000000i128,
        &0,
        &0,
        &t.pool,
        &t.token_a,
        &t.token_b,
        &t.vault,
    );
    t.strategy
        .withdraw(&999_999_999_9999i128, &0, &0, &t.pool, &t.vault, &t.user);
}

#[test]
#[should_panic]
fn test_withdraw_zero_panics() {
    let t = setup();
    t.strategy
        .withdraw(&0i128, &0, &0, &t.pool, &t.vault, &t.user);
}

#[test]
#[should_panic]
fn test_withdraw_not_vault_panics() {
    let t = setup();
    t.strategy.deposit_liquidity(
        &100_0000000i128,
        &100_0000000i128,
        &0,
        &0,
        &t.pool,
        &t.token_a,
        &t.token_b,
        &t.vault,
    );
    let rogue = Address::generate(&t.env);
    t.strategy
        .withdraw(&50_0000000i128, &0, &0, &t.pool, &rogue, &t.user);
}

#[test]
fn test_withdraw_user_receives_both_tokens() {
    let t = setup();
    let shares = t.strategy.deposit_liquidity(
        &100_0000000i128,
        &100_0000000i128,
        &0,
        &0,
        &t.pool,
        &t.token_a,
        &t.token_b,
        &t.vault,
    );
    let (a, b) = t
        .strategy
        .withdraw(&shares, &0, &0, &t.pool, &t.vault, &t.user);
    let half = shares / 2;
    assert_eq!(a, half);
    assert_eq!(b, half);
}

// ---------------------------------------------------------------------------
// Full lifecycle
// ---------------------------------------------------------------------------

#[test]
fn test_partial_withdraw() {
    let t = setup();
    let shares = t.strategy.deposit_liquidity(
        &400_0000000i128,
        &400_0000000i128,
        &0,
        &0,
        &t.pool,
        &t.token_a,
        &t.token_b,
        &t.vault,
    );
    let half = shares / 2;
    t.strategy
        .withdraw(&half, &0, &0, &t.pool, &t.vault, &t.user);
    assert_eq!(t.strategy.get_share_balance(), 1i128); // still one active position
}

#[test]
fn test_full_lifecycle() {
    let t = setup();
    let shares = t.strategy.deposit_liquidity(
        &500_0000000i128,
        &500_0000000i128,
        &0,
        &0,
        &t.pool,
        &t.token_a,
        &t.token_b,
        &t.vault,
    );
    assert_eq!(t.strategy.get_share_balance(), 1i128);

    let first = shares / 3;
    t.strategy
        .withdraw(&first, &0, &0, &t.pool, &t.vault, &t.user);
    assert_eq!(t.strategy.get_share_balance(), 1i128);

    let remaining = shares - first;
    t.strategy
        .withdraw(&remaining, &0, &0, &t.pool, &t.vault, &t.user);
    assert_eq!(t.strategy.get_share_balance(), 0i128);
}

#[test]
fn test_multiple_deposits_accumulate() {
    let t = setup();
    t.strategy.deposit_liquidity(
        &100_0000000i128,
        &100_0000000i128,
        &0,
        &0,
        &t.pool,
        &t.token_a,
        &t.token_b,
        &t.vault,
    );
    t.strategy.deposit_liquidity(
        &200_0000000i128,
        &200_0000000i128,
        &0,
        &0,
        &t.pool,
        &t.token_a,
        &t.token_b,
        &t.vault,
    );
    assert_eq!(t.strategy.get_share_balance(), 1i128); // still one pool
}

// ---------------------------------------------------------------------------
// NotInitialized panics
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_not_initialized_get_name_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(PhoenixLpStrategy, ());
    let client = PhoenixLpStrategyClient::new(&env, &id);
    client.get_name();
}

#[test]
#[should_panic]
fn test_not_initialized_deposit_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(PhoenixLpStrategy, ());
    let client = PhoenixLpStrategyClient::new(&env, &id);
    let vault = Address::generate(&env);
    let pool = Address::generate(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    client.deposit_liquidity(&100i128, &100i128, &0, &0, &pool, &a, &b, &vault);
}

#[test]
#[should_panic]
fn test_not_initialized_withdraw_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(PhoenixLpStrategy, ());
    let client = PhoenixLpStrategyClient::new(&env, &id);
    let vault = Address::generate(&env);
    let pool = Address::generate(&env);
    let user = Address::generate(&env);
    client.withdraw(&100i128, &0, &0, &pool, &vault, &user);
}

// ---------------------------------------------------------------------------
// View functions
// ---------------------------------------------------------------------------

#[test]
fn test_get_name() {
    let t = setup();
    assert_eq!(
        t.strategy.get_name(),
        String::from_str(&t.env, "Phoenix USDC/XLM LP")
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_checked_mul_div_rejects_zero_denominator() {
    let env = Env::default();
    super::checked_mul_div(&env, 1, 1, 0);
}

#[test]
fn test_oracle_adapter_get_price() {
    let t = setup();
    let oracle = t.env.register(mock_oracle::MockOracle, ());
    mock_oracle::MockOracleClient::new(&t.env, &oracle).init(&t.manager);
    mock_oracle::MockOracleClient::new(&t.env, &oracle).set_price(&t.token_a, &123i128);
    assert_eq!(
        OracleAdapter::new(&t.env, &oracle).get_price(&t.token_a),
        123i128
    );
}

#[test]
fn test_get_total_value_wrong_vault_returns_zero() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    assert_eq!(t.strategy.get_total_value(&rogue), 0i128);
}

// ---------------------------------------------------------------------------
// withdraw_fraction
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_withdraw_fraction_not_vault_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.strategy
        .withdraw_fraction(&rogue, &1i128, &2i128, &t.user);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_withdraw_fraction_invalid_fraction_panics() {
    let t = setup();
    t.strategy
        .withdraw_fraction(&t.vault, &2i128, &1i128, &t.user);
}

#[test]
fn test_withdraw_fraction_no_tracked_shares_is_noop() {
    let t = setup();
    t.strategy
        .withdraw_fraction(&t.vault, &1i128, &2i128, &t.user);
    assert_eq!(t.strategy.get_share_balance(), 0i128);
}

#[test]
fn test_withdraw_fraction_rounds_to_zero_is_noop() {
    let t = setup();
    t.strategy.deposit_liquidity(
        &1i128, &1i128, &0, &0, &t.pool, &t.token_a, &t.token_b, &t.vault,
    );
    t.strategy
        .withdraw_fraction(&t.vault, &1i128, &2i128, &t.user);
    assert_eq!(t.strategy.get_share_balance(), 1i128);
}

// ---------------------------------------------------------------------------
// asset_in_use
// ---------------------------------------------------------------------------

#[test]
fn test_asset_in_use_edges() {
    let t = setup();
    let rogue_vault = Address::generate(&t.env);
    let rogue_asset = Address::generate(&t.env);

    assert!(!t.strategy.asset_in_use(&t.vault, &t.token_a));

    t.strategy.deposit_liquidity(
        &100_0000000i128,
        &100_0000000i128,
        &0,
        &0,
        &t.pool,
        &t.token_a,
        &t.token_b,
        &t.vault,
    );

    assert!(!t.strategy.asset_in_use(&rogue_vault, &t.token_a));
    assert!(!t.strategy.asset_in_use(&t.vault, &rogue_asset));
    assert!(t.strategy.asset_in_use(&t.vault, &t.token_a));
    assert!(t.strategy.asset_in_use(&t.vault, &t.token_b));
}

// ---------------------------------------------------------------------------
// add_liquidity (execute_op path)
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_add_liquidity_wrapper_zero_panics() {
    let t = setup();
    t.strategy.add_liquidity(
        &t.vault, &t.pool, &t.token_a, &t.token_b, &0i128, &100i128, &0, &0,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_add_liquidity_wrapper_not_vault_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.strategy.add_liquidity(
        &rogue, &t.pool, &t.token_a, &t.token_b, &100i128, &100i128, &0, &0,
    );
}

#[test]
fn test_add_liquidity_wrapper_success_returns_residuals() {
    let t = setup();
    let vault_a_before = MockTokenClient::new(&t.env, &t.token_a).balance(&t.vault);
    let vault_b_before = MockTokenClient::new(&t.env, &t.token_b).balance(&t.vault);

    t.strategy.add_liquidity(
        &t.vault, &t.pool, &t.token_a, &t.token_b, &200i128, &100i128, &0, &0,
    );

    let vault_a_after = MockTokenClient::new(&t.env, &t.token_a).balance(&t.vault);
    let vault_b_after = MockTokenClient::new(&t.env, &t.token_b).balance(&t.vault);
    // All input tokens should be accounted for (used or returned as residual).
    assert_eq!(vault_a_before, vault_a_after);
    assert_eq!(vault_b_before, vault_b_after);
    assert_eq!(t.strategy.get_share_balance(), 1i128); // 1 active position
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_add_liquidity_wrapper_rejects_pool_that_mints_no_shares() {
    let t = setup();
    MockPhoenixPoolClient::new(&t.env, &t.pool).set_no_mint(&true);
    t.strategy.add_liquidity(
        &t.vault,
        &t.pool,
        &t.token_a,
        &t.token_b,
        &100_0000000i128,
        &100_0000000i128,
        &0,
        &0,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_add_liquidity_wrapper_rejects_pool_that_burns_existing_shares() {
    let t = setup();
    t.strategy.add_liquidity(
        &t.vault,
        &t.pool,
        &t.token_a,
        &t.token_b,
        &100_0000000i128,
        &100_0000000i128,
        &0,
        &0,
    );
    MockPhoenixPoolClient::new(&t.env, &t.pool).set_burn_shares(&true);
    t.strategy.add_liquidity(
        &t.vault,
        &t.pool,
        &t.token_a,
        &t.token_b,
        &100_0000000i128,
        &100_0000000i128,
        &0,
        &0,
    );
}

// ---------------------------------------------------------------------------
// remove_liquidity (execute_op path)
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_remove_liquidity_wrapper_zero_panics() {
    let t = setup();
    t.strategy
        .remove_liquidity(&t.vault, &t.pool, &t.token_a, &t.token_b, &0i128, &0, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_remove_liquidity_wrapper_not_vault_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.strategy
        .remove_liquidity(&rogue, &t.pool, &t.token_a, &t.token_b, &1i128, &0, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_remove_liquidity_wrapper_insufficient_shares_panics() {
    let t = setup();
    t.strategy
        .remove_liquidity(&t.vault, &t.pool, &t.token_a, &t.token_b, &1i128, &0, &0);
}

// ---------------------------------------------------------------------------
// swap (execute_op path)
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_swap_zero_amount_panics() {
    let t = setup();
    // Need a position registered first so swap can look up asset_a/asset_b.
    t.strategy.add_liquidity(
        &t.vault, &t.pool, &t.token_a, &t.token_b, &10i128, &10i128, &0, &0,
    );
    t.strategy
        .swap(&t.vault, &t.pool, &t.token_a, &t.token_b, &0i128, &0i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_swap_negative_min_out_panics() {
    let t = setup();
    t.strategy.add_liquidity(
        &t.vault, &t.pool, &t.token_a, &t.token_b, &10i128, &10i128, &0, &0,
    );
    t.strategy
        .swap(&t.vault, &t.pool, &t.token_a, &t.token_b, &1i128, &-1i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_swap_not_vault_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.strategy
        .swap(&rogue, &t.pool, &t.token_a, &t.token_b, &1i128, &0i128);
}
