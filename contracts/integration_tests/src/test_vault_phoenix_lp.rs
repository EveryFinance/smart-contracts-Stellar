//! Integration tests: Vault ↔ PhoenixLpStrategy ↔ MockPhoenixPool
//!
//! Verifies the Phoenix LP strategy's share-token tracking, provide/withdraw
//! lifecycle, and interaction with the vault's NAV calculation.

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Env, String};

use phoenix_lp_strategy::{PhoenixLpStrategy, PhoenixLpStrategyClient};
use share_token::{ShareTokenContract, ShareTokenContractClient};
use vault::{Vault, VaultClient, VaultParams};

use crate::common::{
    token_balance, MockOracle, MockOracleClient, MockPhoenixPool, MockPhoenixPoolClient, MockToken,
    MockTokenClient,
};

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

struct World {
    env: Env,
    vault: VaultClient<'static>,
    vault_addr: Address,
    strategy: PhoenixLpStrategyClient<'static>,
    strategy_addr: Address,
    token_a: Address,
    token_b: Address,
    share_token: Address, // Phoenix LP share token
    _pool: Address,
    manager: Address,
    _trader: Address,
    user: Address,
}

fn setup() -> World {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let manager = Address::generate(&env);
    let trader = Address::generate(&env);
    let user = Address::generate(&env);

    let token_a = env.register(MockToken, ());
    let token_b = env.register(MockToken, ());
    let share_token = env.register(MockToken, ());

    MockTokenClient::new(&env, &token_a).initialize(&manager);
    MockTokenClient::new(&env, &token_b).initialize(&manager);
    MockTokenClient::new(&env, &share_token).initialize(&manager);

    // Phoenix pool mock.
    let pool = env.register(MockPhoenixPool, ());
    MockPhoenixPoolClient::new(&env, &pool).phoenix_init(&share_token, &token_a, &token_b);

    let vault_share_id = env.register(ShareTokenContract, ());
    let strat_id = env.register(PhoenixLpStrategy, ());

    // Share token: manager as admin; vault constructor will take admin.
    ShareTokenContractClient::new(&env, &vault_share_id).initialize(
        &manager,
        &String::from_str(&env, "VS"),
        &String::from_str(&env, "VS"),
        &7u32,
    );

    // Deploy vault with constructor (atomic, front-run-proof).
    let vault_id = env.register(
        Vault,
        (VaultParams {
            manager: manager.clone(),
            trader: trader.clone(),
            base_asset: token_a.clone(),
            share_token: vault_share_id.clone(),
            share_token_admin: manager.clone(),
            entry_fee_bps: 0,
            exit_fee_bps: 0,
            mgmt_fee_bps: 0,
            perf_fee_bps: 0,
        },),
    );
    let vault = VaultClient::new(&env, &vault_id);

    // Phoenix LP strategy (auto-queries share token from pool).
    let strategy = PhoenixLpStrategyClient::new(&env, &strat_id);
    strategy.initialize(
        &vault_id,
        &token_a,
        &token_b,
        &pool,
        &manager,
        &String::from_str(&env, "Phoenix LP"),
    );

    vault.set_strategies(&manager, &vec![&env, strat_id.clone()]);

    // Fund vault with both tokens.
    MockTokenClient::new(&env, &token_a).mint(&vault_id, &10_000_0000000i128);
    MockTokenClient::new(&env, &token_b).mint(&vault_id, &10_000_0000000i128);
    MockTokenClient::new(&env, &token_a).mint(&user, &100_000_0000000i128);

    let vault: VaultClient<'static> = unsafe { core::mem::transmute(vault) };
    let strategy: PhoenixLpStrategyClient<'static> = unsafe { core::mem::transmute(strategy) };

    World {
        env,
        vault,
        vault_addr: vault_id,
        strategy,
        strategy_addr: strat_id,
        token_a,
        token_b,
        share_token,
        _pool: pool,
        manager,
        _trader: trader,
        user,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Phoenix pool auto-queries share token on initialize.
#[test]
fn test_initialize_auto_queries_share_token() {
    let w = setup();
    // Strategy should have stored the share token queried from the pool.
    assert_eq!(w.strategy.share_token(), w.share_token);
}

/// Providing liquidity mints Phoenix shares to the strategy.
#[test]
fn test_provide_liquidity_mints_shares() {
    let w = setup();
    let shares =
        w.strategy
            .deposit_liquidity(&500_0000000i128, &500_0000000i128, &0, &0, &w.vault_addr);
    assert!(shares > 0);
    assert_eq!(w.strategy.get_share_balance(), shares);

    // Phoenix LP share tokens held by strategy.
    let strategy_shares = token_balance(&w.env, &w.share_token, &w.strategy_addr);
    assert_eq!(strategy_shares, shares);
}

/// Withdrawing from Phoenix LP delivers underlying tokens directly to user.
#[test]
fn test_withdraw_delivers_to_user() {
    let w = setup();
    let shares = w.strategy.deposit_liquidity(
        &1_000_0000000i128,
        &1_000_0000000i128,
        &0,
        &0,
        &w.vault_addr,
    );

    let user_a_before = token_balance(&w.env, &w.token_a, &w.user);
    let user_b_before = token_balance(&w.env, &w.token_b, &w.user);

    let (a, b) = w.strategy.withdraw(&shares, &0, &0, &w.vault_addr, &w.user);

    // Tokens go directly to user (not vault).
    assert_eq!(
        token_balance(&w.env, &w.token_a, &w.user),
        user_a_before + a
    );
    assert_eq!(
        token_balance(&w.env, &w.token_b, &w.user),
        user_b_before + b
    );
    assert_eq!(w.strategy.get_share_balance(), 0);
}

/// Partial withdraw leaves remaining shares in strategy.
#[test]
fn test_partial_withdraw_leaves_remainder() {
    let w = setup();
    let shares =
        w.strategy
            .deposit_liquidity(&800_0000000i128, &800_0000000i128, &0, &0, &w.vault_addr);

    let half = shares / 2;
    w.strategy.withdraw(&half, &0, &0, &w.vault_addr, &w.user);
    assert_eq!(w.strategy.get_share_balance(), shares - half);

    w.strategy
        .withdraw(&(shares - half), &0, &0, &w.vault_addr, &w.user);
    assert_eq!(w.strategy.get_share_balance(), 0);
}

/// Full lifecycle with vault deposit and multiple LP rounds.
#[test]
fn test_full_phoenix_lp_lifecycle() {
    let w = setup();

    // User deposits into vault.
    let vault_deposit = 2_000_0000000i128;
    let vault_shares = w.vault.deposit(&vault_deposit, &w.user);
    assert!(vault_shares > 0);

    // Manager provides liquidity to Phoenix pool.
    let lp_shares =
        w.strategy
            .deposit_liquidity(&500_0000000i128, &500_0000000i128, &0, &0, &w.vault_addr);

    // Strategy tracks LP correctly.
    assert_eq!(w.strategy.get_share_balance(), lp_shares);
    assert_eq!(w.strategy.get_value(&w.vault_addr), lp_shares);

    // Withdraw half.
    let half = lp_shares / 2;
    w.strategy.withdraw(&half, &0, &0, &w.vault_addr, &w.user);
    assert_eq!(w.strategy.get_share_balance(), lp_shares - half);

    // Withdraw rest.
    w.strategy
        .withdraw(&(lp_shares - half), &0, &0, &w.vault_addr, &w.user);
    assert_eq!(w.strategy.get_share_balance(), 0);
}

/// Paused strategy blocks both provide_liquidity and withdraw.
#[test]
#[should_panic]
fn test_paused_phoenix_strategy_deposit_panics() {
    let w = setup();
    w.strategy.pause(&w.manager);
    w.strategy
        .deposit_liquidity(&100_0000000i128, &100_0000000i128, &0, &0, &w.vault_addr);
}

/// Paused strategy blocks withdraw.
#[test]
#[should_panic]
fn test_paused_phoenix_strategy_withdraw_panics() {
    let w = setup();
    let shares =
        w.strategy
            .deposit_liquidity(&100_0000000i128, &100_0000000i128, &0, &0, &w.vault_addr);
    w.strategy.pause(&w.manager);
    w.strategy.withdraw(&shares, &0, &0, &w.vault_addr, &w.user);
}

/// Vault-level LP invest must fail when strategy oracle is missing.
#[test]
#[should_panic(expected = "Error(Contract, #19)")]
fn test_vault_invest_lp_requires_oracle() {
    let w = setup();
    w.vault.set_lp_strategy(&w.manager, &w.strategy_addr, &true);
    w.vault.invest_lp(
        &w.manager,
        &w.strategy_addr,
        &500_0000000i128,
        &500_0000000i128,
        &0,
        &0,
    );
}

/// With strategy oracle configured, vault NAV includes Phoenix reserve-decomposed value.
#[test]
fn test_vault_nav_with_phoenix_oracle_valuation() {
    let w = setup();
    let oracle = w.env.register(MockOracle, ());
    let oracle_client = MockOracleClient::new(&w.env, &oracle);

    // Price both assets at 1.0 in PRICE_PRECISION units.
    oracle_client.set_price(&w.token_a, &10_000_000i128);
    oracle_client.set_price(&w.token_b, &10_000_000i128);
    w.strategy.set_oracle(&w.manager, &oracle);
    w.vault.set_lp_strategy(&w.manager, &w.strategy_addr, &true);

    // invest_lp spends 500 token_a from vault base balance and deposits
    // 500/500 into the Phoenix pool; strategy value should read as 1000.
    let _lp = w.vault.invest_lp(
        &w.manager,
        &w.strategy_addr,
        &500_0000000i128,
        &500_0000000i128,
        &0,
        &0,
    );

    assert_eq!(w.strategy.get_value(&w.vault_addr), 1_000_0000000i128);
    // NAV = vault token_a balance (10_000 - 500) + strategy value (1_000).
    assert_eq!(w.vault.get_nav(), 10_500_0000000i128);
}
