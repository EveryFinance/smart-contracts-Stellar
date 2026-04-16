//! Integration tests: Vault ↔ BlendStrategy ↔ MockBlendPool
//!
//! Verifies the full invest/unwind lifecycle using the real BlendStrategy
//! contract and a simplified mock of the Blend pool.

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Env, String};

use blend_strategy::{BlendStrategy, BlendStrategyClient};
use share_token::{ShareTokenContract, ShareTokenContractClient};
use vault::{Vault, VaultClient, VaultParams};

use crate::common::{
    token_balance, MockBlendPool, MockBlendPoolClient, MockToken, MockTokenClient,
};

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

struct World {
    env: Env,
    vault: VaultClient<'static>,
    vault_addr: Address,
    strategy: BlendStrategyClient<'static>,
    strategy_addr: Address,
    base: Address,
    _blend_pool: Address,
    manager: Address,
    _trader: Address,
    user: Address,
}

fn setup() -> World {
    let env = Env::default();
    // Allow non-root auth so vault can authorize token sub-calls made by the strategy.
    env.mock_all_auths_allowing_non_root_auth();

    let manager = Address::generate(&env);
    let trader = Address::generate(&env);
    let user = Address::generate(&env);

    // Base token (e.g. USDC).
    let base = env.register(MockToken, ());
    MockTokenClient::new(&env, &base).initialize(&manager);

    // Share token (vault will be admin after vault.initialize).
    let share_id = env.register(ShareTokenContract, ());

    // Blend pool mock.
    let blend_pool = env.register(MockBlendPool, ());
    MockBlendPoolClient::new(&env, &blend_pool).blend_init();
    MockBlendPoolClient::new(&env, &blend_pool).set_token(&base);

    // Real Vault.
    let vault_id = env.register(Vault, ());

    // Real BlendStrategy.
    let strat_id = env.register(BlendStrategy, ());
    let strategy = BlendStrategyClient::new(&env, &strat_id);

    // Initialize share token with manager as admin; vault init will take admin.
    ShareTokenContractClient::new(&env, &share_id).initialize(
        &manager,
        &String::from_str(&env, "VS"),
        &String::from_str(&env, "VS"),
        &7u32,
    );

    // Initialize vault.
    let vault = VaultClient::new(&env, &vault_id);
    vault.initialize(&VaultParams {
        manager: manager.clone(),
        trader: trader.clone(),
        base_asset: base.clone(),
        share_token: share_id.clone(),
        share_token_admin: manager.clone(),
        entry_fee_bps: 0,
        exit_fee_bps: 0,
        mgmt_fee_bps: 0,
        perf_fee_bps: 0,
    });

    // Initialize Blend strategy.
    strategy.initialize(
        &vault_id,
        &base,
        &blend_pool,
        &manager,
        &String::from_str(&env, "Blend USDC"),
    );

    // Whitelist strategy.
    vault.set_strategies(&manager, &vec![&env, strat_id.clone()]);

    // Fund user with base tokens.
    MockTokenClient::new(&env, &base).mint(&user, &100_000_0000000i128);

    let vault: VaultClient<'static> = unsafe { core::mem::transmute(vault) };
    let strategy: BlendStrategyClient<'static> = unsafe { core::mem::transmute(strategy) };

    World {
        env,
        vault,
        vault_addr: vault_id,
        strategy,
        strategy_addr: strat_id,
        base,
        _blend_pool: blend_pool,
        manager,
        _trader: trader,
        user,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Vault deposits, manager invests into Blend, NAV stays constant.
#[test]
fn test_invest_blend_nav_constant() {
    let w = setup();
    let deposit = 1_000_0000000i128;
    let invest = 600_0000000i128;

    w.vault.deposit(&deposit, &w.user);
    assert_eq!(w.vault.get_nav(), deposit);

    w.vault.invest(&w.manager, &w.strategy_addr, &invest);

    // Vault base balance reduced by invest amount.
    assert_eq!(
        token_balance(&w.env, &w.base, &w.vault_addr),
        deposit - invest
    );
    // Strategy position grows.
    assert_eq!(w.strategy.get_value(&w.vault_addr), invest);
    // NAV = vault_balance + strategy_value = unchanged.
    assert_eq!(w.vault.get_nav(), deposit);
}

/// Unwind returns tokens from Blend strategy back to vault.
#[test]
fn test_unwind_blend_returns_to_vault() {
    let w = setup();
    let deposit = 1_000_0000000i128;
    let invest = 400_0000000i128;

    w.vault.deposit(&deposit, &w.user);
    w.vault.invest(&w.manager, &w.strategy_addr, &invest);

    let vault_before = token_balance(&w.env, &w.base, &w.vault_addr);
    w.vault.unwind(&w.manager, &w.strategy_addr, &invest);

    // Tokens returned to vault.
    assert_eq!(
        token_balance(&w.env, &w.base, &w.vault_addr),
        vault_before + invest
    );
    // Strategy position zeroed.
    assert_eq!(w.strategy.get_value(&w.vault_addr), 0);
    // NAV unchanged.
    assert_eq!(w.vault.get_nav(), deposit);
}

/// Full lifecycle: deposit → invest → unwind → withdraw.
#[test]
fn test_full_blend_lifecycle() {
    let w = setup();
    let deposit = 2_000_0000000i128;
    let invest = 1_500_0000000i128;
    let partial = 500_0000000i128;

    // Deposit.
    let shares = w.vault.deposit(&deposit, &w.user);
    assert_eq!(w.vault.get_nav(), deposit);

    // Invest most of it.
    w.vault.invest(&w.manager, &w.strategy_addr, &invest);
    assert_eq!(w.vault.get_nav(), deposit); // NAV constant

    // Partial unwind.
    w.vault.unwind(&w.manager, &w.strategy_addr, &partial);
    assert_eq!(w.strategy.get_value(&w.vault_addr), invest - partial);

    // Unwind rest.
    w.vault
        .unwind(&w.manager, &w.strategy_addr, &(invest - partial));
    assert_eq!(w.strategy.get_value(&w.vault_addr), 0);

    // User withdraws all shares — receives full deposit back.
    let user_before = token_balance(&w.env, &w.base, &w.user);
    let returned = w.vault.withdraw(&shares, &w.user, &w.user);
    assert_eq!(returned, deposit);
    assert_eq!(
        token_balance(&w.env, &w.base, &w.user),
        user_before + deposit
    );
}

/// Multiple invest/unwind rounds don't corrupt NAV tracking.
#[test]
fn test_multiple_invest_unwind_rounds() {
    let w = setup();
    let deposit = 3_000_0000000i128;
    w.vault.deposit(&deposit, &w.user);

    for _ in 0..3u32 {
        w.vault
            .invest(&w.manager, &w.strategy_addr, &1_000_0000000i128);
        assert_eq!(w.vault.get_nav(), deposit);
        w.vault
            .unwind(&w.manager, &w.strategy_addr, &1_000_0000000i128);
        assert_eq!(w.vault.get_nav(), deposit);
    }

    // All value back in vault.
    assert_eq!(token_balance(&w.env, &w.base, &w.vault_addr), deposit);
}

/// Strategy is paused — vault cannot invest into it.
#[test]
#[should_panic]
fn test_invest_into_paused_strategy_panics() {
    let w = setup();
    w.vault.deposit(&1_000_0000000i128, &w.user);
    w.strategy.pause(&w.manager);
    w.vault
        .invest(&w.manager, &w.strategy_addr, &500_0000000i128);
}
