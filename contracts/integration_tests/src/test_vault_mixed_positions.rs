//! Integration tests: vault deposit/withdraw across idle assets, lending,
//! LP positions, fees, and AssetHandler oracle edge cases.

#![cfg(test)]

use asset_handler::{AssetHandler, AssetHandlerClient};
use blend_strategy::{BlendStrategy, BlendStrategyClient};
use phoenix_lp_strategy::{PhoenixLpStrategy, PhoenixLpStrategyClient};
use share_token::{ShareTokenContract, ShareTokenContractClient};
use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger},
    Address, Env, IntoVal, String, Symbol, Val, Vec,
};
use vault::{Vault, VaultClient, VaultParams};

use crate::common::{
    token_balance, MockBlendPool, MockBlendPoolClient, MockPhoenixPool, MockPhoenixPoolClient,
    MockToken, MockTokenClient,
};

const PRICE_PRECISION: i128 = 10_000_000;
const SECONDS_PER_YEAR: u64 = 31_536_000;

// ---------------------------------------------------------------------------
// Local mocks
// ---------------------------------------------------------------------------

#[contracttype]
enum MixedFactoryKey {
    AssetHandler,
    Asset(Address),
    Guard(Address),
}

#[contract]
struct MixedFactory;

#[contractimpl]
impl MixedFactory {
    pub fn init(env: Env, asset_handler: Address) {
        env.storage()
            .instance()
            .set(&MixedFactoryKey::AssetHandler, &asset_handler);
    }

    pub fn authorize_asset(env: Env, asset: Address) {
        env.storage()
            .instance()
            .set(&MixedFactoryKey::Asset(asset), &true);
    }

    pub fn is_authorized_asset(env: Env, asset: Address) -> bool {
        env.storage()
            .instance()
            .get(&MixedFactoryKey::Asset(asset))
            .unwrap_or(false)
    }

    pub fn authorize_guard(env: Env, guard: Address) {
        env.storage()
            .instance()
            .set(&MixedFactoryKey::Guard(guard), &true);
    }

    pub fn is_authorized_guard(env: Env, guard: Address) -> bool {
        env.storage()
            .instance()
            .get(&MixedFactoryKey::Guard(guard))
            .unwrap_or(false)
    }

    pub fn get_asset_handler(env: Env) -> Option<Address> {
        env.storage().instance().get(&MixedFactoryKey::AssetHandler)
    }
}

mod fixed_oracle {
    use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};

    #[contract]
    pub struct FixedOracle;

    #[contractimpl]
    impl FixedOracle {
        pub fn __constructor(env: Env, price: i128) {
            env.storage()
                .instance()
                .set(&Symbol::new(&env, "price"), &price);
        }

        pub fn get_price(env: Env, _asset: Address) -> i128 {
            env.storage()
                .instance()
                .get(&Symbol::new(&env, "price"))
                .unwrap_or(0)
        }
    }
}

mod zero_oracle {
    use soroban_sdk::{contract, contractimpl, Address, Env};

    #[contract]
    pub struct ZeroOracle;

    #[contractimpl]
    impl ZeroOracle {
        pub fn get_price(_env: Env, _asset: Address) -> i128 {
            0
        }
    }
}

mod panic_oracle {
    use asset_handler::AssetHandlerError;
    use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env};

    #[contract]
    pub struct PanicOracle;

    #[contractimpl]
    impl PanicOracle {
        pub fn get_price(env: Env, _asset: Address) -> i128 {
            panic_with_error!(&env, AssetHandlerError::PriceNotAvailable)
        }
    }
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

struct MixedWorld {
    env: Env,
    vault: VaultClient<'static>,
    vault_addr: Address,
    blend_strategy: Address,
    blend_pool: MockBlendPoolClient<'static>,
    blend_pool_addr: Address,
    phoenix_strategy: Address,
    phoenix_pool_addr: Address,
    phoenix_share: Address,
    base: Address,
    token_b: Address,
    trader: Address,
    user: Address,
    vault_share: Address,
}

struct FeeWorld {
    env: Env,
    vault: VaultClient<'static>,
    vault_addr: Address,
    base: Address,
    manager: Address,
    user: Address,
    vault_share: Address,
}

