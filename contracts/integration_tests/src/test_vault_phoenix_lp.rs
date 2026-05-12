//! Integration tests: Vault ↔ Phoenix LP strategy (execute_op API)

#![cfg(test)]

use phoenix_lp_strategy::{PhoenixLpStrategy, PhoenixLpStrategyClient};
use share_token::ShareTokenContract;
use soroban_sdk::{
    contract, contractimpl, contracttype, testutils::Address as _, Address, Env, IntoVal, Map,
    String, Symbol, Val, Vec,
};
use vault::{Vault, VaultClient, VaultParams};

use crate::common::{
    token_balance, MockPhoenixPool, MockPhoenixPoolClient, MockToken, MockTokenClient,
};

const PRICE_PRECISION: i128 = 10_000_000;

#[contract]
struct MockLpAssetHandler;

#[contractimpl]
impl MockLpAssetHandler {
    pub fn get_price(_env: Env, _asset: Address) -> i128 {
        PRICE_PRECISION
    }

    pub fn get_prices(env: Env, assets: Vec<Address>) -> Map<Address, i128> {
        let mut result = Map::new(&env);
        for asset in assets.iter() {
            result.set(asset, PRICE_PRECISION);
        }
        result
    }
}

#[contracttype]
enum LpFactoryKey {
    AssetHandler,
    Asset(Address),
    Guard(Address),
}

#[contract]
struct MockLpFactory;

#[contractimpl]
impl MockLpFactory {
    pub fn init(env: Env, asset_handler: Address) {
        env.storage()
            .instance()
            .set(&LpFactoryKey::AssetHandler, &asset_handler);
    }

    pub fn authorize_asset(env: Env, asset: Address) {
        env.storage()
            .instance()
            .set(&LpFactoryKey::Asset(asset), &true);
    }

    pub fn is_authorized_asset(env: Env, asset: Address) -> bool {
        env.storage()
            .instance()
            .get(&LpFactoryKey::Asset(asset))
            .unwrap_or(false)
    }

    pub fn authorize_guard(env: Env, guard: Address) {
        env.storage()
            .instance()
            .set(&LpFactoryKey::Guard(guard), &true);
    }

    pub fn is_authorized_guard(env: Env, guard: Address) -> bool {
        env.storage()
            .instance()
            .get(&LpFactoryKey::Guard(guard))
            .unwrap_or(false)
    }

    pub fn get_asset_handler(env: Env) -> Option<Address> {
        env.storage().instance().get(&LpFactoryKey::AssetHandler)
    }
}

// ---------------------------------------------------------------------------
// World fixture
// ---------------------------------------------------------------------------

struct PhoenixWorld {
    env: Env,
    vault: VaultClient<'static>,
    vault_addr: Address,
    strategy: PhoenixLpStrategyClient<'static>,
    strategy_addr: Address,
    pool: MockPhoenixPoolClient<'static>,
    pool_addr: Address,
    asset_a: Address,
    asset_b: Address,
    share_token: Address,
    manager: Address,
    trader: Address,
    user: Address,
}

