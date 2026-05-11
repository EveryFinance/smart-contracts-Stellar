//! Integration tests: Vault ↔ Phoenix LP strategy (execute_op API)

#![cfg(test)]

use phoenix_lp_strategy::{PhoenixLpStrategy, PhoenixLpStrategyClient};
use share_token::ShareTokenContract;
use soroban_sdk::{testutils::Address as _, Address, Env, IntoVal, String, Symbol, Val, Vec};
use vault::{Vault, VaultClient, VaultParams};

use crate::common::{
    token_balance, MockPhoenixPool, MockPhoenixPoolClient, MockToken, MockTokenClient,
};

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

    // Use asset_a as the vault's base asset.
    let vault_share_id = env.register(
        ShareTokenContract,
        (
            manager.clone(),
            String::from_str(&env, "Phoenix Vault Share"),
            String::from_str(&env, "PVS"),
            7u32,
        ),
    );

    let vault_id = env.register(
        Vault,
        (VaultParams {
            admin: manager.clone(),
            manager: manager.clone(),
            manager_name: None,
            trader: trader.clone(),
            base_asset: asset_a.clone(),
            share_token: vault_share_id.clone(),
            share_token_admin: manager.clone(),
            treasury: manager.clone(),
            entry_fee_bps: 0,
            exit_fee_bps: 0,
            mgmt_fee_bps: 0,
            perf_fee_bps: 0,
            factory: None,
            is_private: false,
        },),
    );
    let vault = VaultClient::new(&env, &vault_id);

    // MockPhoenixPool.
    let pool_id = env.register(MockPhoenixPool, ());
    MockPhoenixPoolClient::new(&env, &pool_id).phoenix_init(&share_token, &asset_a, &asset_b);

    // PhoenixLpStrategy — auto-queries share_token from pool during initialize.
    let strategy_id = env.register(PhoenixLpStrategy, ());
    PhoenixLpStrategyClient::new(&env, &strategy_id).initialize(
        &vault_id,
        &asset_a,
        &asset_b,
        &pool_id,
        &manager,
        &String::from_str(&env, "Phoenix USDC-XLM"),
    );

    // Whitelist asset_a (base) in portfolio.  LP positions require an oracle for
    // accurate NAV; we disable the TVL guard in these dispatch-focused tests.
    vault.add_portfolio_asset(&manager, &asset_a);

    vault.add_active_guard(&manager, &strategy_id);
    let ops: Vec<Symbol> = soroban_sdk::vec![
        &env,
        Symbol::new(&env, "add_liquidity"),
        Symbol::new(&env, "remove_liquidity"),
    ];
    vault.set_authorized_ops(&manager, &strategy_id, &ops);

    // Disable TVL guard: LP oracle not configured in these dispatch tests.
    vault.set_max_loss_bps(&manager, &0u32);

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
/// The strategy first pulls tokens from vault via transfer_from(strategy, vault, strategy).
#[test]
fn test_phoenix_add_liquidity_via_execute_op() {
    let w = setup_phoenix();
    let amount_a = 1_000_0000000i128;
    let amount_b = 1_000_0000000i128;

    // Strategy pulls from vault → vault must approve strategy.
    MockTokenClient::new(&w.env, &w.asset_a).approve(
        &w.vault_addr,
        &w.strategy_addr,
        &amount_a,
        &1000u32,
    );
    MockTokenClient::new(&w.env, &w.asset_b).approve(
        &w.vault_addr,
        &w.strategy_addr,
        &amount_b,
        &1000u32,
    );
    // Pool pulls from strategy → strategy approves pool (done inside add_liquidity).
    // We also need vault → pool approvals for the mock's transfer_from(pool, strategy, pool).
    // The strategy sets approve(strategy, pool) before calling provide_liquidity,
    // so no extra setup is needed here.

    let args: Vec<Val> = soroban_sdk::vec![
        &w.env,
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
    // Strategy's tracked share count matches.
    assert_eq!(w.strategy.get_share_balance(), shares_minted);

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
    MockTokenClient::new(&w.env, &w.asset_a).approve(
        &w.vault_addr,
        &w.strategy_addr,
        &amount_a,
        &1000u32,
    );
    MockTokenClient::new(&w.env, &w.asset_b).approve(
        &w.vault_addr,
        &w.strategy_addr,
        &amount_b,
        &1000u32,
    );
    let add_args: Vec<Val> = soroban_sdk::vec![
        &w.env,
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

/// asset_in_use returns true for both underlying assets after add_liquidity.
#[test]
fn test_phoenix_asset_in_use_after_add_liquidity() {
    let w = setup_phoenix();
    let amount = 1_000_0000000i128;

    assert!(!w.strategy.asset_in_use(&w.vault_addr, &w.asset_a));
    assert!(!w.strategy.asset_in_use(&w.vault_addr, &w.asset_b));

    MockTokenClient::new(&w.env, &w.asset_a).approve(
        &w.vault_addr,
        &w.strategy_addr,
        &amount,
        &1000u32,
    );
    MockTokenClient::new(&w.env, &w.asset_b).approve(
        &w.vault_addr,
        &w.strategy_addr,
        &amount,
        &1000u32,
    );
    let args: Vec<Val> = soroban_sdk::vec![
        &w.env,
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

/// get_total_value returns 0 when no oracle is configured.
#[test]
fn test_phoenix_get_total_value_without_oracle_returns_zero() {
    let w = setup_phoenix();
    assert_eq!(w.strategy.get_total_value(&w.vault_addr), 0);

    // After adding liquidity, still 0 without an oracle.
    let amount = 1_000_0000000i128;
    MockTokenClient::new(&w.env, &w.asset_a).approve(
        &w.vault_addr,
        &w.strategy_addr,
        &amount,
        &1000u32,
    );
    MockTokenClient::new(&w.env, &w.asset_b).approve(
        &w.vault_addr,
        &w.strategy_addr,
        &amount,
        &1000u32,
    );
    let args: Vec<Val> = soroban_sdk::vec![
        &w.env,
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

    assert_eq!(w.strategy.get_total_value(&w.vault_addr), 0);
}