fn fixed_oracle(env: &Env, price: i128) -> Address {
    env.register(fixed_oracle::FixedOracle, (price,))
}

fn zero_oracle(env: &Env) -> Address {
    env.register(zero_oracle::ZeroOracle, ())
}

fn panic_oracle(env: &Env) -> Address {
    env.register(panic_oracle::PanicOracle, ())
}

fn setup_asset_handler(
    env: &Env,
    manager: &Address,
    assets: &[Address],
) -> (Address, AssetHandlerClient<'static>) {
    let asset_handler_id = env.register(AssetHandler, (manager.clone(),));
    let asset_handler: AssetHandlerClient<'static> =
        unsafe { core::mem::transmute(AssetHandlerClient::new(env, &asset_handler_id)) };
    let oracle = fixed_oracle(env, PRICE_PRECISION);

    for asset in assets {
        asset_handler.add_asset(manager, asset);
    }
    asset_handler.set_primary_oracle(manager, &oracle);

    (asset_handler_id, asset_handler)
}

fn setup_mixed_world() -> MixedWorld {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let manager = Address::generate(&env);
    let trader = Address::generate(&env);
    let user = Address::generate(&env);

    let base = env.register(MockToken, ());
    MockTokenClient::new(&env, &base).initialize(&manager);
    let token_b = env.register(MockToken, ());
    MockTokenClient::new(&env, &token_b).initialize(&manager);
    let phoenix_share = env.register(MockToken, ());
    MockTokenClient::new(&env, &phoenix_share).initialize(&manager);

    let (asset_handler_id, _) =
        setup_asset_handler(&env, &manager, &[base.clone(), token_b.clone()]);
    let factory_id = env.register(MixedFactory, ());
    let factory = MixedFactoryClient::new(&env, &factory_id);
    factory.init(&asset_handler_id);
    factory.authorize_asset(&base);
    factory.authorize_asset(&token_b);

    let vault_id = Address::generate(&env);
    let vault_share = env.register(
        ShareTokenContract,
        (
            vault_id.clone(),
            String::from_str(&env, "Mixed Vault Share"),
            String::from_str(&env, "MVS"),
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
            base_asset: base.clone(),
            share_token: vault_share.clone(),
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

    let blend_pool_id = env.register(MockBlendPool, ());
    MockBlendPoolClient::new(&env, &blend_pool_id).blend_init(&base);
    let blend_strategy_id = env.register(BlendStrategy, ());
    BlendStrategyClient::new(&env, &blend_strategy_id)
        .initialize(&vault_id, &String::from_str(&env, "Blend Base"));

    let phoenix_pool_id = env.register(MockPhoenixPool, ());
    MockPhoenixPoolClient::new(&env, &phoenix_pool_id).phoenix_init(
        &phoenix_share,
        &base,
        &token_b,
    );
    let phoenix_strategy_id = env.register(PhoenixLpStrategy, ());
    PhoenixLpStrategyClient::new(&env, &phoenix_strategy_id)
        .initialize(&vault_id, &String::from_str(&env, "Phoenix Base-B"));

    factory.authorize_guard(&blend_strategy_id);
    factory.authorize_guard(&phoenix_strategy_id);

    vault.add_portfolio_asset(&manager, &base);
    vault.add_portfolio_asset(&manager, &token_b);
    vault.add_deposit_asset(&manager, &base);
    vault.add_deposit_asset(&manager, &token_b);
    vault.add_active_guard(&manager, &blend_strategy_id);
    vault.add_active_guard(&manager, &phoenix_strategy_id);

    vault.set_authorized_ops(
        &manager,
        &blend_strategy_id,
        &soroban_sdk::vec![
            &env,
            Symbol::new(&env, "supply"),
            Symbol::new(&env, "withdraw_from_lending"),
        ],
    );
    vault.set_authorized_ops(
        &manager,
        &phoenix_strategy_id,
        &soroban_sdk::vec![
            &env,
            Symbol::new(&env, "add_liquidity"),
            Symbol::new(&env, "remove_liquidity"),
        ],
    );
    vault.set_max_loss_bps(&manager, &9_999u32);

    MockTokenClient::new(&env, &base).mint(&user, &10_000_0000000i128);
    MockTokenClient::new(&env, &token_b).mint(&user, &10_000_0000000i128);

    let vault: VaultClient<'static> = unsafe { core::mem::transmute(vault) };
    let blend_pool: MockBlendPoolClient<'static> =
        unsafe { core::mem::transmute(MockBlendPoolClient::new(&env, &blend_pool_id)) };

    MixedWorld {
        env,
        vault,
        vault_addr: vault_id,
        blend_strategy: blend_strategy_id,
        blend_pool,
        blend_pool_addr: blend_pool_id,
        phoenix_strategy: phoenix_strategy_id,
        phoenix_pool_addr: phoenix_pool_id,
        phoenix_share,
        base,
        token_b,
        trader,
        user,
        vault_share,
    }
}

fn setup_fee_vault(
    entry_fee_bps: u32,
    exit_fee_bps: u32,
    mgmt_fee_bps: u32,
    perf_fee_bps: u32,
) -> FeeWorld {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let manager = Address::generate(&env);
    let trader = Address::generate(&env);
    let user = Address::generate(&env);
    let base = env.register(MockToken, ());
    MockTokenClient::new(&env, &base).initialize(&manager);

    let vault_id = Address::generate(&env);
    let vault_share = env.register(
        ShareTokenContract,
        (
            vault_id.clone(),
            String::from_str(&env, "Fee Vault Share"),
            String::from_str(&env, "FVS"),
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
            base_asset: base.clone(),
            share_token: vault_share.clone(),
            share_token_admin: vault_id.clone(),
            treasury: manager.clone(),
            entry_fee_bps,
            exit_fee_bps,
            mgmt_fee_bps,
            perf_fee_bps,
            factory: None,
            is_private: false,
        },),
    );
    let vault = VaultClient::new(&env, &vault_id);
    MockTokenClient::new(&env, &base).mint(&user, &10_000_0000000i128);

    let vault: VaultClient<'static> = unsafe { core::mem::transmute(vault) };

    FeeWorld {
        env,
        vault,
        vault_addr: vault_id,
        base,
        manager,
        user,
        vault_share,
    }
}

fn advance_time(env: &Env, secs: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp = li.timestamp.saturating_add(secs);
        li.sequence_number = li.sequence_number.saturating_add(1);
    });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn deposit_and_withdraw_user_fraction_across_idle_lending_and_lp_positions() {
    let w = setup_mixed_world();

    w.vault
        .deposit(&2_000_0000000i128, &w.user, &w.base, &2_000_0000000i128);
    w.vault
        .deposit(&1_000_0000000i128, &w.user, &w.token_b, &1_000_0000000i128);

    assert_eq!(
        ShareTokenContractClient::new(&w.env, &w.vault_share).total_supply(),
        3_000_0000000i128
    );

    let supply_args: Vec<Val> = soroban_sdk::vec![
        &w.env,
        w.blend_pool_addr.clone().into_val(&w.env),
        w.base.clone().into_val(&w.env),
        400_0000000i128.into_val(&w.env),
    ];
    w.vault.execute_op(
        &w.trader,
        &w.blend_strategy,
        &Symbol::new(&w.env, "supply"),
        &supply_args,
    );

    let lp_args: Vec<Val> = soroban_sdk::vec![
        &w.env,
        w.phoenix_pool_addr.clone().into_val(&w.env),
        w.base.clone().into_val(&w.env),
        w.token_b.clone().into_val(&w.env),
        600_0000000i128.into_val(&w.env),
        600_0000000i128.into_val(&w.env),
        0i128.into_val(&w.env),
        0i128.into_val(&w.env),
    ];
    w.vault.execute_op(
        &w.trader,
        &w.phoenix_strategy,
        &Symbol::new(&w.env, "add_liquidity"),
        &lp_args,
    );

    assert_eq!(token_balance(&w.env, &w.base, &w.vault_addr), 1_000_0000000);
    assert_eq!(
        token_balance(&w.env, &w.token_b, &w.vault_addr),
        400_0000000
    );
    assert_eq!(w.blend_pool.get_supply(&w.blend_strategy), 400_0000000i128);
    assert_eq!(
        token_balance(&w.env, &w.phoenix_share, &w.phoenix_strategy),
        600_0000000i128
    );
    assert_eq!(w.vault.get_nav(), 3_000_0000000i128);

    let user_base_before = token_balance(&w.env, &w.base, &w.user);
    let user_token_b_before = token_balance(&w.env, &w.token_b, &w.user);

    let withdrawn_value = w
        .vault
        .withdraw(&1_500_0000000i128, &w.user, &w.user, &0i128);

    assert_eq!(withdrawn_value, 1_500_0000000i128);
    assert_eq!(
        token_balance(&w.env, &w.base, &w.user) - user_base_before,
        1_000_0000000i128
    );
    assert_eq!(
        token_balance(&w.env, &w.token_b, &w.user) - user_token_b_before,
        500_0000000i128
    );

    assert_eq!(token_balance(&w.env, &w.base, &w.vault_addr), 500_0000000);
    assert_eq!(
        token_balance(&w.env, &w.token_b, &w.vault_addr),
        200_0000000
    );
    assert_eq!(w.blend_pool.get_supply(&w.blend_strategy), 200_0000000i128);
    assert_eq!(
        token_balance(&w.env, &w.phoenix_share, &w.phoenix_strategy),
        300_0000000i128
    );
    assert_eq!(w.vault.get_nav(), 1_500_0000000i128);
}