fn setup_phoenix() -> PhoenixWorld {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let manager = Address::generate(&env);
    let trader = Address::generate(&env);
    let user = Address::generate(&env);

    // Two underlying assets.
    let asset_a = env.register(MockToken, ());
    MockTokenClient::new(&env, &asset_a).initialize(&manager);

    let asset_b = env.register(MockToken, ());
    MockTokenClient::new(&env, &asset_b).initialize(&manager);

    // Phoenix LP share token.
    let share_token = env.register(MockToken, ());
    MockTokenClient::new(&env, &share_token).initialize(&manager);

    let asset_handler = env.register(MockLpAssetHandler, ());
    let factory_id = env.register(MockLpFactory, ());
    MockLpFactoryClient::new(&env, &factory_id).init(&asset_handler);
    MockLpFactoryClient::new(&env, &factory_id).authorize_asset(&asset_a);
    MockLpFactoryClient::new(&env, &factory_id).authorize_asset(&asset_b);

    // Use asset_a as the vault's base asset.
    let vault_id = Address::generate(&env);
    let vault_share_id = env.register(
        ShareTokenContract,
        (
            vault_id.clone(),
            String::from_str(&env, "Phoenix Vault Share"),
            String::from_str(&env, "PVS"),
            7u32,
        ),
    );

    env.register_at(
        &vault_id,
        Vault,
        (VaultParams {
            admin: manager.clone(),
            manager: manager.clone(),
            manager_name: None,
            trader: trader.clone(),
            base_asset: asset_a.clone(),
            share_token: vault_share_id.clone(),
            share_token_admin: vault_id.clone(),
            treasury: manager.clone(),
            entry_fee_bps: 0,
            exit_fee_bps: 0,
            mgmt_fee_bps: 0,
            perf_fee_bps: 0,
            factory: Some(factory_id.clone()),
            is_private: false,
        },),
    );
    let vault = VaultClient::new(&env, &vault_id);

    // MockPhoenixPool.
    let pool_id = env.register(MockPhoenixPool, ());
    MockPhoenixPoolClient::new(&env, &pool_id).phoenix_init(&share_token, &asset_a, &asset_b);

    // PhoenixLpStrategy — auto-queries share_token from pool during initialize.
    let strategy_id = env.register(PhoenixLpStrategy, ());
    PhoenixLpStrategyClient::new(&env, &strategy_id)
        .initialize(&vault_id, &String::from_str(&env, "Phoenix USDC-XLM"));
    MockLpFactoryClient::new(&env, &factory_id).authorize_guard(&strategy_id);

    // Whitelist both pool assets in portfolio. LP positions and idle balances
    // are valued through the factory's AssetHandler, matching production.
    vault.add_portfolio_asset(&manager, &asset_a);
    vault.add_portfolio_asset(&manager, &asset_b);

    vault.add_active_guard(&manager, &strategy_id);
    let ops: Vec<Symbol> = soroban_sdk::vec![
        &env,
        Symbol::new(&env, "add_liquidity"),
        Symbol::new(&env, "remove_liquidity"),
        Symbol::new(&env, "swap"),
    ];
    vault.set_authorized_ops(&manager, &strategy_id, &ops);

    // Use a permissive but non-zero TVL guard for dispatch-focused tests.
    vault.set_max_loss_bps(&manager, &9_999u32);

    // Fund vault with both underlying assets.
    MockTokenClient::new(&env, &asset_a).mint(&vault_id, &50_000_0000000i128);
    MockTokenClient::new(&env, &asset_b).mint(&vault_id, &50_000_0000000i128);
    // Fund user with asset_a.
    MockTokenClient::new(&env, &asset_a).mint(&user, &10_000_0000000i128);

    let vault: VaultClient<'static> = unsafe { core::mem::transmute(vault) };
    let strategy: PhoenixLpStrategyClient<'static> =
        unsafe { core::mem::transmute(PhoenixLpStrategyClient::new(&env, &strategy_id)) };
    let pool: MockPhoenixPoolClient<'static> =
        unsafe { core::mem::transmute(MockPhoenixPoolClient::new(&env, &pool_id)) };

    PhoenixWorld {
        env,
        vault,
        vault_addr: vault_id,
        strategy,
        strategy_addr: strategy_id,
        pool,
        pool_addr: pool_id,
        asset_a,
        asset_b,
        share_token,
        manager,
        trader,
        user,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// add_liquidity via execute_op: vault tokens → strategy → Phoenix pool →
/// share tokens minted to strategy.
///
/// MockPhoenixPool.provide_liquidity pulls tokens from the strategy via
/// transfer_from(pool, strategy, pool) after the strategy approves pool.
/// The strategy first pulls tokens from vault via vault-scoped contract auth.
#[test]
fn test_phoenix_add_liquidity_via_execute_op() {
    let w = setup_phoenix();
    let amount_a = 1_000_0000000i128;
    let amount_b = 1_000_0000000i128;

    // Pool pulls from strategy → strategy approves pool (done inside add_liquidity).

    let args: Vec<Val> = soroban_sdk::vec![
        &w.env,
        w.pool_addr.clone().into_val(&w.env), // pool
        w.asset_a.clone().into_val(&w.env),   // asset_a
        w.asset_b.clone().into_val(&w.env),   // asset_b
        amount_a.into_val(&w.env),
        amount_b.into_val(&w.env),
        0i128.into_val(&w.env), // min_a
        0i128.into_val(&w.env), // min_b
    ];
    w.vault.execute_op(
        &w.trader,
        &w.strategy_addr,
        &Symbol::new(&w.env, "add_liquidity"),
        &args,
    );

    // MockPhoenixPool mints min(amount_a, amount_b) share tokens to strategy.
    let shares_minted = amount_a.min(amount_b);
    assert_eq!(
        token_balance(&w.env, &w.share_token, &w.strategy_addr),
        shares_minted
    );
    // Strategy has 1 active position after add_liquidity.
    assert_eq!(w.strategy.get_share_balance(), 1i128);

    // Vault lost both assets.
    assert_eq!(
        token_balance(&w.env, &w.asset_a, &w.vault_addr),
        50_000_0000000i128 - amount_a
    );
    assert_eq!(
        token_balance(&w.env, &w.asset_b, &w.vault_addr),
        50_000_0000000i128 - amount_b
    );
}

/// remove_liquidity via execute_op: share tokens burned at pool → underlying
/// tokens minted directly to vault.
#[test]
fn test_phoenix_remove_liquidity_via_execute_op() {
    let w = setup_phoenix();
    let amount_a = 2_000_0000000i128;
    let amount_b = 2_000_0000000i128;

    // --- add liquidity first ---
    let add_args: Vec<Val> = soroban_sdk::vec![
        &w.env,
        w.pool_addr.clone().into_val(&w.env),
        w.asset_a.clone().into_val(&w.env),
        w.asset_b.clone().into_val(&w.env),
        amount_a.into_val(&w.env),
        amount_b.into_val(&w.env),
        0i128.into_val(&w.env),
        0i128.into_val(&w.env),
    ];
    w.vault.execute_op(
        &w.trader,
        &w.strategy_addr,
        &Symbol::new(&w.env, "add_liquidity"),
        &add_args,
    );

    let shares_minted = amount_a.min(amount_b);
    let vault_a_before = token_balance(&w.env, &w.asset_a, &w.vault_addr);
    let vault_b_before = token_balance(&w.env, &w.asset_b, &w.vault_addr);

    // --- remove all liquidity ---
    // Strategy calls approve(strategy, pool, share_amount) internally before
    // withdraw_liquidity; pool calls transfer_from(pool, strategy, pool) to burn shares.
    // No extra allowance setup needed in tests.
    let remove_args: Vec<Val> = soroban_sdk::vec![
        &w.env,
        w.pool_addr.clone().into_val(&w.env),
        w.asset_a.clone().into_val(&w.env),
        w.asset_b.clone().into_val(&w.env),
        shares_minted.into_val(&w.env),
        0i128.into_val(&w.env), // min_a
        0i128.into_val(&w.env), // min_b
    ];
    w.vault.execute_op(
        &w.trader,
        &w.strategy_addr,
        &Symbol::new(&w.env, "remove_liquidity"),
        &remove_args,
    );

    // MockPhoenixPool distributes proportional assets to vault.
    // With total_shares = shares_minted and equal reserves, each gets ~half.
    let (expected_a, expected_b) = {
        // Mirror MockPhoenixPool logic: reserve_a * share / total_shares.
        let total = shares_minted;
        let ra = amount_a;
        let rb = amount_b;
        (ra * shares_minted / total, rb * shares_minted / total)
    };
    assert_eq!(
        token_balance(&w.env, &w.asset_a, &w.vault_addr),
        vault_a_before + expected_a
    );
    assert_eq!(
        token_balance(&w.env, &w.asset_b, &w.vault_addr),
        vault_b_before + expected_b
    );

    // Strategy holds no share tokens.
    assert_eq!(token_balance(&w.env, &w.share_token, &w.strategy_addr), 0);
    assert_eq!(w.strategy.get_share_balance(), 0);
}

#[test]
fn test_phoenix_swap_via_execute_op() {
    let w = setup_phoenix();
    let amount_in = 500_0000000i128;

    // Register the position first — swap looks up asset_a/asset_b from the stored position.
    let add_args: Vec<Val> = soroban_sdk::vec![
        &w.env,
        w.pool_addr.clone().into_val(&w.env),
        w.asset_a.clone().into_val(&w.env),
        w.asset_b.clone().into_val(&w.env),
        100_0000000i128.into_val(&w.env),
        100_0000000i128.into_val(&w.env),
        0i128.into_val(&w.env),
        0i128.into_val(&w.env),
    ];
    w.vault.execute_op(
        &w.trader,
        &w.strategy_addr,
        &Symbol::new(&w.env, "add_liquidity"),
        &add_args,
    );

    let vault_a_before = token_balance(&w.env, &w.asset_a, &w.vault_addr);
    let vault_b_before = token_balance(&w.env, &w.asset_b, &w.vault_addr);

    // phoenix swap: [pool, asset_in, asset_out, amount_in, min_out]
    let args: Vec<Val> = soroban_sdk::vec![
        &w.env,
        w.pool_addr.clone().into_val(&w.env),
        w.asset_a.clone().into_val(&w.env), // selling asset_a
        w.asset_b.clone().into_val(&w.env), // receiving asset_b
        amount_in.into_val(&w.env),
        0i128.into_val(&w.env),
    ];
    w.vault.execute_op(
        &w.trader,
        &w.strategy_addr,
        &Symbol::new(&w.env, "swap"),
        &args,
    );

    assert_eq!(
        token_balance(&w.env, &w.asset_a, &w.vault_addr),
        vault_a_before - amount_in
    );
    assert_eq!(
        token_balance(&w.env, &w.asset_b, &w.vault_addr),
        vault_b_before + amount_in
    );
}

/// asset_in_use returns true for both underlying assets after add_liquidity.
#[test]
fn test_phoenix_asset_in_use_after_add_liquidity() {
    let w = setup_phoenix();
    let amount = 1_000_0000000i128;

    assert!(!w.strategy.asset_in_use(&w.vault_addr, &w.asset_a));
    assert!(!w.strategy.asset_in_use(&w.vault_addr, &w.asset_b));

    let args: Vec<Val> = soroban_sdk::vec![
        &w.env,
        w.pool_addr.clone().into_val(&w.env),
        w.asset_a.clone().into_val(&w.env),
        w.asset_b.clone().into_val(&w.env),
        amount.into_val(&w.env),
        amount.into_val(&w.env),
        0i128.into_val(&w.env),
        0i128.into_val(&w.env),
    ];
    w.vault.execute_op(
        &w.trader,
        &w.strategy_addr,
        &Symbol::new(&w.env, "add_liquidity"),
        &args,
    );

    assert!(w.strategy.asset_in_use(&w.vault_addr, &w.asset_a));
    assert!(w.strategy.asset_in_use(&w.vault_addr, &w.asset_b));
}

/// get_total_value returns 0 before a position and positive value after LP entry.
#[test]
fn test_phoenix_get_total_value_with_asset_handler() {
    let w = setup_phoenix();
    assert_eq!(w.strategy.get_total_value(&w.vault_addr), 0);

    let amount = 1_000_0000000i128;
    let args: Vec<Val> = soroban_sdk::vec![
        &w.env,
        w.pool_addr.clone().into_val(&w.env),
        w.asset_a.clone().into_val(&w.env),
        w.asset_b.clone().into_val(&w.env),
        amount.into_val(&w.env),
        amount.into_val(&w.env),
        0i128.into_val(&w.env),
        0i128.into_val(&w.env),
    ];
    w.vault.execute_op(
        &w.trader,
        &w.strategy_addr,
        &Symbol::new(&w.env, "add_liquidity"),
        &args,
    );

    assert!(w.strategy.get_total_value(&w.vault_addr) > 0);
}
