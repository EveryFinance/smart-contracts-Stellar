//! Integration tests: Vault ↔ Blend strategy (execute_op API)

#![cfg(test)]

use blend_strategy::{BlendStrategy, BlendStrategyClient};
use share_token::{ShareTokenContract, ShareTokenContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env, IntoVal, String, Symbol, Val, Vec};
use vault::{Vault, VaultClient, VaultParams};

use crate::common::{
    token_balance, MockBlendPool, MockBlendPoolClient, MockToken, MockTokenClient,
};

// ---------------------------------------------------------------------------
// World fixture
// ---------------------------------------------------------------------------

struct BlendWorld {
    env: Env,
    vault: VaultClient<'static>,
    vault_addr: Address,
    strategy: BlendStrategyClient<'static>,
    strategy_addr: Address,
    blend_pool: MockBlendPoolClient<'static>,
    share_token_addr: Address,
    base: Address,
    manager: Address,
    trader: Address,
    user: Address,
}

fn setup_blend() -> BlendWorld {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let manager = Address::generate(&env);
    let trader = Address::generate(&env);
    let user = Address::generate(&env);

    // Base asset (USDC mock).
    let base = env.register(MockToken, ());
    MockTokenClient::new(&env, &base).initialize(&manager);

    // Share token for the vault.
    let share_id = env.register(
        ShareTokenContract,
        (
            manager.clone(),
            String::from_str(&env, "Blend Vault Share"),
            String::from_str(&env, "BVS"),
            7u32,
        ),
    );

    // Deploy vault.
    let vault_id = env.register(
        Vault,
        (VaultParams {
            admin: manager.clone(),
            manager: manager.clone(),
            manager_name: None,
            trader: trader.clone(),
            base_asset: base.clone(),
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

    // Deploy MockBlendPool.
    let pool_id = env.register(MockBlendPool, ());
    MockBlendPoolClient::new(&env, &pool_id).blend_init(&base);

    // Deploy BlendStrategy.
    let strategy_id = env.register(BlendStrategy, ());
    BlendStrategyClient::new(&env, &strategy_id).initialize(
        &vault_id,
        &base,
        &pool_id,
        &String::from_str(&env, "Blend USDC"),
    );

    // Whitelist base in portfolio and as a deposit asset so vault NAV includes
    // idle cash.  Guards must also be registered so their positions count.
    vault.add_portfolio_asset(&manager, &base);
    vault.add_deposit_asset(&manager, &base);

    // Register strategy as active guard and authorize named ops.
    vault.add_active_guard(&manager, &strategy_id);
    let ops: Vec<Symbol> = soroban_sdk::vec![
        &env,
        Symbol::new(&env, "supply"),
        Symbol::new(&env, "withdraw_from_lending"),
    ];
    vault.set_authorized_ops(&manager, &strategy_id, &ops);

    // Fund user.
    MockTokenClient::new(&env, &base).mint(&user, &10_000_0000000i128);

    let vault: VaultClient<'static> = unsafe { core::mem::transmute(vault) };
    let strategy: BlendStrategyClient<'static> =
        unsafe { core::mem::transmute(BlendStrategyClient::new(&env, &strategy_id)) };
    let blend_pool: MockBlendPoolClient<'static> =
        unsafe { core::mem::transmute(MockBlendPoolClient::new(&env, &pool_id)) };

    BlendWorld {
        env,
        vault,
        vault_addr: vault_id,
        strategy,
        strategy_addr: strategy_id,
        blend_pool,
        share_token_addr: share_id,
        base,
        manager,
        trader,
        user,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// supply via execute_op: tokens move from vault → strategy → Blend pool.
#[test]
fn test_blend_supply_via_execute_op() {
    let w = setup_blend();
    let deposit = 1_000_0000000i128;
    let supply_amount = 500_0000000i128;

    w.vault.deposit(&deposit, &w.user, &w.base, &0i128);
    assert_eq!(token_balance(&w.env, &w.base, &w.vault_addr), deposit);

    let args: Vec<Val> = soroban_sdk::vec![&w.env, supply_amount.into_val(&w.env)];
    w.vault.execute_op(
        &w.trader,
        &w.strategy_addr,
        &Symbol::new(&w.env, "supply"),
        &args,
    );

    // Blend pool records strategy's position.
    assert_eq!(w.blend_pool.get_supply(&w.strategy_addr), supply_amount);
    // Vault's idle balance reduced by the supplied amount.
    assert_eq!(
        token_balance(&w.env, &w.base, &w.vault_addr),
        deposit - supply_amount
    );
    // Strategy holds no base asset (it all went to Blend).
    assert_eq!(token_balance(&w.env, &w.base, &w.strategy_addr), 0);
}

/// withdraw_from_lending via execute_op: Blend sends tokens directly to vault.
#[test]
fn test_blend_withdraw_from_lending_via_execute_op() {
    let w = setup_blend();
    let deposit = 1_000_0000000i128;
    let supply_amount = 500_0000000i128;

    w.vault.deposit(&deposit, &w.user, &w.base, &0i128);
    let supply_args: Vec<Val> = soroban_sdk::vec![&w.env, supply_amount.into_val(&w.env)];
    w.vault.execute_op(
        &w.trader,
        &w.strategy_addr,
        &Symbol::new(&w.env, "supply"),
        &supply_args,
    );

    assert_eq!(w.blend_pool.get_supply(&w.strategy_addr), supply_amount);

    let withdraw_args: Vec<Val> = soroban_sdk::vec![&w.env, supply_amount.into_val(&w.env)];
    w.vault.execute_op(
        &w.trader,
        &w.strategy_addr,
        &Symbol::new(&w.env, "withdraw_from_lending"),
        &withdraw_args,
    );

    // Blend position is cleared.
    assert_eq!(w.blend_pool.get_supply(&w.strategy_addr), 0);
    // Tokens returned directly from Blend to vault (MockBlendPool mints on withdraw).
    assert_eq!(token_balance(&w.env, &w.base, &w.vault_addr), deposit);
}

/// get_total_value returns the live Blend position for the correct vault.
#[test]
fn test_blend_get_total_value_reflects_position() {
    let w = setup_blend();
    let deposit = 1_000_0000000i128;
    let supply_amount = 700_0000000i128;

    // No position yet → 0.
    assert_eq!(w.strategy.get_total_value(&w.vault_addr), 0);

    w.vault.deposit(&deposit, &w.user, &w.base, &0i128);
    let args: Vec<Val> = soroban_sdk::vec![&w.env, supply_amount.into_val(&w.env)];
    w.vault.execute_op(
        &w.trader,
        &w.strategy_addr,
        &Symbol::new(&w.env, "supply"),
        &args,
    );

    assert_eq!(w.strategy.get_total_value(&w.vault_addr), supply_amount);

    // Wrong vault address → 0.
    let stranger = Address::generate(&w.env);
    assert_eq!(w.strategy.get_total_value(&stranger), 0);
}

/// asset_in_use returns true for the managed asset while position is non-zero.
#[test]
fn test_blend_asset_in_use() {
    let w = setup_blend();
    let deposit = 1_000_0000000i128;
    let supply_amount = 200_0000000i128;

    assert!(!w.strategy.asset_in_use(&w.vault_addr, &w.base));

    w.vault.deposit(&deposit, &w.user, &w.base, &0i128);
    let args: Vec<Val> = soroban_sdk::vec![&w.env, supply_amount.into_val(&w.env)];
    w.vault.execute_op(
        &w.trader,
        &w.strategy_addr,
        &Symbol::new(&w.env, "supply"),
        &args,
    );

    assert!(w.strategy.asset_in_use(&w.vault_addr, &w.base));
}

/// Proportional withdrawal: vault.withdraw calls withdraw_fraction on the
/// Blend strategy, which sends Blend-held tokens directly to the user.
#[test]
fn test_blend_proportional_withdrawal_with_active_guard() {
    let w = setup_blend();
    let deposit = 1_000_0000000i128;
    let supply_amount = 500_0000000i128;

    w.vault.deposit(&deposit, &w.user, &w.base, &0i128);
    let supply_args: Vec<Val> = soroban_sdk::vec![&w.env, supply_amount.into_val(&w.env)];
    w.vault.execute_op(
        &w.trader,
        &w.strategy_addr,
        &Symbol::new(&w.env, "supply"),
        &supply_args,
    );

    // State: vault has 500 idle, Blend has 500 for strategy.
    let share_client = ShareTokenContractClient::new(&w.env, &w.share_token_addr);
    let user_shares = share_client.balance(&w.user);
    assert_eq!(user_shares, deposit); // bootstrap 1:1, no fees

    let user_base_before = token_balance(&w.env, &w.base, &w.user);

    // Withdraw all shares: vault sends proportional base (500) + Blend strategy
    // sends its proportional position (500) directly to user via withdraw_fraction.
    w.vault.withdraw(&user_shares, &w.user, &w.user, &0i128);

    let user_base_after = token_balance(&w.env, &w.base, &w.user);
    // User receives the full deposit back.
    assert_eq!(user_base_after - user_base_before, deposit);
    assert_eq!(share_client.balance(&w.user), 0);
    assert_eq!(w.blend_pool.get_supply(&w.strategy_addr), 0);
}

/// execute_op rejects a function name that is not in the authorized ops list.
#[test]
#[should_panic]
fn test_blend_execute_op_rejects_unauthorized_fn() {
    let w = setup_blend();
    let args: Vec<Val> = soroban_sdk::vec![&w.env, 100i128.into_val(&w.env)];
    w.vault.execute_op(
        &w.trader,
        &w.strategy_addr,
        &Symbol::new(&w.env, "deposit"),
        &args,
    );
}

/// execute_op rejects callers that are neither manager nor trader.
#[test]
#[should_panic]
fn test_blend_execute_op_rejects_non_trader_caller() {
    let w = setup_blend();
    let stranger = Address::generate(&w.env);
    let args: Vec<Val> = soroban_sdk::vec![&w.env, 100i128.into_val(&w.env)];
    w.vault.execute_op(
        &stranger,
        &w.strategy_addr,
        &Symbol::new(&w.env, "supply"),
        &args,
    );
}