#[test]
fn entry_and_exit_fees_are_applied_to_user_deposit_and_withdraw() {
    let w = setup_fee_vault(100, 200, 0, 0);
    let shares = ShareTokenContractClient::new(&w.env, &w.vault_share);

    let minted = w
        .vault
        .deposit(&1_000_0000000i128, &w.user, &w.base, &0i128);
    assert_eq!(minted, 990_0000000i128);
    assert_eq!(shares.balance(&w.user), 990_0000000i128);
    assert_eq!(shares.balance(&w.manager), 10_0000000i128);
    assert_eq!(shares.total_supply(), 1_000_0000000i128);

    let user_before = token_balance(&w.env, &w.base, &w.user);
    let withdrawn = w.vault.withdraw(&990_0000000i128, &w.user, &w.user, &0i128);

    // 2% exit fee on 990 shares → fee_shares=19.8, net_shares=970.2
    // user receives 970.2 / 1000 × 1000 USDC = 970.2 USDC
    assert_eq!(withdrawn, 970_2000000i128);
    assert_eq!(
        token_balance(&w.env, &w.base, &w.user) - user_before,
        970_2000000i128
    );
    // vault retains the 29.8 USDC backed by treasury's fee shares
    assert_eq!(
        token_balance(&w.env, &w.base, &w.vault_addr),
        29_8000000i128
    );
    assert_eq!(shares.balance(&w.user), 0);
    // treasury holds entry fee shares (10) + exit fee shares (19.8) = 29.8
    assert_eq!(shares.balance(&w.manager), 29_8000000i128);
}

