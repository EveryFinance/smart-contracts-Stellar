//! Integration tests: Vault ↔ Soroswap LP strategy (execute_op API)

#![cfg(test)]

use share_token::ShareTokenContract;
use soroban_sdk::{
    contract, contractimpl, contracttype, testutils::Address as _, Address, Env, IntoVal, Map,
    String, Symbol, Val, Vec,
};
use soroswap_lp_strategy::{SoroswapLpStrategy, SoroswapLpStrategyClient};
use vault::{Vault, VaultClient, VaultParams};

use crate::common::{
    token_balance, MockSoroswapRouter, MockSoroswapRouterClient, MockToken, MockTokenClient,
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

#[contracttype]
enum PairKey {
    Balance(Address),
    Allowance(Address, Address),
    TotalSupply,
    Token0,
    Token1,
    Reserve0,
    Reserve1,
}

#[contract]
struct MockSoroswapPair;

#[contractimpl]
impl MockSoroswapPair {
    pub fn pair_init(env: Env, token0: Address, token1: Address, reserve0: i128, reserve1: i128) {
        env.storage().instance().set(&PairKey::Token0, &token0);
        env.storage().instance().set(&PairKey::Token1, &token1);
        env.storage().instance().set(&PairKey::Reserve0, &reserve0);
        env.storage().instance().set(&PairKey::Reserve1, &reserve1);
    }

    pub fn mint(env: Env, to: Address, amount: i128) {
        let bal: i128 = env
            .storage()
            .persistent()
            .get(&PairKey::Balance(to.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&PairKey::Balance(to), &(bal + amount));
        let supply: i128 = env
            .storage()
            .persistent()
            .get(&PairKey::TotalSupply)
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&PairKey::TotalSupply, &(supply + amount));
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&PairKey::Balance(id))
            .unwrap_or(0)
    }

    pub fn total_supply(env: Env) -> i128 {
        env.storage()
            .persistent()
            .get(&PairKey::TotalSupply)
            .unwrap_or(0)
    }

    pub fn approve(env: Env, from: Address, spender: Address, amount: i128, _expiry: u32) {
        from.require_auth();
        env.storage()
            .persistent()
            .set(&PairKey::Allowance(from, spender), &amount);
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        let from_bal = Self::balance(env.clone(), from.clone());
        assert!(from_bal >= amount, "pair transfer: insufficient balance");
        env.storage()
            .persistent()
            .set(&PairKey::Balance(from), &(from_bal - amount));
        let to_bal = Self::balance(env.clone(), to.clone());
        env.storage()
            .persistent()
            .set(&PairKey::Balance(to), &(to_bal + amount));
    }

    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        spender.require_auth();
        let allowance: i128 = env
            .storage()
            .persistent()
            .get(&PairKey::Allowance(from.clone(), spender.clone()))
            .unwrap_or(0);
        assert!(
            allowance >= amount,
            "pair transfer_from: insufficient allowance"
        );
        env.storage().persistent().set(
            &PairKey::Allowance(from.clone(), spender),
            &(allowance - amount),
        );
        let from_bal = Self::balance(env.clone(), from.clone());
        assert!(
            from_bal >= amount,
            "pair transfer_from: insufficient balance"
        );
        env.storage()
            .persistent()
            .set(&PairKey::Balance(from), &(from_bal - amount));
        let to_bal = Self::balance(env.clone(), to.clone());
        env.storage()
            .persistent()
            .set(&PairKey::Balance(to), &(to_bal + amount));
    }

    pub fn get_reserves(env: Env) -> (i128, i128) {
        let reserve0 = env
            .storage()
            .instance()
            .get(&PairKey::Reserve0)
            .unwrap_or(0);
        let reserve1 = env
            .storage()
            .instance()
            .get(&PairKey::Reserve1)
            .unwrap_or(0);
        (reserve0, reserve1)
    }

    pub fn token0(env: Env) -> Address {
        env.storage().instance().get(&PairKey::Token0).unwrap()
    }

    pub fn token1(env: Env) -> Address {
        env.storage().instance().get(&PairKey::Token1).unwrap()
    }
}

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

    let lp_token = env.register(MockSoroswapPair, ());
    MockSoroswapPairClient::new(&env, &lp_token).pair_init(
        &asset_a,
        &asset_b,
        &50_000_0000000i128,
        &50_000_0000000i128,
    );

    let asset_handler = env.register(MockLpAssetHandler, ());
    let factory_id = env.register(MockLpFactory, ());
    MockLpFactoryClient::new(&env, &factory_id).init(&asset_handler);
    MockLpFactoryClient::new(&env, &factory_id).authorize_asset(&asset_a);
    MockLpFactoryClient::new(&env, &factory_id).authorize_asset(&asset_b);

    // Use asset_a as vault base (simplest setup — no oracle needed for basic ops).
    let vault_id = Address::generate(&env);
    let share_id = env.register(
        ShareTokenContract,
        (
            vault_id.clone(),
            String::from_str(&env, "SS Vault Share"),
            String::from_str(&env, "SVS"),
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
            share_token: share_id.clone(),
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

    // MockSoroswapRouter — mints LP tokens on add_liquidity, mints assets on remove.
    let router_id = env.register(MockSoroswapRouter, ());
    MockSoroswapRouterClient::new(&env, &router_id).router_init(&lp_token);

    // SoroswapLpStrategy.
    let strategy_id = env.register(SoroswapLpStrategy, ());
    SoroswapLpStrategyClient::new(&env, &strategy_id).initialize(
        &vault_id,
        &router_id,
        &String::from_str(&env, "Soroswap USDC-XLM"),
    );
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

    let args: Vec<Val> = soroban_sdk::vec![
        &w.env,
        w.lp_token.clone().into_val(&w.env), // lp_token (pair address)
        w.asset_a.clone().into_val(&w.env),  // asset_a
        w.asset_b.clone().into_val(&w.env),  // asset_b
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
    assert_eq!(
        token_balance(&w.env, &w.lp_token, &w.strategy_addr),
        lp_minted
    );

    // Strategy's tracked LP balance matches.
    assert_eq!(w.strategy.get_lp_balance(&w.lp_token), lp_minted);

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
    let add_args: Vec<Val> = soroban_sdk::vec![
        &w.env,
        w.lp_token.clone().into_val(&w.env),
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

    let lp_minted = amount_a.min(amount_b);
    let vault_a_before = token_balance(&w.env, &w.asset_a, &w.vault_addr);
    let vault_b_before = token_balance(&w.env, &w.asset_b, &w.vault_addr);

    // Remove all LP tokens.
    let remove_args: Vec<Val> = soroban_sdk::vec![
        &w.env,
        w.lp_token.clone().into_val(&w.env),
        w.asset_a.clone().into_val(&w.env),
        w.asset_b.clone().into_val(&w.env),
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
    assert_eq!(w.strategy.get_lp_balance(&w.lp_token), 0);
    // Note: the mock router does not burn LP token on-chain (it lacks caller identity);
    // real Soroswap uses an allowance-based pull from the strategy to the pair.
    // The tracked balance above is the authoritative measure for NAV.
}

#[test]
fn test_soroswap_withdraw_fraction_sends_underlyings_to_user() {
    let w = setup_soroswap();
    let amount_a = 2_000_0000000i128;
    let amount_b = 2_000_0000000i128;

    let add_args: Vec<Val> = soroban_sdk::vec![
        &w.env,
        w.lp_token.clone().into_val(&w.env),
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

    let user_a_before = token_balance(&w.env, &w.asset_a, &w.user);
    let user_b_before = token_balance(&w.env, &w.asset_b, &w.user);

    w.strategy
        .withdraw_fraction(&w.vault_addr, &1i128, &2i128, &w.user);

    let expected_each = (amount_a.min(amount_b) / 2) / 2;
    assert_eq!(
        token_balance(&w.env, &w.asset_a, &w.user),
        user_a_before + expected_each
    );
    assert_eq!(
        token_balance(&w.env, &w.asset_b, &w.user),
        user_b_before + expected_each
    );
    assert_eq!(
        w.strategy.get_lp_balance(&w.lp_token),
        amount_a.min(amount_b) / 2
    );
}

#[test]
fn test_soroswap_wrong_vault_views_return_zero_or_false() {
    let w = setup_soroswap();
    let stranger = Address::generate(&w.env);
    assert_eq!(w.strategy.get_total_value(&stranger), 0);
    assert!(!w.strategy.asset_in_use(&stranger, &w.asset_a));
}

/// swap via execute_op: vault's from_asset → strategy → router → to_asset
/// minted directly to vault.
#[test]
fn test_soroswap_swap_via_execute_op() {
    let w = setup_soroswap();
    let amount_in = 500_0000000i128;

    let vault_a_before = token_balance(&w.env, &w.asset_a, &w.vault_addr);
    let vault_b_before = token_balance(&w.env, &w.asset_b, &w.vault_addr);

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

#[test]
#[should_panic(expected = "Error(Contract, #31)")]
fn test_soroswap_swap_rejects_non_pair_asset_via_execute_op() {
    let w = setup_soroswap();
    // Use a deployed token contract that is NOT in the vault's portfolio.
    // A raw random address has no contract instance, which fails the pre-op balance check.
    let rogue_asset = w.env.register(MockToken, ());
    MockTokenClient::new(&w.env, &rogue_asset).initialize(&w.manager);
    let amount_in = 500_0000000i128;

    let args: Vec<Val> = soroban_sdk::vec![
        &w.env,
        rogue_asset.into_val(&w.env),
        w.asset_b.clone().into_val(&w.env),
        amount_in.into_val(&w.env),
        0i128.into_val(&w.env),
    ];
    w.vault.execute_op(
        &w.trader,
        &w.strategy_addr,
        &Symbol::new(&w.env, "swap"),
        &args,
    );
}

/// get_total_value returns 0 before any LP position exists.
#[test]
fn test_soroswap_get_total_value_without_position_returns_zero() {
    let w = setup_soroswap();
    assert_eq!(w.strategy.get_total_value(&w.vault_addr), 0);
}

/// asset_in_use returns true after add_liquidity for both underlying assets.
#[test]
fn test_soroswap_asset_in_use_after_add_liquidity() {
    let w = setup_soroswap();
    let amount = 1_000_0000000i128;

    assert!(!w.strategy.asset_in_use(&w.vault_addr, &w.asset_a));
    assert!(!w.strategy.asset_in_use(&w.vault_addr, &w.asset_b));

    let args: Vec<Val> = soroban_sdk::vec![
        &w.env,
        w.lp_token.clone().into_val(&w.env),
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
