//! Integration tests: Vault ↔ Soroswap LP strategy (execute_op API)

#![cfg(test)]

use soroswap_lp_strategy::{SoroswapLpStrategy, SoroswapLpStrategyClient};
use share_token::ShareTokenContract;
use soroban_sdk::{testutils::Address as _, Address, Env, IntoVal, String, Symbol, Val, Vec};
use vault::{Vault, VaultClient, VaultParams};

use crate::common::{
    token_balance, MockSoroswapRouter, MockSoroswapRouterClient, MockToken, MockTokenClient,
};

// ---------------------------------------------------------------------------
// World fixture
// ---------------------------------------------------------------------------

struct SoroswapWorld {
    env: Env,
    vault: VaultClient<'static>,
    vault_addr: Address,
    strategy: SoroswapLpStrategyClient<'static>,
    strategy_addr: Address,
    asset_a: Address,
    asset_b: Address,
    lp_token: Address,
    manager: Address,
    trader: Address,
    user: Address,
}

fn setup_soroswap() -> SoroswapWorld {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let manager = Address::generate(&env);
    let trader = Address::generate(&env);
    let user = Address::generate(&env);

    // Two underlying assets and an LP token (all MockToken).
    let asset_a = env.register(MockToken, ());
    MockTokenClient::new(&env, &asset_a).initialize(&manager);

    let asset_b = env.register(MockToken, ());
    MockTokenClient::new(&env, &asset_b).initialize(&manager);

    let lp_token = env.register(MockToken, ());
    MockTokenClient::new(&env, &lp_token).initialize(&manager);

    // Use asset_a as vault base (simplest setup — no oracle needed for basic ops).
    let share_id = env.register(
        ShareTokenContract,
        (
            manager.clone(),
            String::from_str(&env, "SS Vault Share"),
            String::from_str(&env, "SVS"),
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
            share_token: share_id.clone(),
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

    // MockSoroswapRouter — mints LP tokens on add_liquidity, mints assets on remove.
    let router_id = env.register(MockSoroswapRouter, ());
    MockSoroswapRouterClient::new(&env, &router_id).router_init(&lp_token);

    // SoroswapLpStrategy.
    let strategy_id = env.register(SoroswapLpStrategy, ());
    SoroswapLpStrategyClient::new(&env, &strategy_id).initialize(
        &vault_id,
        &asset_a,
        &asset_b,
        &lp_token,
        &router_id,
        &manager,
        &String::from_str(&env, "Soroswap USDC-XLM"),
    );

    // Whitelist asset_a (base) in portfolio.  asset_b is not whitelisted here
    // because these tests pre-fund the vault via mint, not deposit; only asset_a
    // needs to appear in NAV for the TVL check.  asset_b and LP positions require
    // an oracle for accurate NAV — we disable the TVL guard instead.
    vault.add_portfolio_asset(&manager, &asset_a);

    vault.add_active_guard(&manager, &strategy_id);
    let ops: Vec<Symbol> = soroban_sdk::vec![
        &env,
        Symbol::new(&env, "add_liquidity"),
        Symbol::new(&env, "remove_liquidity"),
        Symbol::new(&env, "swap"),
    ];
    vault.set_authorized_ops(&manager, &strategy_id, &ops);

    // Disable TVL guard: LP positions require an oracle for get_total_value,
    // which is not configured in these dispatch-focused tests.
    vault.set_max_loss_bps(&manager, &0u32);

    // Fund the vault with both underlying assets.
    MockTokenClient::new(&env, &asset_a).mint(&vault_id, &50_000_0000000i128);
    MockTokenClient::new(&env, &asset_b).mint(&vault_id, &50_000_0000000i128);
    // Fund user with asset_a for deposits.
    MockTokenClient::new(&env, &asset_a).mint(&user, &10_000_0000000i128);

    let vault: VaultClient<'static> = unsafe { core::mem::transmute(vault) };
    let strategy: SoroswapLpStrategyClient<'static> =
        unsafe { core::mem::transmute(SoroswapLpStrategyClient::new(&env, &strategy_id)) };

    SoroswapWorld {
        env,
        vault,
        vault_addr: vault_id,
        strategy,
        strategy_addr: strategy_id,
        asset_a,
        asset_b,
        lp_token,
        manager,
        trader,
        user,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// add_liquidity via execute_op: vault tokens → strategy → router → LP tokens
/// minted to strategy.
#[test]
fn test_soroswap_add_liquidity_via_execute_op() {
    let w = setup_soroswap();
    let amount_a = 1_000_0000000i128;
    let amount_b = 1_000_0000000i128;

    // strategy.add_liquidity calls transfer_from(strategy, vault, strategy) for both assets.
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

    // MockRouter mints min(amount_a, amount_b) LP tokens to strategy.
    let lp_minted = amount_a.min(amount_b);
    assert_eq!(token_balance(&w.env, &w.lp_token, &w.strategy_addr), lp_minted);

    // Strategy's tracked LP balance matches.
    assert_eq!(w.strategy.get_lp_balance(), lp_minted);

    // Vault lost both assets.
    let vault_a_after = token_balance(&w.env, &w.asset_a, &w.vault_addr);
    let vault_b_after = token_balance(&w.env, &w.asset_b, &w.vault_addr);
    assert_eq!(vault_a_after, 50_000_0000000i128 - amount_a);
    assert_eq!(vault_b_after, 50_000_0000000i128 - amount_b);
}

/// remove_liquidity via execute_op: LP tokens burned at router → underlying
/// tokens minted directly to vault.
#[test]
fn test_soroswap_remove_liquidity_via_execute_op() {
    let w = setup_soroswap();
    let amount_a = 2_000_0000000i128;
    let amount_b = 2_000_0000000i128;

    // Add liquidity first.
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

    let lp_minted = amount_a.min(amount_b);
    let vault_a_before = token_balance(&w.env, &w.asset_a, &w.vault_addr);
    let vault_b_before = token_balance(&w.env, &w.asset_b, &w.vault_addr);

    // Remove all LP tokens.
    let remove_args: Vec<Val> = soroban_sdk::vec![
        &w.env,
        lp_minted.into_val(&w.env),
        0i128.into_val(&w.env), // min_a
        0i128.into_val(&w.env), // min_b
    ];
    w.vault.execute_op(
        &w.trader,
        &w.strategy_addr,
        &Symbol::new(&w.env, "remove_liquidity"),
        &remove_args,
    );

    // MockRouter's remove_liquidity mints half each of asset_a/asset_b to vault.
    let expected_each = lp_minted / 2;
    assert_eq!(
        token_balance(&w.env, &w.asset_a, &w.vault_addr),
        vault_a_before + expected_each
    );
    assert_eq!(
        token_balance(&w.env, &w.asset_b, &w.vault_addr),
        vault_b_before + expected_each
    );

    // Strategy's tracked LP balance is zero (internal accounting).
    assert_eq!(w.strategy.get_lp_balance(), 0);
    // Note: the mock router does not burn LP token on-chain (it lacks caller identity);
    // real Soroswap uses an allowance-based pull from the strategy to the pair.
    // The tracked balance above is the authoritative measure for NAV.
}

/// swap via execute_op: vault's from_asset → strategy → router → to_asset
/// minted directly to vault.
#[test]
fn test_soroswap_swap_via_execute_op() {
    let w = setup_soroswap();
    let amount_in = 500_0000000i128;

    let vault_a_before = token_balance(&w.env, &w.asset_a, &w.vault_addr);
    let vault_b_before = token_balance(&w.env, &w.asset_b, &w.vault_addr);

    // strategy.swap calls transfer_from(strategy, vault, strategy, amount_in) on asset_a.
    MockTokenClient::new(&w.env, &w.asset_a).approve(
        &w.vault_addr,
        &w.strategy_addr,
        &amount_in,
        &1000u32,
    );

    let args: Vec<Val> = soroban_sdk::vec![
        &w.env,
        w.asset_a.clone().into_val(&w.env), // from_asset
        w.asset_b.clone().into_val(&w.env), // to_asset
        amount_in.into_val(&w.env),
        0i128.into_val(&w.env), // min_out
    ];
    w.vault.execute_op(
        &w.trader,
        &w.strategy_addr,
        &Symbol::new(&w.env, "swap"),
        &args,
    );

    // MockRouter's 1:1 swap: vault loses asset_a, gains asset_b.
    assert_eq!(
        token_balance(&w.env, &w.asset_a, &w.vault_addr),
        vault_a_before - amount_in
    );
    // MockRouter mints amount_out = amount_in of asset_b to vault.
    assert_eq!(
        token_balance(&w.env, &w.asset_b, &w.vault_addr),
        vault_b_before + amount_in
    );
}

/// get_total_value returns 0 when no oracle is configured (early exit).
#[test]
fn test_soroswap_get_total_value_without_oracle_returns_zero() {
    let w = setup_soroswap();
    // No oracle set → strategy returns 0 rather than panicking.
    assert_eq!(w.strategy.get_total_value(&w.vault_addr), 0);
}

/// asset_in_use returns true after add_liquidity for both underlying assets.
#[test]
fn test_soroswap_asset_in_use_after_add_liquidity() {
    let w = setup_soroswap();
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