#[test]
fn performance_fee_mints_treasury_shares_before_withdraw_on_nav_growth() {
    let w = setup_fee_vault(0, 0, 0, 1_000);
    let shares = ShareTokenContractClient::new(&w.env, &w.vault_share);

    w.vault
        .deposit(&1_000_0000000i128, &w.user, &w.base, &0i128);
    MockTokenClient::new(&w.env, &w.base).mint(&w.vault_addr, &200_0000000i128);

    let user_before = token_balance(&w.env, &w.base, &w.user);
    let withdrawn = w
        .vault
        .withdraw(&1_000_0000000i128, &w.user, &w.user, &0i128);

    assert!(shares.balance(&w.manager) > 0);
    assert_eq!(shares.balance(&w.user), 0);
    assert!(withdrawn > 1_000_0000000i128);
    assert!(withdrawn < 1_200_0000000i128);
    assert_eq!(
        token_balance(&w.env, &w.base, &w.user) - user_before,
        withdrawn
    );
    assert!(token_balance(&w.env, &w.base, &w.vault_addr) > 0);
}

#[test]
fn collect_pending_fees_mints_accrued_management_fee_without_user_action() {
    let w = setup_fee_vault(0, 0, 200, 0);
    let shares = ShareTokenContractClient::new(&w.env, &w.vault_share);

    w.vault
        .deposit(&1_000_0000000i128, &w.user, &w.base, &0i128);
    advance_time(&w.env, SECONDS_PER_YEAR / 2);

    let minted = w.vault.collect_pending_fees();

    assert_eq!(minted, 10_0000000i128);
    assert_eq!(shares.balance(&w.manager), 10_0000000i128);
    assert_eq!(shares.balance(&w.user), 1_000_0000000i128);
    assert_eq!(shares.total_supply(), 1_010_0000000i128);
    assert_eq!(
        token_balance(&w.env, &w.base, &w.vault_addr),
        1_000_0000000i128
    );
    assert_eq!(w.vault.collect_pending_fees(), 0);
}

