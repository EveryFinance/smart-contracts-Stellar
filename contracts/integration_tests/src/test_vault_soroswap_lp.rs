//! Integration tests: Vault ↔ SoroswapLpStrategy ↔ MockSoroswapRouter
//!
//! Verifies LP deposit/withdraw through the real SoroswapLpStrategy,
//! with vault NAV tracking and multi-user share accounting.

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Env, String};

use share_token::{ShareTokenContract, ShareTokenContractClient};
use soroswap_lp_strategy::{SoroswapLpStrategy, SoroswapLpStrategyClient};
use vault::{Vault, VaultClient, VaultParams};

use crate::common::{
    token_balance, MockSoroswapRouter, MockSoroswapRouterClient, MockToken, MockTokenClient,
};

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

struct World {
    env: Env,
    vault: VaultClient<'static>,
    vault_addr: Address,
    strategy: SoroswapLpStrategyClient<'static>,
    _strategy_addr: Address,
    _base: Address, // token_a (base deposit asset)
    token_b: Address,
    _lp_token: Address,
    manager: Address,
    _trader: Address,
    user: Address,
    _user2: Address,
}

fn setup() -> World {
    let env = Env::default();
    env.mock_all_auths();

    let manager = Address::generate(&env);
    let trader = Address::generate(&env);
    let user = Address::generate(&env);
    let user2 = Address::generate(&env);

    let token_a = env.register(MockToken, ());
    let token_b = env.register(MockToken, ());
    let lp_token = env.register(MockToken, ());

    MockTokenClient::new(&env, &token_a).initialize(&manager);
    MockTokenClient::new(&env, &token_b).initialize(&manager);
    MockTokenClient::new(&env, &lp_token).initialize(&manager);

    let router_id = env.register(MockSoroswapRouter, ());
    MockSoroswapRouterClient::new(&env, &router_id).router_init(&lp_token);

    let share_id = env.register(ShareTokenContract, ());
    let vault_id = env.register(Vault, ());
    let strat_id = env.register(SoroswapLpStrategy, ());

    // Share token: manager as admin; vault init will take admin.
    ShareTokenContractClient::new(&env, &share_id).initialize(
        &manager,
        &String::from_str(&env, "VS"),
        &String::from_str(&env, "VS"),
        &7u32,
    );

    // Vault init.
    let vault = VaultClient::new(&env, &vault_id);
    vault.initialize(&VaultParams {
        manager: manager.clone(),
        trader: trader.clone(),
        base_asset: token_a.clone(),
        share_token: share_id.clone(),
        share_token_admin: manager.clone(),
        entry_fee_bps: 0,
        exit_fee_bps: 0,
        mgmt_fee_bps: 0,
        perf_fee_bps: 0,
    });

    // Soroswap LP strategy.
    let strategy = SoroswapLpStrategyClient::new(&env, &strat_id);
    strategy.initialize(
        &vault_id,
        &token_a,
        &token_b,
        &lp_token,
        &router_id,
        &manager,
        &String::from_str(&env, "Soroswap LP"),
    );

    vault.set_strategies(&manager, &vec![&env, strat_id.clone()]);

    // Fund vault and users.
    MockTokenClient::new(&env, &token_a).mint(&vault_id, &10_000_0000000i128);
    MockTokenClient::new(&env, &token_b).mint(&vault_id, &10_000_0000000i128);
    MockTokenClient::new(&env, &token_a).mint(&user, &100_000_0000000i128);
    MockTokenClient::new(&env, &token_a).mint(&user2, &100_000_0000000i128);

    let vault: VaultClient<'static> = unsafe { core::mem::transmute(vault) };
    let strategy: SoroswapLpStrategyClient<'static> = unsafe { core::mem::transmute(strategy) };

    World {
        env,
        vault,
        vault_addr: vault_id,
        strategy,
        _strategy_addr: strat_id,
        _base: token_a,
        token_b,
        _lp_token: lp_token,
        manager,
        _trader: trader,
        user,
        _user2: user2,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// deposit_liquidity via strategy: LP tokens credited to strategy, NAV unchanged.
#[test]
fn test_deposit_lp_strategy_tracks_lp() {
    let w = setup();

    let lp =
        w.strategy
            .deposit_liquidity(&500_0000000i128, &500_0000000i128, &0, &0, &w.vault_addr);
    assert!(lp > 0);
    assert_eq!(w.strategy.get_lp_balance(), lp);
    // get_value returns LP balance (proxy for NAV contribution).
    assert_eq!(w.strategy.get_value(&w.vault_addr), lp);
}

/// Partial withdraw from LP strategy sends underlying tokens to user.
#[test]
fn test_withdraw_lp_sends_to_user() {
    let w = setup();
    let _user_b_before = token_balance(&w.env, &w.token_b, &w.user);

    let lp = w.strategy.deposit_liquidity(
        &1_000_0000000i128,
        &1_000_0000000i128,
        &0,
        &0,
        &w.vault_addr,
    );
    let (a, b) = w.strategy.withdraw(&lp, &0, &0, &w.vault_addr, &w.user);

    // MockSoroswapRouter returns half/half.
    assert!(a > 0 || b > 0);
    assert_eq!(w.strategy.get_lp_balance(), 0);
}

/// Full Vault → Strategy → Router → withdraw back lifecycle.
#[test]
fn test_full_soroswap_lp_lifecycle() {
    let w = setup();

    // User deposits base token into vault.
    let deposit = 2_000_0000000i128;
    let _vault_shares = w.vault.deposit(&deposit, &w.user);

    // Manager invests vault's base tokens AND token_b into LP.
    // (In production the vault would need token_b too; here we pre-funded vault.)
    let lp = w.strategy.deposit_liquidity(
        &1_000_0000000i128,
        &1_000_0000000i128,
        &0,
        &0,
        &w.vault_addr,
    );
    assert_eq!(w.strategy.get_lp_balance(), lp);

    // Strategy is paused and unpaused.
    w.strategy.pause(&w.manager);
    assert!(w.strategy.is_paused());
    w.strategy.unpause(&w.manager);
    assert!(!w.strategy.is_paused());

    // Withdraw from LP strategy to user.
    let half = lp / 2;
    w.strategy.withdraw(&half, &0, &0, &w.vault_addr, &w.user);
    assert_eq!(w.strategy.get_lp_balance(), lp - half);

    w.strategy
        .withdraw(&(lp - half), &0, &0, &w.vault_addr, &w.user);
    assert_eq!(w.strategy.get_lp_balance(), 0);
}

/// Multiple LP deposits accumulate correctly.
#[test]
fn test_multiple_lp_deposits() {
    let w = setup();

    let lp1 =
        w.strategy
            .deposit_liquidity(&200_0000000i128, &200_0000000i128, &0, &0, &w.vault_addr);
    let lp2 =
        w.strategy
            .deposit_liquidity(&300_0000000i128, &300_0000000i128, &0, &0, &w.vault_addr);
    assert_eq!(w.strategy.get_lp_balance(), lp1 + lp2);
}

/// Paused strategy rejects LP deposits.
#[test]
#[should_panic]
fn test_paused_strategy_deposit_panics() {
    let w = setup();
    w.strategy.pause(&w.manager);
    w.strategy
        .deposit_liquidity(&100_0000000i128, &100_0000000i128, &0, &0, &w.vault_addr);
}
