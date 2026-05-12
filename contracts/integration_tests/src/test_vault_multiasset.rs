//! Integration tests: multi-asset vault (portfolio assets + active guards + PnL).
//!
//! Verifies the full dHedge-V2-style lifecycle:
//!   1. Manager sets up portfolio assets and a mock factory/oracle.
//!   2. User deposits with the base asset.
//!   3. NAV includes both base and non-base portfolio balances.
//!   4. Proportional withdrawal delivers a fraction of every portfolio asset.
//!   5. PnL tracking records cost_basis and realized_pnl correctly.
//!   6. Active guard `get_total_value` contributes to NAV; `withdraw_fraction`
//!      is called on proportional withdrawal.

#![cfg(test)]

use share_token::{ShareTokenContract, ShareTokenContractClient};
use soroban_sdk::{
    contract, contractimpl, contracttype, testutils::Address as _, Address, Env, Map, String, Vec,
};
use vault::{Vault, VaultClient, VaultParams};

use crate::common::{token_balance, MockToken, MockTokenClient};

// ---------------------------------------------------------------------------
// MockOracle — returns a fixed price for any asset
// ---------------------------------------------------------------------------

#[contracttype]
enum OKey {
    Price(Address),
}

#[contract]
struct MockOracle;

#[contractimpl]
impl MockOracle {
    pub fn set_price(env: Env, asset: Address, price: i128) {
        env.storage().instance().set(&OKey::Price(asset), &price);
    }
    pub fn get_price(env: Env, asset: Address) -> i128 {
        env.storage()
            .instance()
            .get(&OKey::Price(asset))
            .unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// MockFactory2 — minimal factory for portfolio/guard whitelist validation
// ---------------------------------------------------------------------------

#[contracttype]
enum F2Key {
    Asset(Address),
    Guard(Address),
}

#[contract]
struct MockFactory2;

#[contractimpl]
impl MockFactory2 {
    pub fn authorize_asset(env: Env, asset: Address) {
        env.storage().instance().set(&F2Key::Asset(asset), &true);
    }
    pub fn is_authorized_asset(env: Env, asset: Address) -> bool {
        env.storage()
            .instance()
            .get(&F2Key::Asset(asset))
            .unwrap_or(false)
    }
    pub fn authorize_guard(env: Env, guard: Address) {
        env.storage().instance().set(&F2Key::Guard(guard), &true);
    }
    pub fn is_authorized_guard(env: Env, guard: Address) -> bool {
        env.storage()
            .instance()
            .get(&F2Key::Guard(guard))
            .unwrap_or(false)
    }
    pub fn get_asset_handler(_env: Env) -> Option<Address> {
        None
    }
}

// ---------------------------------------------------------------------------
// MockGuard2 — simple guard that tracks a value and supports withdraw_fraction
// ---------------------------------------------------------------------------

#[contracttype]
enum G2Key {
    Value,
    Token,
    VaultAddr,
}

#[contract]
struct MockGuard2;

#[contractimpl]
impl MockGuard2 {
    /// Initialize: store the token used for guard positions.
    pub fn init(env: Env, vault: Address, token: Address) {
        env.storage().instance().set(&G2Key::VaultAddr, &vault);
        env.storage().instance().set(&G2Key::Token, &token);
        env.storage().instance().set(&G2Key::Value, &0i128);
    }
    /// Simulate a position deposit: add `amount` to the guard's tracked value.
    pub fn add_position(env: Env, amount: i128) {
        let v: i128 = env.storage().instance().get(&G2Key::Value).unwrap_or(0);
        env.storage().instance().set(&G2Key::Value, &(v + amount));
    }

    // Guard interface.
    pub fn get_total_value(env: Env, _vault: Address) -> i128 {
        env.storage().instance().get(&G2Key::Value).unwrap_or(0)
    }
    pub fn get_underlying_asset_balances(env: Env, _vault: Address) -> Map<Address, i128> {
        let mut out = Map::new(&env);
        let v: i128 = env.storage().instance().get(&G2Key::Value).unwrap_or(0);
        if v > 0 {
            let token: Address = env.storage().instance().get(&G2Key::Token).unwrap();
            out.set(token, v);
        }
        out
    }
    pub fn withdraw_fraction(
        env: Env,
        _vault: Address,
        numerator: i128,
        denominator: i128,
        to: Address,
    ) {
        let v: i128 = env.storage().instance().get(&G2Key::Value).unwrap_or(0);
        let payout = v * numerator / denominator;
        // Transfer tokens to user.
        let token: Address = env.storage().instance().get(&G2Key::Token).unwrap();
        MockTokenClient::new(&env, &token).transfer(&env.current_contract_address(), &to, &payout);
        env.storage().instance().set(&G2Key::Value, &(v - payout));
    }
    pub fn asset_in_use(env: Env, _vault: Address, _asset: Address) -> bool {
        let v: i128 = env.storage().instance().get(&G2Key::Value).unwrap_or(0);
        v > 0
    }
}

// ---------------------------------------------------------------------------
// World setup
// ---------------------------------------------------------------------------

struct World {
    env: Env,
    vault: VaultClient<'static>,
    vault_addr: Address,
    share: ShareTokenContractClient<'static>,
    base: Address,    // USDC — base asset
    token_b: Address, // USDT — second portfolio asset
    manager: Address,
    _trader: Address,
    user: Address,
    user2: Address,
    oracle: Address,
    factory: Address,
}

fn setup() -> World {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let manager = Address::generate(&env);
    let trader = Address::generate(&env);
    let user = Address::generate(&env);
    let user2 = Address::generate(&env);

    // Tokens.
    let base = env.register(MockToken, ());
    let token_b = env.register(MockToken, ());
    MockTokenClient::new(&env, &base).initialize(&manager);
    MockTokenClient::new(&env, &token_b).initialize(&manager);

    // Oracle: 1 token_b = 1 base (price = 1_0000000).
    let oracle_id = env.register(MockOracle, ());
    MockOracleClient::new(&env, &oracle_id).set_price(&token_b, &1_0000000i128);

    // Factory.
    let factory_id = env.register(MockFactory2, ());
    MockFactory2Client::new(&env, &factory_id).authorize_asset(&base);
    MockFactory2Client::new(&env, &factory_id).authorize_asset(&token_b);

    let vault_id = Address::generate(&env);

    // Share token.
    let share_id = env.register(
        ShareTokenContract,
        (
            vault_id.clone(),
            String::from_str(&env, "Vault Share"),
            String::from_str(&env, "VS"),
            7u32,
        ),
    );
    let share = ShareTokenContractClient::new(&env, &share_id);

    // Vault.
    env.register_at(
        &vault_id,
        Vault,
        (VaultParams {
            admin: manager.clone(),
            manager: manager.clone(),
            manager_name: None,
            trader: trader.clone(),
            base_asset: base.clone(),
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
    let vault_client = VaultClient::new(&env, &vault_id);

    // Wire oracle.
    vault_client.set_oracle(&manager, &oracle_id);

    // Add both tokens to portfolio and deposit assets.
    vault_client.add_portfolio_asset(&manager, &base);
    vault_client.add_portfolio_asset(&manager, &token_b);
    vault_client.add_deposit_asset(&manager, &base);
    vault_client.add_deposit_asset(&manager, &token_b);

    // Fund users.
    MockTokenClient::new(&env, &base).mint(&user, &100_000_0000000i128);
    MockTokenClient::new(&env, &base).mint(&user2, &100_000_0000000i128);
    MockTokenClient::new(&env, &token_b).mint(&user, &100_000_0000000i128);
    MockTokenClient::new(&env, &token_b).mint(&user2, &100_000_0000000i128);

    let vault: VaultClient<'static> = unsafe { core::mem::transmute(vault_client) };
    let share: ShareTokenContractClient<'static> = unsafe { core::mem::transmute(share) };

    World {
        env,
        vault,
        vault_addr: vault_id,
        share,
        base,
        token_b,
        manager,
        _trader: trader,
        user,
        user2,
        oracle: oracle_id,
        factory: factory_id,
    }
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn nav(w: &World) -> i128 {
    w.vault.get_nav()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Portfolio assets list is correctly populated after setup.
#[test]
fn test_portfolio_and_deposit_assets_setup() {
    let w = setup();
    let portfolio = w.vault.get_portfolio_assets();
    assert_eq!(portfolio.len(), 2);
    let deposits = w.vault.get_deposit_assets();
    assert_eq!(deposits.len(), 2);
}

/// Depositing the base asset in multi-asset mode works and updates NAV.
#[test]
fn test_deposit_base_asset_multi_asset_mode() {
    let w = setup();
    let deposit = 1_000_0000000i128;

    let shares = w.vault.deposit(&deposit, &w.user, &w.base, &0i128);

    assert!(shares > 0);
    assert_eq!(token_balance(&w.env, &w.base, &w.vault_addr), deposit);
    assert_eq!(nav(&w), deposit);
}

/// NAV includes both base and non-base portfolio asset balances.
#[test]
fn test_nav_includes_all_portfolio_assets() {
    let w = setup();

    // Deposit base into vault via deposit().
    let base_deposit = 500_0000000i128;
    w.vault.deposit(&base_deposit, &w.user, &w.base, &0i128);

    // Deposit token_b into vault via deposit().
    let b_deposit = 300_0000000i128;
    w.vault.deposit(&b_deposit, &w.user, &w.token_b, &0i128);

    // NAV = base_deposit + b_deposit * oracle_price / PRICE_PRECISION
    // oracle_price = 1_0000000, PRICE_PRECISION = 1_0000000
    let expected_nav = base_deposit + b_deposit;
    assert_eq!(nav(&w), expected_nav);
}

/// Active guard total_value contributes to NAV.
#[test]
fn test_nav_includes_active_guard_value() {
    let w = setup();

    // Set up guard and register it.
    let guard_id = w.env.register(MockGuard2, ());
    MockFactory2Client::new(&w.env, &w.factory).authorize_guard(&guard_id);
    MockGuard2Client::new(&w.env, &guard_id).init(&w.vault_addr, &w.base);
    w.vault.add_active_guard(&w.manager, &guard_id);

    // Deposit base.
    let deposit = 1_000_0000000i128;
    w.vault.deposit(&deposit, &w.user, &w.base, &0i128);

    // Simulate guard position: invest 200 from vault to guard.
    let guard_value = 200_0000000i128;
    // Transfer base from vault to guard to simulate investment.
    MockTokenClient::new(&w.env, &w.base).transfer(&w.vault_addr, &guard_id, &guard_value);
    MockGuard2Client::new(&w.env, &guard_id).add_position(&guard_value);
    w.vault.sync_guard_position(&guard_id);
    // Vault now holds (deposit - guard_value) base + guard_value in guard.
    let vault_cash = token_balance(&w.env, &w.base, &w.vault_addr);
    assert_eq!(vault_cash, deposit - guard_value);

    // NAV = vault_cash + guard.get_total_value() = deposit.
    assert_eq!(nav(&w), deposit);
}

/// Proportional withdrawal: user gets fraction of each portfolio asset.
#[test]
fn test_proportional_withdrawal_two_assets() {
    let w = setup();

    // Deposit base and token_b.
    let base_dep = 600_0000000i128;
    let b_dep = 400_0000000i128;
    let shares = w.vault.deposit(&base_dep, &w.user, &w.base, &0i128);
    w.vault.deposit(&b_dep, &w.user, &w.token_b, &0i128);

    // All shares belong to user. Withdraw all → should receive all assets.
    let user_base_before = token_balance(&w.env, &w.base, &w.user);
    let user_b_before = token_balance(&w.env, &w.token_b, &w.user);

    let total_shares = w.share.total_supply();
    w.vault.withdraw(&total_shares, &w.user, &w.user, &0i128);

    let user_base_after = token_balance(&w.env, &w.base, &w.user);
    let user_b_after = token_balance(&w.env, &w.token_b, &w.user);

    // User should have received base and token_b back.
    assert_eq!(user_base_after - user_base_before, base_dep);
    assert_eq!(user_b_after - user_b_before, b_dep);
    // Vault should be empty.
    assert_eq!(token_balance(&w.env, &w.base, &w.vault_addr), 0);
    assert_eq!(token_balance(&w.env, &w.token_b, &w.vault_addr), 0);
}

/// Two users hold shares; partial withdrawal of user1 receives correct fraction.
#[test]
fn test_proportional_withdrawal_two_users() {
    let w = setup();

    // user deposits 1000 base, user2 deposits 1000 base.
    let dep = 1_000_0000000i128;
    w.vault.deposit(&dep, &w.user, &w.base, &0i128);
    w.vault.deposit(&dep, &w.user2, &w.base, &0i128);

    // Vault holds 2000 base; user holds 50% of shares.
    let user_shares = w.share.balance(&w.user);
    let total_supply = w.share.total_supply();
    // user's fraction = user_shares / total_supply = 1/2.

    let user_base_before = token_balance(&w.env, &w.base, &w.user);
    w.vault.withdraw(&user_shares, &w.user, &w.user, &0i128);
    let user_base_after = token_balance(&w.env, &w.base, &w.user);

    let received = user_base_after - user_base_before;
    // user should get exactly dep (= 2*dep * user_shares/total_supply = 2*dep/2).
    assert_eq!(received, dep);
    // user2's share is untouched.
    assert_eq!(w.share.balance(&w.user2), total_supply - user_shares);
}

/// Withdrawal with active guard calls withdraw_fraction on the guard.
#[test]
fn test_proportional_withdrawal_includes_guard() {
    let w = setup();

    // Register guard.
    let guard_id = w.env.register(MockGuard2, ());
    MockFactory2Client::new(&w.env, &w.factory).authorize_guard(&guard_id);
    MockGuard2Client::new(&w.env, &guard_id).init(&w.vault_addr, &w.base);
    w.vault.add_active_guard(&w.manager, &guard_id);

    // user deposits.
    let deposit = 1_000_0000000i128;
    w.vault.deposit(&deposit, &w.user, &w.base, &0i128);

    // Invest 600 into guard; vault retains 400 base.
    let guard_position = 600_0000000i128;
    MockTokenClient::new(&w.env, &w.base).transfer(&w.vault_addr, &guard_id, &guard_position);
    MockGuard2Client::new(&w.env, &guard_id).add_position(&guard_position);
    w.vault.sync_guard_position(&guard_id);

    // User withdraws all shares.
    let user_shares = w.share.balance(&w.user);
    let user_base_before = token_balance(&w.env, &w.base, &w.user);
    w.vault.withdraw(&user_shares, &w.user, &w.user, &0i128);
    let user_base_after = token_balance(&w.env, &w.base, &w.user);

    // User should receive vault cash (400) + guard payout (600) = 1000.
    let total_received = user_base_after - user_base_before;
    assert_eq!(total_received, deposit);
    // Guard position should be zeroed out.
    assert_eq!(
        MockGuard2Client::new(&w.env, &guard_id).get_total_value(&w.vault_addr),
        0
    );
}

/// PnL tracking: cost_basis is set on deposit and realized on withdrawal.
#[test]
fn test_pnl_tracking_deposit_and_withdrawal() {
    let w = setup();
    let deposit = 1_000_0000000i128;

    // Before deposit: zero report.
    let before = w.vault.get_user_pnl(&w.user);
    assert_eq!(before.cost_basis, 0);

    // After deposit: cost_basis = deposit_value.
    w.vault.deposit(&deposit, &w.user, &w.base, &0i128);
    let after_deposit = w.vault.get_user_pnl(&w.user);
    assert_eq!(after_deposit.cost_basis, deposit);
    assert_eq!(after_deposit.realized_pnl, 0);
    assert_eq!(after_deposit.unrealized_pnl, 0);

    // After full withdrawal: cost_basis = 0, realized_pnl = 0 (no growth).
    let user_shares = w.share.balance(&w.user);
    w.vault.withdraw(&user_shares, &w.user, &w.user, &0i128);
    let after_withdraw = w.vault.get_user_pnl(&w.user);
    assert_eq!(after_withdraw.cost_basis, 0);
    assert_eq!(after_withdraw.realized_pnl, 0);
}

/// PnL tracking: NAV growth is captured as realized_pnl on withdrawal.
#[test]
fn test_pnl_realized_on_nav_growth() {
    let w = setup();
    let deposit = 1_000_0000000i128;
    w.vault.deposit(&deposit, &w.user, &w.base, &0i128);

    // Simulate 10% NAV growth via airdrop.
    let extra = 100_0000000i128;
    MockTokenClient::new(&w.env, &w.base).mint(&w.vault_addr, &extra);

    // Withdraw all shares.
    let user_shares = w.share.balance(&w.user);
    w.vault.withdraw(&user_shares, &w.user, &w.user, &0i128);

    let report = w.vault.get_user_pnl(&w.user);
    assert_eq!(report.cost_basis, 0); // position fully closed
    assert!(report.realized_pnl > 0); // profit was realized
}

/// Depositing non-base asset (token_b) correctly values deposit via oracle.
#[test]
fn test_deposit_non_base_asset_via_oracle() {
    let w = setup();

    // Set oracle price: 1 token_b = 2 base.
    MockOracleClient::new(&w.env, &w.oracle).set_price(&w.token_b, &2_0000000i128);

    let b_deposit = 500_0000000i128; // 500 token_b at price 2 → 1000 base value
    w.vault.deposit(&b_deposit, &w.user, &w.token_b, &0i128);

    // NAV should be 1000 base equivalent.
    assert_eq!(nav(&w), 1_000_0000000i128);

    // cost_basis should reflect oracle-priced deposit_value = 1000.
    let report = w.vault.get_user_pnl(&w.user);
    assert_eq!(report.cost_basis, 1_000_0000000i128);
}

/// Removing a portfolio asset fails when guard uses it.
#[test]
#[should_panic(expected = "Error(Contract, #34)")]
fn test_remove_portfolio_asset_blocked_by_guard() {
    let w = setup();

    // Register guard that uses token_b.
    let guard_id = w.env.register(MockGuard2, ());
    MockFactory2Client::new(&w.env, &w.factory).authorize_guard(&guard_id);
    MockGuard2Client::new(&w.env, &guard_id).init(&w.vault_addr, &w.token_b);
    w.vault.add_active_guard(&w.manager, &guard_id);

    // Give guard a position so asset_in_use returns true.
    MockGuard2Client::new(&w.env, &guard_id).add_position(&100_0000000i128);

    // Attempt to remove token_b from portfolio → should fail.
    w.vault.remove_portfolio_asset(&w.manager, &w.token_b);
}

/// Factory whitelist blocks adding an unauthorized asset to portfolio.
#[test]
#[should_panic(expected = "Error(Contract, #30)")]
fn test_add_unauthorized_portfolio_asset_panics() {
    let w = setup();
    let unknown_asset = Address::generate(&w.env);
    // unknown_asset is not in factory's authorized list.
    w.vault.add_portfolio_asset(&w.manager, &unknown_asset);
}