#[test]
fn withdraw_lazily_collects_management_fee_before_user_accounting() {
    let w = setup_fee_vault(0, 0, 200, 0);
    let shares = ShareTokenContractClient::new(&w.env, &w.vault_share);

    w.vault
        .deposit(&1_000_0000000i128, &w.user, &w.base, &0i128);
    advance_time(&w.env, SECONDS_PER_YEAR / 2);

    let user_before = token_balance(&w.env, &w.base, &w.user);
    let withdrawn = w
        .vault
        .withdraw(&1_000_0000000i128, &w.user, &w.user, &0i128);

    assert_eq!(shares.balance(&w.manager), 10_0000000i128);
    assert_eq!(shares.balance(&w.user), 0);
    assert_eq!(shares.total_supply(), 10_0000000i128);
    assert_eq!(withdrawn, 990_0990000i128);
    assert_eq!(
        token_balance(&w.env, &w.base, &w.user) - user_before,
        990_0990000i128
    );
    assert_eq!(token_balance(&w.env, &w.base, &w.vault_addr), 9_9010000i128);
}

#[test]
fn asset_handler_uses_per_asset_primary_and_fallback_oracles_in_order() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let manager = Address::generate(&env);
    let asset = Address::generate(&env);

    let (_, handler) = setup_asset_handler(&env, &manager, &[asset.clone()]);

    let per_asset = fixed_oracle(&env, 5 * PRICE_PRECISION);
    handler.set_asset_oracle(&manager, &asset, &per_asset);
    assert_eq!(handler.get_price(&asset), 5 * PRICE_PRECISION);

    let zero = zero_oracle(&env);
    let primary = fixed_oracle(&env, 2 * PRICE_PRECISION);
    handler.set_asset_oracle(&manager, &asset, &zero);
    handler.set_primary_oracle(&manager, &primary);
    assert_eq!(handler.get_price(&asset), 2 * PRICE_PRECISION);

    let panicking = panic_oracle(&env);
    handler.set_asset_oracle(&manager, &asset, &panicking);
    assert_eq!(handler.get_price(&asset), 2 * PRICE_PRECISION);

    let primary_zero = zero_oracle(&env);
    let fallback = fixed_oracle(&env, 3 * PRICE_PRECISION);
    handler.set_asset_oracle(&manager, &asset, &zero);
    handler.set_primary_oracle(&manager, &primary_zero);
    handler.set_fallback_oracle(&manager, &fallback);
    assert_eq!(handler.get_price(&asset), 3 * PRICE_PRECISION);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn asset_handler_reverts_when_all_configured_oracles_are_unavailable() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let manager = Address::generate(&env);
    let asset = Address::generate(&env);

    let (_, handler) = setup_asset_handler(&env, &manager, &[asset.clone()]);
    let zero = zero_oracle(&env);
    handler.set_primary_oracle(&manager, &zero);
    handler.set_fallback_oracle(&manager, &zero);

    handler.get_price(&asset);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn asset_handler_reverts_for_unregistered_asset() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let manager = Address::generate(&env);
    let registered = Address::generate(&env);
    let unregistered = Address::generate(&env);

    let (_, handler) = setup_asset_handler(&env, &manager, &[registered]);
    handler.get_price(&unregistered);
}
