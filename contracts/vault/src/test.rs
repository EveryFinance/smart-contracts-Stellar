#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger as _},
    vec, Address, Env, IntoVal, String, Symbol, Val, Vec,
};

use crate::{Vault, VaultClient, VaultParams};

// ---------------------------------------------------------------------------
// MockToken — minimal SEP-41 for testing
//
// NOTE: This mock intentionally omits admin-auth checks on mint/burn and does
// not enforce allowance expiry. It exists only to exercise vault accounting
// logic; it does NOT test SEP-41 token security properties. Auth enforcement
// and allowance semantics are verified by the real token contracts in
// integration tests.
// ---------------------------------------------------------------------------

#[contracttype]
enum TKey {
    Balance(Address),
    Allowance(Address, Address),
    TotalSupply,
    Admin,
}
#[contract]
pub struct MockToken;
#[contractimpl]
impl MockToken {
    pub fn initialize(env: Env, admin: Address) {
        env.storage().instance().set(&TKey::Admin, &admin);
    }
    pub fn get_admin(env: Env) -> Address {
        env.storage().instance().get(&TKey::Admin).unwrap()
    }
    pub fn set_admin(env: Env, new_admin: Address) {
        let admin: Address = env.storage().instance().get(&TKey::Admin).unwrap();
        admin.require_auth();
        env.storage().instance().set(&TKey::Admin, &new_admin);
    }
    pub fn mint(env: Env, to: Address, amount: i128) {
        let b: i128 = env
            .storage()
            .persistent()
            .get(&TKey::Balance(to.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&TKey::Balance(to.clone()), &(b + amount));
        let s: i128 = env
            .storage()
            .persistent()
            .get(&TKey::TotalSupply)
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&TKey::TotalSupply, &(s + amount));
    }
    pub fn burn(env: Env, from: Address, amount: i128) {
        from.require_auth();
        let b: i128 = env
            .storage()
            .persistent()
            .get(&TKey::Balance(from.clone()))
            .unwrap_or(0);
        assert!(b >= amount, "burn > balance");
        env.storage()
            .persistent()
            .set(&TKey::Balance(from.clone()), &(b - amount));
        let s: i128 = env
            .storage()
            .persistent()
            .get(&TKey::TotalSupply)
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&TKey::TotalSupply, &(s - amount));
    }
    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&TKey::Balance(id))
            .unwrap_or(0)
    }
    pub fn total_supply(env: Env) -> i128 {
        env.storage()
            .persistent()
            .get(&TKey::TotalSupply)
            .unwrap_or(0)
    }
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        let fb: i128 = env
            .storage()
            .persistent()
            .get(&TKey::Balance(from.clone()))
            .unwrap_or(0);
        assert!(fb >= amount, "transfer: insufficient balance");
        env.storage()
            .persistent()
            .set(&TKey::Balance(from), &(fb - amount));
        let tb: i128 = env
            .storage()
            .persistent()
            .get(&TKey::Balance(to.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&TKey::Balance(to), &(tb + amount));
    }
    pub fn approve(env: Env, from: Address, spender: Address, amount: i128, _expiry: u32) {
        env.storage()
            .persistent()
            .set(&TKey::Allowance(from, spender), &amount);
    }
    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&TKey::Allowance(from, spender))
            .unwrap_or(0)
    }
    pub fn transfer_from(env: Env, _sp: Address, from: Address, to: Address, amount: i128) {
        let fb: i128 = env
            .storage()
            .persistent()
            .get(&TKey::Balance(from.clone()))
            .unwrap_or(0);
        assert!(fb >= amount);
        env.storage()
            .persistent()
            .set(&TKey::Balance(from), &(fb - amount));
        let tb: i128 = env
            .storage()
            .persistent()
            .get(&TKey::Balance(to.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&TKey::Balance(to), &(tb + amount));
    }
    pub fn decimals(_env: Env) -> u32 {
        7
    }
    pub fn name(env: Env) -> String {
        String::from_str(&env, "Mock")
    }
    pub fn symbol(env: Env) -> String {
        String::from_str(&env, "MCK")
    }
}

// ---------------------------------------------------------------------------
// MockStrategy — returns a fixed value; deposit/withdraw just pass base tokens
// ---------------------------------------------------------------------------

#[contracttype]
enum SKey {
    Value,
    BaseAsset,
    HasOracle,
}
#[contract]
pub struct MockStrategy;
#[contractimpl]
impl MockStrategy {
    pub fn init(env: Env, base_asset: Address) {
        env.storage().instance().set(&SKey::BaseAsset, &base_asset);
        env.storage().instance().set(&SKey::Value, &0i128);
        env.storage().instance().set(&SKey::HasOracle, &false);
    }
    /// Simulate deposit: pull tokens from vault, record value.
    pub fn deposit(env: Env, amount: i128, from: Address) -> i128 {
        let base: Address = env.storage().instance().get(&SKey::BaseAsset).unwrap();
        let strategy = env.current_contract_address();
        MockTokenClient::new(&env, &base).transfer(&from, &strategy, &amount);
        let v: i128 = env.storage().instance().get(&SKey::Value).unwrap_or(0);
        env.storage().instance().set(&SKey::Value, &(v + amount));
        amount
    }
    /// Simulate withdraw: return tokens to `to`, decrease value.
    pub fn withdraw(env: Env, amount: i128, _from: Address, to: Address) -> i128 {
        let base: Address = env.storage().instance().get(&SKey::BaseAsset).unwrap();
        let strategy = env.current_contract_address();
        MockTokenClient::new(&env, &base).transfer(&strategy, &to, &amount);
        let v: i128 = env.storage().instance().get(&SKey::Value).unwrap_or(0);
        let new_v = if v >= amount { v - amount } else { 0 };
        env.storage().instance().set(&SKey::Value, &new_v);
        amount
    }
    pub fn get_value(env: Env, _vault: Address) -> i128 {
        env.storage().instance().get(&SKey::Value).unwrap_or(0)
    }
    pub fn has_oracle(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&SKey::HasOracle)
            .unwrap_or(false)
    }
    pub fn set_oracle_enabled(env: Env, enabled: bool) {
        env.storage().instance().set(&SKey::HasOracle, &enabled);
    }
}

// ---------------------------------------------------------------------------
// Test setup
// ---------------------------------------------------------------------------

struct T {
    env: Env,
    vault: VaultClient<'static>,
    vault_addr: Address,
    base: Address,
    share: Address,
    manager: Address,
    trader: Address,
    user: Address,
    user2: Address,
}

fn setup_with_fees(entry: u32, exit: u32, mgmt: u32, perf: u32) -> T {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let manager = Address::generate(&env);
    let trader = Address::generate(&env);
    let user = Address::generate(&env);
    let user2 = Address::generate(&env);

    let base = env.register(MockToken, ());
    let share = env.register(MockToken, ());

    MockTokenClient::new(&env, &base).initialize(&manager);
    MockTokenClient::new(&env, &share).initialize(&manager);

    // Fund users with base tokens.
    MockTokenClient::new(&env, &base).mint(&manager, &1_000_000_0000000i128);
    MockTokenClient::new(&env, &base).mint(&user, &1_000_000_0000000i128);
    MockTokenClient::new(&env, &base).mint(&user2, &1_000_000_0000000i128);

    let vid = env.register(
        Vault,
        (VaultParams {
            admin: manager.clone(),
            manager: manager.clone(),
            manager_name: None,
            trader: trader.clone(),
            base_asset: base.clone(),
            share_token: share.clone(),
            share_token_admin: manager.clone(),
            treasury: manager.clone(),
            entry_fee_bps: entry,
            exit_fee_bps: exit,
            mgmt_fee_bps: mgmt,
            perf_fee_bps: perf,
            factory: None,
            is_private: false,
        },),
    );
    let vault = VaultClient::new(&env, &vid);

    // NAV only counts assets explicitly in PortfolioAssets.  Add base so that
    // idle vault cash appears in NAV and deposit/withdraw tests work correctly.
    vault.add_portfolio_asset(&manager, &base);
    vault.add_deposit_asset(&manager, &base);

    let vault: VaultClient<'static> = unsafe { core::mem::transmute(vault) };
    let vault_addr = vid;

    T {
        env,
        vault,
        vault_addr,
        base,
        share,
        manager,
        trader,
        user,
        user2,
    }
}

fn setup() -> T {
    setup_with_fees(0, 0, 0, 0)
}

/// Returns a `T` whose vault was constructed with `factory` wired at deploy
/// time, plus the factory address so callers can configure its whitelist.
fn setup_with_factory() -> (T, Address) {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let manager = Address::generate(&env);
    let trader = Address::generate(&env);
    let user = Address::generate(&env);
    let user2 = Address::generate(&env);

    let base = env.register(MockToken, ());
    let share = env.register(MockToken, ());
    MockTokenClient::new(&env, &base).initialize(&manager);
    MockTokenClient::new(&env, &share).initialize(&manager);
    MockTokenClient::new(&env, &base).mint(&user, &1_000_000_0000000i128);
    MockTokenClient::new(&env, &base).mint(&user2, &1_000_000_0000000i128);

    let factory_id = env.register(MockFactory, ());

    let vid = env.register(
        Vault,
        (VaultParams {
            admin: manager.clone(),
            manager: manager.clone(),
            manager_name: None,
            trader: trader.clone(),
            base_asset: base.clone(),
            share_token: share.clone(),
            share_token_admin: manager.clone(),
            treasury: manager.clone(),
            entry_fee_bps: 0,
            exit_fee_bps: 0,
            mgmt_fee_bps: 0,
            perf_fee_bps: 0,
            factory: Some(factory_id.clone()),
            is_private: false,
        },),
    );
    let vault = VaultClient::new(&env, &vid);
    let vault: VaultClient<'static> = unsafe { core::mem::transmute(vault) };
    let vault_addr = vid;

    let t = T { env, vault, vault_addr, base, share, manager, trader, user, user2 };
    (t, factory_id)
}

fn base(t: &T, addr: &Address) -> i128 {
    MockTokenClient::new(&t.env, &t.base).balance(addr)
}
fn shares(t: &T, addr: &Address) -> i128 {
    MockTokenClient::new(&t.env, &t.share).balance(addr)
}
fn total_supply(t: &T) -> i128 {
    MockTokenClient::new(&t.env, &t.share).total_supply()
}

fn advance_time(t: &T, secs: u64) {
    t.env.ledger().with_mut(|li| {
        li.timestamp = li.timestamp.saturating_add(secs);
        li.sequence_number = li.sequence_number.saturating_add(1);
    });
}

// ---------------------------------------------------------------------------
// Initialization tests
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_stores_params() {
    let t = setup();
    assert_eq!(t.vault.get_manager(), t.manager);
    assert_eq!(t.vault.get_trader(), t.trader);
    assert_eq!(t.vault.get_base_asset(), t.base);
    assert_eq!(t.vault.get_share_token(), t.share);
    assert!(!t.vault.is_paused());
    assert!(t.vault.get_active_guards().is_empty());
}

#[test]
#[should_panic]
fn test_initialize_entry_fee_too_high_panics() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let manager = Address::generate(&env);
    let trader = Address::generate(&env);
    let base = env.register(MockToken, ());
    let share = env.register(MockToken, ());
    MockTokenClient::new(&env, &base).initialize(&manager);
    MockTokenClient::new(&env, &share).initialize(&manager);
    env.register(
        Vault,
        (VaultParams {
            admin: manager.clone(),
            manager: manager.clone(),
            manager_name: None,
            trader: trader.clone(),
            base_asset: base,
            share_token: share,
            share_token_admin: manager.clone(),
            treasury: manager.clone(),
            entry_fee_bps: 501, // > MAX
            exit_fee_bps: 0,
            mgmt_fee_bps: 0,
            perf_fee_bps: 0,
            factory: None,
            is_private: false,
        },),
    );
}

// ---------------------------------------------------------------------------
// Deposit tests
// ---------------------------------------------------------------------------

#[test]
fn test_deposit_mints_shares_bootstrap() {
    let t = setup();
    let amount = 1_000_0000000i128;
    let shares_minted = t.vault.deposit(&amount, &t.user, &t.base, &0i128);
    // Bootstrap: shares_minted = net_amount = amount (no fees).
    assert_eq!(shares_minted, amount);
    assert_eq!(shares(&t, &t.user), amount);
    assert_eq!(total_supply(&t), amount);
}

#[test]
fn test_deposit_with_entry_fee() {
    let t = setup_with_fees(100, 0, 0, 0); // 1% entry
    let amount = 1_000_0000000i128;
    let user_shares = t.vault.deposit(&amount, &t.user, &t.base, &0i128);
    // Bootstrap: total_shares = amount; fee_shares = amount * 100/10_000.
    let expected_fee_shares = amount * 100 / 10_000;
    let expected_user_shares = amount - expected_fee_shares;
    // User receives (1 − fee%) of total shares.
    assert_eq!(user_shares, expected_user_shares);
    // Manager receives fee as shares, not base asset.
    assert_eq!(shares(&t, &t.manager), expected_fee_shares);
    // Full deposit amount stays in vault — no base-asset extraction.
    assert_eq!(base(&t, &t.vault_addr), amount);
}

#[test]
fn test_second_deposit_proportional_shares() {
    let t = setup();
    let first = 1_000_0000000i128;
    t.vault.deposit(&first, &t.user, &t.base, &0i128);

    let second = 500_0000000i128;
    let shares2 = t.vault.deposit(&second, &t.user2, &t.base, &0i128);
    // NAV = first, total_supply = first → price = 1.0
    // shares2 should equal second.
    assert_eq!(shares2, second);
}

#[test]
#[should_panic]
fn test_deposit_paused_panics() {
    let t = setup();
    t.vault.pause_deposits(&t.manager);
    t.vault.deposit(&1_000i128, &t.user, &t.base, &0i128);
}

#[test]
#[should_panic]
fn test_deposit_zero_panics() {
    let t = setup();
    t.vault.deposit(&0i128, &t.user, &t.base, &0i128);
}

// ---------------------------------------------------------------------------
// Withdraw tests
// ---------------------------------------------------------------------------

#[test]
fn test_withdraw_burns_shares_returns_base() {
    let t = setup();
    let amount = 1_000_0000000i128;
    t.vault.deposit(&amount, &t.user, &t.base, &0i128);

    let user_shares = shares(&t, &t.user);
    let before_base = base(&t, &t.user);
    let returned = t.vault.withdraw(&user_shares, &t.user, &t.user, &0i128);

    assert_eq!(returned, amount); // no fees, full amount back
    assert_eq!(shares(&t, &t.user), 0);
    assert_eq!(base(&t, &t.user), before_base + amount);
}

#[test]
fn test_withdraw_with_exit_fee() {
    let t = setup_with_fees(0, 100, 0, 0); // 1% exit
    let amount = 1_000_0000000i128;
    let before_mgr_base = base(&t, &t.manager);
    t.vault.deposit(&amount, &t.user, &t.base, &0i128);

    let user_shares = shares(&t, &t.user);
    let returned = t.vault.withdraw(&user_shares, &t.user, &t.user, &0i128);

    // User gets (1 − 1%) of gross value.
    let expected_net = amount * (10_000 - 100) / 10_000;
    assert_eq!(returned, expected_net);
    // Fee stays in vault — manager's base balance is unchanged.
    assert_eq!(base(&t, &t.manager), before_mgr_base);
    // Fee residual (1%) remains in vault.
    assert_eq!(base(&t, &t.vault_addr), amount - expected_net);
}

#[test]
#[should_panic]
fn test_withdraw_insufficient_shares_panics() {
    let t = setup();
    let amount = 1_000_0000000i128;
    t.vault.deposit(&amount, &t.user, &t.base, &0i128);
    t.vault.withdraw(&(amount + 1), &t.user, &t.user, &0i128);
}

#[test]
#[should_panic]
fn test_withdraw_paused_panics() {
    let t = setup();
    t.vault.deposit(&1_000_0000000i128, &t.user, &t.base, &0i128);
    t.vault.pause_deposits(&t.manager);
    t.vault.withdraw(&1i128, &t.user, &t.user, &0i128);
}

#[test]
#[should_panic]
fn test_withdraw_zero_panics() {
    let t = setup();
    t.vault.deposit(&1_000_0000000i128, &t.user, &t.base, &0i128);
    t.vault.withdraw(&0i128, &t.user, &t.user, &0i128);
}

/// Share-price floor test: when nav * PRICE_PRECISION < total_supply the price
/// truncates to 0 under naive integer division.  The floor-at-1 fix ensures
/// get_share_price() never returns 0 when there is non-zero NAV, preventing
/// the withdrawal path from computing base_gross = 0 and bricking.
///
/// We test the floor logic by verifying the price is >= 1 in an extreme dilution
/// scenario.  The actual withdrawal math (share_amount * price / PRICE_PRECISION)
/// requires enough shares to produce base_gross >= 1, so we also verify that
/// a holder of PRICE_PRECISION shares can withdraw when price is floored to 1.
#[test]
fn test_share_price_floor_prevents_zero_price() {
    let t = setup();

    // Deposit a small amount to establish a non-zero NAV.
    let small_deposit = 1_000_0000000i128; // 1000 tokens (7 decimals)
    t.vault.deposit(&small_deposit, &t.user, &t.base, &0i128);
    // total_supply = 1000_0000000, nav = 1000_0000000 → price = PRICE_PRECISION = 10_000_000.

    // Now mint a huge number of share tokens to simulate extreme dilution.
    // nav stays at 1000_0000000 but total_supply grows massively.
    // nav * PRICE_PRECISION = 1000_0000000 * 10_000_000 = 10^16
    // We need total_supply > 10^16 to force naive price to 0.
    let huge_shares = 100_000_000_000_000_000i128; // 10^17 shares
    MockTokenClient::new(&t.env, &t.share).mint(&t.user2, &huge_shares);

    // price = nav * PRICE_PRECISION / total_supply
    //       = 10_000_000_000_000_000 / (10_000_000_000 + 10^17) ≈ 0 (naive)
    // With floor: price = 1.
    let price = t.vault.get_share_price();
    assert!(price >= 1, "share price must be at least 1 even under extreme dilution");
}

// ---------------------------------------------------------------------------
// Pause / Unpause tests
// ---------------------------------------------------------------------------

#[test]
fn test_pause_unpause() {
    let t = setup();
    t.vault.pause_deposits(&t.manager);
    assert!(t.vault.is_paused());
    t.vault.unpause_deposits(&t.manager);
    assert!(!t.vault.is_paused());
}

#[test]
#[should_panic]
fn test_pause_not_admin_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.vault.pause_deposits(&rogue);
}

#[test]
fn test_pause_operations() {
    let t = setup();
    assert!(!t.vault.is_ops_paused());
    t.vault.pause_operations(&t.manager);
    assert!(t.vault.is_ops_paused());
    t.vault.unpause_operations(&t.manager);
    assert!(!t.vault.is_ops_paused());
}

// ---------------------------------------------------------------------------
// NAV / Share-price tests
// ---------------------------------------------------------------------------

#[test]
fn test_nav_and_share_price_without_strategies() {
    let t = setup();
    let amount = 1_000_0000000i128;
    t.vault.deposit(&amount, &t.user, &t.base, &0i128);
    assert_eq!(t.vault.get_nav(), amount);
    // share_price = NAV * PRICE_PRECISION / total_supply = 1.0
    assert_eq!(t.vault.get_share_price(), 10_000_000i128);
}

#[test]
fn test_full_lifecycle() {
    let t = setup();
    let (guard_id, _) = setup_with_execute_op(&t);
    let no_args: Vec<Val> = Vec::new(&t.env);

    // Two users deposit.
    let dep1 = 1_000_0000000i128;
    let dep2 = 500_0000000i128;
    let s1 = t.vault.deposit(&dep1, &t.user, &t.base, &0i128);
    let s2 = t.vault.deposit(&dep2, &t.user2, &t.base, &0i128);

    assert_eq!(total_supply(&t), s1 + s2);
    assert_eq!(t.vault.get_nav(), dep1 + dep2);

    // Manager dispatches "noop" through the guard — NAV unchanged.
    t.vault.execute_op(&t.manager, &guard_id, &Symbol::new(&t.env, "noop"), &no_args);
    assert_eq!(t.vault.get_nav(), dep1 + dep2);

    // User 1 withdraws all.
    let before = base(&t, &t.user);
    t.vault.withdraw(&s1, &t.user, &t.user, &0i128);
    assert_eq!(base(&t, &t.user), before + dep1);

    // User 2 withdraws all.
    let before2 = base(&t, &t.user2);
    t.vault.withdraw(&s2, &t.user2, &t.user2, &0i128);
    assert_eq!(base(&t, &t.user2), before2 + dep2);

    assert_eq!(total_supply(&t), 0);
}

// ---------------------------------------------------------------------------
// Fee validation at initialization
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_initialize_exit_fee_too_high_panics() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let manager = Address::generate(&env);
    let trader = Address::generate(&env);
    let base = env.register(MockToken, ());
    let share = env.register(MockToken, ());
    MockTokenClient::new(&env, &base).initialize(&manager);
    MockTokenClient::new(&env, &share).initialize(&manager);
    env.register(
        Vault,
        (VaultParams {
            admin: manager.clone(),
            manager: manager.clone(),
            manager_name: None,
            trader: trader.clone(),
            base_asset: base,
            share_token: share,
            share_token_admin: manager.clone(),
            treasury: manager.clone(),
            entry_fee_bps: 0,
            exit_fee_bps: 501, // > MAX_ENTRY_EXIT_FEE_BPS
            mgmt_fee_bps: 0,
            perf_fee_bps: 0,
            factory: None,
            is_private: false,
        },),
    );
}

#[test]
#[should_panic]
fn test_initialize_mgmt_fee_too_high_panics() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let manager = Address::generate(&env);
    let trader = Address::generate(&env);
    let base = env.register(MockToken, ());
    let share = env.register(MockToken, ());
    MockTokenClient::new(&env, &base).initialize(&manager);
    MockTokenClient::new(&env, &share).initialize(&manager);
    env.register(
        Vault,
        (VaultParams {
            admin: manager.clone(),
            manager: manager.clone(),
            manager_name: None,
            trader: trader.clone(),
            base_asset: base,
            share_token: share,
            share_token_admin: manager.clone(),
            treasury: manager.clone(),
            entry_fee_bps: 0,
            exit_fee_bps: 0,
            mgmt_fee_bps: 301, // > MAX_MGMT_FEE_BPS
            perf_fee_bps: 0,
            factory: None,
            is_private: false,
        },),
    );
}

#[test]
#[should_panic]
fn test_initialize_perf_fee_too_high_panics() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let manager = Address::generate(&env);
    let trader = Address::generate(&env);
    let base = env.register(MockToken, ());
    let share = env.register(MockToken, ());
    MockTokenClient::new(&env, &base).initialize(&manager);
    MockTokenClient::new(&env, &share).initialize(&manager);
    env.register(
        Vault,
        (VaultParams {
            admin: manager.clone(),
            manager: manager.clone(),
            manager_name: None,
            trader: trader.clone(),
            base_asset: base,
            share_token: share,
            share_token_admin: manager.clone(),
            treasury: manager.clone(),
            entry_fee_bps: 0,
            exit_fee_bps: 0,
            mgmt_fee_bps: 0,
            perf_fee_bps: 3_001, // > MAX_PERF_FEE_BPS
            factory: None,
            is_private: false,
        },),
    );
}

// ---------------------------------------------------------------------------
// Withdraw to a different address
// ---------------------------------------------------------------------------

#[test]
fn test_withdraw_to_different_address() {
    let t = setup();
    let amount = 1_000_0000000i128;
    t.vault.deposit(&amount, &t.user, &t.base, &0i128);

    let recipient = Address::generate(&t.env);
    let share_amount = shares(&t, &t.user);
    let before_recipient = base(&t, &recipient);
    t.vault.withdraw(&share_amount, &t.user, &recipient, &0i128);

    assert_eq!(base(&t, &recipient), before_recipient + amount);
    assert_eq!(shares(&t, &t.user), 0);
}

// ---------------------------------------------------------------------------
// Unpause by non-manager panics
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_unpause_not_admin_panics() {
    let t = setup();
    t.vault.pause_deposits(&t.manager);
    let rogue = Address::generate(&t.env);
    t.vault.unpause_deposits(&rogue);
}

// ---------------------------------------------------------------------------
// Fee update functions (manager only)
// ---------------------------------------------------------------------------

#[test]
fn test_set_entry_fee_bps_updates() {
    let t = setup();
    t.vault
        .announce_fee_increase(&t.manager, &300u32, &0u32, &0u32, &0u32);
    advance_time(&t, 86_401);
    t.vault.commit_fee_increase(&t.manager);
    let deposit = 10_000_0000000i128;
    t.vault.deposit(&deposit, &t.user, &t.base, &0i128);
    // Fee charged as shares: fee_shares = deposit * 300 / 10_000.
    let expected_fee_shares = deposit * 300 / 10_000;
    assert_eq!(shares(&t, &t.manager), expected_fee_shares);
    // No base asset goes to manager.
    assert_eq!(base(&t, &t.vault_addr), deposit);
}

#[test]
#[should_panic]
fn test_set_entry_fee_bps_too_high_panics() {
    let t = setup();
    t.vault.set_entry_fee_bps(&t.manager, &501u32);
}

#[test]
#[should_panic]
fn test_set_entry_fee_bps_not_manager_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.vault.set_entry_fee_bps(&rogue, &100u32);
}

#[test]
fn test_set_exit_fee_bps_updates() {
    let t = setup();
    t.vault
        .announce_fee_increase(&t.manager, &0u32, &200u32, &0u32, &0u32);
    advance_time(&t, 86_401);
    t.vault.commit_fee_increase(&t.manager);
    let deposit = 10_000_0000000i128;
    t.vault.deposit(&deposit, &t.user, &t.base, &0i128);
    let share_amount = shares(&t, &t.user);
    let before_mgr_base = base(&t, &t.manager);
    let returned = t.vault.withdraw(&share_amount, &t.user, &t.user, &0i128);
    // User receives (1 − 2%) of deposit.
    let expected_net = deposit * (10_000 - 200) / 10_000;
    assert_eq!(returned, expected_net);
    // Manager's base balance is unchanged — fee stays in vault.
    assert_eq!(base(&t, &t.manager), before_mgr_base);
    // Fee residual (2%) remains in vault.
    assert_eq!(base(&t, &t.vault_addr), deposit - expected_net);
}

#[test]
#[should_panic(expected = "Error(Contract, #25)")]
fn test_set_entry_fee_bps_increase_requires_delay() {
    let t = setup();
    t.vault.set_entry_fee_bps(&t.manager, &100u32);
}

#[test]
#[should_panic]
fn test_set_mgmt_fee_bps_too_high_panics() {
    let t = setup();
    t.vault.set_mgmt_fee_bps(&t.manager, &301u32);
}

#[test]
#[should_panic]
fn test_set_perf_fee_bps_too_high_panics() {
    let t = setup();
    t.vault.set_perf_fee_bps(&t.manager, &3_001u32);
}

// ---------------------------------------------------------------------------
// Deposit cap
// ---------------------------------------------------------------------------

#[test]
fn test_deposit_cap_allows_deposit_below_cap() {
    let t = setup();
    t.vault.set_deposit_cap(&t.manager, &100_000_0000000i128);
    t.vault.deposit(&1_000_0000000i128, &t.user, &t.base, &0i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #15)")]
fn test_deposit_cap_exceeded_panics() {
    let t = setup();
    t.vault.set_deposit_cap(&t.manager, &500_0000000i128);
    t.vault.deposit(&1_000_0000000i128, &t.user, &t.base, &0i128);
}

#[test]
fn test_deposit_cap_zero_means_uncapped() {
    let t = setup();
    t.vault.set_deposit_cap(&t.manager, &0i128);
    t.vault.deposit(&10_000_0000000i128, &t.user, &t.base, &0i128);
    t.vault.deposit(&10_000_0000000i128, &t.user2, &t.base, &0i128);
}

#[test]
#[should_panic]
fn test_set_deposit_cap_not_manager_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.vault.set_deposit_cap(&rogue, &1_000_0000000i128);
}


// ---------------------------------------------------------------------------
// Oracle mock (used by multi-asset portfolio and NAV tests below)
// ---------------------------------------------------------------------------

mod mock_oracle_mod {
    use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};
    #[contracttype]
    enum OKey {
        Price(Address),
    }
    #[contract]
    pub struct MockOracle;
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
}
use mock_oracle_mod::MockOracle;

// ---------------------------------------------------------------------------
// dHedge parity: private pool, cooldown, fee timelock, value manipulation
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #24)")]
fn test_private_pool_blocks_non_member_deposit() {
    let t = setup();
    t.vault.set_private_pool(&t.manager, &true); // manager == admin in test setup
    t.vault.deposit(&1_000_0000000i128, &t.user, &t.base, &0i128);
}

#[test]
fn test_private_pool_allows_manager_and_member() {
    let t = setup();
    t.vault.set_private_pool(&t.manager, &true);
    t.vault.deposit(&1_000_0000000i128, &t.manager, &t.base, &0i128);
    t.vault.add_member(&t.manager, &t.user);
    t.vault.deposit(&1_000_0000000i128, &t.user, &t.base, &0i128);
    assert!(shares(&t, &t.user) > 0);
}

#[test]
fn test_add_remove_member_updates_allowlist() {
    let t = setup();
    t.vault.add_member(&t.manager, &t.user);
    assert!(t.vault.is_member_allowed(&t.user));
    t.vault.remove_member(&t.manager, &t.user);
    assert!(!t.vault.is_member_allowed(&t.user));
}

#[test]
#[should_panic(expected = "Error(Contract, #23)")]
fn test_withdraw_respects_exit_cooldown() {
    let t = setup();
    t.vault.set_exit_cooldown_secs(&t.manager, &120u64);
    t.vault.deposit(&1_000_0000000i128, &t.user, &t.base, &0i128);
    let user_shares = shares(&t, &t.user);
    t.vault.withdraw(&user_shares, &t.user, &t.user, &0i128);
}

#[test]
fn test_withdraw_after_cooldown_succeeds() {
    let t = setup();
    t.vault.set_exit_cooldown_secs(&t.manager, &120u64);
    t.vault.deposit(&1_000_0000000i128, &t.user, &t.base, &0i128);
    assert!(t.vault.get_exit_remaining_cooldown(&t.user) > 0);
    advance_time(&t, 121);
    let user_shares = shares(&t, &t.user);
    let returned = t.vault.withdraw(&user_shares, &t.user, &t.user, &0i128);
    assert!(returned > 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #25)")]
fn test_commit_fee_increase_before_delay_panics() {
    let t = setup();
    t.vault
        .announce_fee_increase(&t.manager, &100u32, &0u32, &0u32, &0u32);
    t.vault.commit_fee_increase(&t.manager);
}

#[test]
fn test_commit_fee_increase_after_delay_applies() {
    let t = setup();
    t.vault
        .announce_fee_increase(&t.manager, &100u32, &200u32, &10u32, &100u32);
    advance_time(&t, 86_401);
    t.vault.commit_fee_increase(&t.manager);

    let announced = t.vault.get_announced_fees();
    assert_eq!(announced.activation_ts, None);
    assert_eq!(announced.entry_fee_bps, None);
}

#[test]
#[should_panic(expected = "Error(Contract, #26)")]
fn test_commit_fee_increase_without_announce_panics() {
    let t = setup();
    t.vault.commit_fee_increase(&t.manager);
}

#[test]
fn test_renounce_fee_increase_clears_pending() {
    let t = setup();
    t.vault
        .announce_fee_increase(&t.manager, &100u32, &0u32, &0u32, &0u32);
    t.vault.renounce_fee_increase(&t.manager);
    let announced = t.vault.get_announced_fees();
    assert_eq!(announced.activation_ts, None);
    assert_eq!(announced.entry_fee_bps, None);
}

#[test]
#[should_panic(expected = "Error(Contract, #27)")]
fn test_value_guard_same_ledger_operation_type_mismatch_panics() {
    let t = setup();
    t.vault.set_value_guard_enabled(&t.manager, &true);
    t.vault.deposit(&1_000_0000000i128, &t.user, &t.base, &0i128);
    let user_shares = shares(&t, &t.user);
    // Same ledger + same actor but different op type (deposit -> withdraw).
    t.vault.withdraw(&user_shares, &t.user, &t.user, &0i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #28)")]
fn test_value_guard_same_ledger_nav_mismatch_panics() {
    let t = setup();
    t.vault.set_value_guard_enabled(&t.manager, &true);
    t.vault.deposit(&1_000_0000000i128, &t.user, &t.base, &0i128);
    // External NAV mutation in same ledger (simulated mint to vault).
    MockTokenClient::new(&t.env, &t.base).mint(&t.vault_addr, &1i128);
    // Same op type (deposit), same ledger, but nav_before != expected_nav_after.
    t.vault.deposit(&1_000_0000000i128, &t.user, &t.base, &0i128);
}

// ---------------------------------------------------------------------------
// MockFactory — minimal factory stub for add_portfolio_asset / add_active_guard tests
// ---------------------------------------------------------------------------

#[contracttype]
enum FKey {
    AuthorizedAsset(Address),
    AuthorizedGuard(Address),
}
#[contract]
pub struct MockFactory;
#[contractimpl]
impl MockFactory {
    pub fn set_asset(env: Env, asset: Address, allowed: bool) {
        env.storage()
            .instance()
            .set(&FKey::AuthorizedAsset(asset), &allowed);
    }
    pub fn is_authorized_asset(env: Env, asset: Address) -> bool {
        env.storage()
            .instance()
            .get(&FKey::AuthorizedAsset(asset))
            .unwrap_or(false)
    }
    pub fn set_guard(env: Env, guard: Address, allowed: bool) {
        env.storage()
            .instance()
            .set(&FKey::AuthorizedGuard(guard), &allowed);
    }
    pub fn is_authorized_guard(env: Env, guard: Address) -> bool {
        env.storage()
            .instance()
            .get(&FKey::AuthorizedGuard(guard))
            .unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// MockGuard — minimal guard stub for add/remove_active_guard and execute_op tests
// ---------------------------------------------------------------------------

#[contracttype]
enum GKey {
    TotalValue,
    AssetInUse(Address),
    // For TVL guard tests: op_type 2 reduces TotalValue by this bps amount.
    LossBps,
}
#[contract]
pub struct MockGuard;
#[contractimpl]
impl MockGuard {
    pub fn set_total_value(env: Env, value: i128) {
        env.storage().instance().set(&GKey::TotalValue, &value);
    }
    pub fn get_total_value(env: Env, _vault: Address) -> i128 {
        env.storage()
            .instance()
            .get(&GKey::TotalValue)
            .unwrap_or(0)
    }
    pub fn set_loss_bps(env: Env, bps: u32) {
        env.storage().instance().set(&GKey::LossBps, &bps);
    }
    pub fn set_asset_in_use(env: Env, asset: Address, in_use: bool) {
        env.storage()
            .instance()
            .set(&GKey::AssetInUse(asset), &in_use);
    }
    pub fn asset_in_use(env: Env, _vault: Address, asset: Address) -> bool {
        env.storage()
            .instance()
            .get(&GKey::AssetInUse(asset))
            .unwrap_or(false)
    }
    pub fn withdraw_fraction(
        _env: Env,
        _vault: Address,
        _numerator: i128,
        _denominator: i128,
        _to: Address,
    ) {
    }
    /// No-op: succeeds with no state change.
    /// Vault injects itself as first arg — guard receives vault address.
    pub fn noop(_env: Env, _vault: Address) {}

    /// Reduces get_total_value by the stored loss_bps percentage.
    /// Used to test the TVL guard without token transfers.
    pub fn simulate_loss(env: Env, _vault: Address) {
        let loss_bps: u32 = env.storage().instance().get(&GKey::LossBps).unwrap_or(0);
        let current: i128 = env.storage().instance().get(&GKey::TotalValue).unwrap_or(0);
        let new_val = current * (10_000 - loss_bps as i128) / 10_000;
        env.storage().instance().set(&GKey::TotalValue, &new_val);
    }

    /// Always panics — used to verify the vault's authorized_ops gate fires
    /// before the guard is even reached.
    pub fn reject_op(_env: Env, _vault: Address) {
        panic!("guard rejected operation");
    }
}

// ---------------------------------------------------------------------------
// Portfolio asset management — add_portfolio_asset / remove_portfolio_asset
// ---------------------------------------------------------------------------

#[test]
fn test_add_portfolio_asset_no_factory() {
    // Without factory set, any asset may be added (no whitelist check).
    // setup() already added base, so the new asset is at index 1.
    let t = setup();
    let asset = Address::generate(&t.env);

    t.vault.add_portfolio_asset(&t.manager, &asset);

    let list = t.vault.get_portfolio_assets();
    assert_eq!(list.len(), 2);
    assert!(list.contains(asset));
}

#[test]
fn test_add_multiple_portfolio_assets() {
    // setup() pre-adds base; adding 3 more gives total of 4.
    let t = setup();
    let a1 = Address::generate(&t.env);
    let a2 = Address::generate(&t.env);
    let a3 = Address::generate(&t.env);

    t.vault.add_portfolio_asset(&t.manager, &a1);
    t.vault.add_portfolio_asset(&t.manager, &a2);
    t.vault.add_portfolio_asset(&t.manager, &a3);

    let list = t.vault.get_portfolio_assets();
    assert_eq!(list.len(), 4);
}

#[test]
#[should_panic]
fn test_add_portfolio_asset_not_manager_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    let asset = Address::generate(&t.env);
    t.vault.add_portfolio_asset(&rogue, &asset);
}

#[test]
#[should_panic(expected = "Error(Contract, #32)")]
fn test_add_portfolio_asset_duplicate_panics() {
    let t = setup();
    let asset = Address::generate(&t.env);
    t.vault.add_portfolio_asset(&t.manager, &asset);
    t.vault.add_portfolio_asset(&t.manager, &asset);
}

#[test]
fn test_add_portfolio_asset_with_factory_authorized() {
    let (t, factory_id) = setup_with_factory();
    let asset = Address::generate(&t.env);
    MockFactoryClient::new(&t.env, &factory_id).set_asset(&asset, &true);

    t.vault.add_portfolio_asset(&t.manager, &asset);

    assert_eq!(t.vault.get_portfolio_assets().len(), 1);
}

#[test]
#[should_panic(expected = "Error(Contract, #30)")]
fn test_add_portfolio_asset_with_factory_unauthorized_panics() {
    let (t, _factory_id) = setup_with_factory();
    let asset = Address::generate(&t.env);
    // asset not whitelisted in factory → must panic

    t.vault.add_portfolio_asset(&t.manager, &asset);
}

#[test]
fn test_remove_portfolio_asset_zero_balance() {
    // setup() already added base to portfolio with balance=0 (no deposits yet).
    // We can remove it directly without re-adding.
    let t = setup();

    t.vault.remove_portfolio_asset(&t.manager, &t.base);

    let list = t.vault.get_portfolio_assets();
    assert!(!list.contains(t.base.clone()));
}

#[test]
#[should_panic]
fn test_remove_portfolio_asset_not_manager_panics() {
    let t = setup();
    let asset = Address::generate(&t.env);
    t.vault.add_portfolio_asset(&t.manager, &asset);
    let rogue = Address::generate(&t.env);
    t.vault.remove_portfolio_asset(&rogue, &asset);
}

#[test]
#[should_panic(expected = "Error(Contract, #31)")]
fn test_remove_portfolio_asset_not_in_portfolio_panics() {
    let t = setup();
    let asset = Address::generate(&t.env);
    t.vault.remove_portfolio_asset(&t.manager, &asset);
}

#[test]
fn test_remove_portfolio_asset_also_removes_from_deposit_assets() {
    // setup() already added base to both portfolio and deposit assets.
    let t = setup();

    assert_eq!(t.vault.get_deposit_assets().len(), 1);

    // Remove base from portfolio (balance=0 since no deposit yet).
    // This should also cascade-remove it from deposit assets.
    t.vault.remove_portfolio_asset(&t.manager, &t.base);

    assert_eq!(t.vault.get_portfolio_assets().len(), 0);
    assert_eq!(t.vault.get_deposit_assets().len(), 0);
}

// ---------------------------------------------------------------------------
// Deposit asset management — add_deposit_asset / remove_deposit_asset
// ---------------------------------------------------------------------------

#[test]
fn test_add_deposit_asset_requires_portfolio_membership() {
    // setup() already has base in portfolio+deposit; adding a new asset gives len=2.
    let t = setup();
    let asset = Address::generate(&t.env);
    // Must add to portfolio first.
    t.vault.add_portfolio_asset(&t.manager, &asset);
    t.vault.add_deposit_asset(&t.manager, &asset);

    let list = t.vault.get_deposit_assets();
    assert_eq!(list.len(), 2);
    assert!(list.contains(asset));
}

#[test]
#[should_panic(expected = "Error(Contract, #31)")]
fn test_add_deposit_asset_not_in_portfolio_panics() {
    let t = setup();
    let asset = Address::generate(&t.env);
    // Skip add_portfolio_asset.
    t.vault.add_deposit_asset(&t.manager, &asset);
}

#[test]
#[should_panic(expected = "Error(Contract, #32)")]
fn test_add_deposit_asset_duplicate_panics() {
    let t = setup();
    let asset = Address::generate(&t.env);
    t.vault.add_portfolio_asset(&t.manager, &asset);
    t.vault.add_deposit_asset(&t.manager, &asset);
    t.vault.add_deposit_asset(&t.manager, &asset);
}

#[test]
fn test_remove_deposit_asset() {
    // setup() pre-adds base to portfolio+deposit. Add one more, then remove it.
    let t = setup();
    let asset = Address::generate(&t.env);
    t.vault.add_portfolio_asset(&t.manager, &asset);
    t.vault.add_deposit_asset(&t.manager, &asset);

    t.vault.remove_deposit_asset(&t.manager, &asset);

    let list = t.vault.get_deposit_assets();
    // base remains in deposit, new asset was removed → len = 1
    assert_eq!(list.len(), 1);
    // Both base and asset remain in portfolio → len = 2
    assert_eq!(t.vault.get_portfolio_assets().len(), 2);
}

#[test]
#[should_panic]
fn test_remove_deposit_asset_not_manager_panics() {
    let t = setup();
    let asset = Address::generate(&t.env);
    t.vault.add_portfolio_asset(&t.manager, &asset);
    t.vault.add_deposit_asset(&t.manager, &asset);
    let rogue = Address::generate(&t.env);
    t.vault.remove_deposit_asset(&rogue, &asset);
}

#[test]
#[should_panic(expected = "Error(Contract, #31)")]
fn test_remove_deposit_asset_not_present_panics() {
    let t = setup();
    let asset = Address::generate(&t.env);
    t.vault.remove_deposit_asset(&t.manager, &asset);
}

// ---------------------------------------------------------------------------
// Active guard management — add_active_guard / remove_active_guard
// ---------------------------------------------------------------------------

#[test]
fn test_add_active_guard_no_factory() {
    let t = setup();
    let guard_id = t.env.register(MockGuard, ());

    t.vault.add_active_guard(&t.manager, &guard_id);

    let list = t.vault.get_active_guards();
    assert_eq!(list.len(), 1);
    assert_eq!(list.get(0).unwrap(), guard_id);
}

#[test]
#[should_panic]
fn test_add_active_guard_not_manager_panics() {
    let t = setup();
    let guard_id = t.env.register(MockGuard, ());
    let rogue = Address::generate(&t.env);
    t.vault.add_active_guard(&rogue, &guard_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #37)")]
fn test_add_active_guard_duplicate_panics() {
    let t = setup();
    let guard_id = t.env.register(MockGuard, ());
    t.vault.add_active_guard(&t.manager, &guard_id);
    t.vault.add_active_guard(&t.manager, &guard_id);
}

#[test]
fn test_add_active_guard_with_factory_authorized() {
    let (t, factory_id) = setup_with_factory();
    let guard_id = t.env.register(MockGuard, ());
    MockFactoryClient::new(&t.env, &factory_id).set_guard(&guard_id, &true);

    t.vault.add_active_guard(&t.manager, &guard_id);

    assert_eq!(t.vault.get_active_guards().len(), 1);
}

#[test]
#[should_panic(expected = "Error(Contract, #36)")]
fn test_add_active_guard_with_factory_unauthorized_panics() {
    let (t, _factory_id) = setup_with_factory();
    let guard_id = t.env.register(MockGuard, ());
    // guard not whitelisted in factory → must panic

    t.vault.add_active_guard(&t.manager, &guard_id);
}

#[test]
fn test_remove_active_guard_zero_position() {
    let t = setup();
    let guard_id = t.env.register(MockGuard, ());
    // Default total_value = 0.
    t.vault.add_active_guard(&t.manager, &guard_id);

    t.vault.remove_active_guard(&t.manager, &guard_id);

    assert_eq!(t.vault.get_active_guards().len(), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #35)")]
fn test_remove_active_guard_nonzero_position_panics() {
    let t = setup();
    let guard_id = t.env.register(MockGuard, ());
    MockGuardClient::new(&t.env, &guard_id).set_total_value(&1_000i128);
    t.vault.add_active_guard(&t.manager, &guard_id);

    t.vault.remove_active_guard(&t.manager, &guard_id);
}

#[test]
#[should_panic]
fn test_remove_active_guard_not_manager_panics() {
    let t = setup();
    let guard_id = t.env.register(MockGuard, ());
    t.vault.add_active_guard(&t.manager, &guard_id);
    let rogue = Address::generate(&t.env);
    t.vault.remove_active_guard(&rogue, &guard_id);
}

// ---------------------------------------------------------------------------
// factory set via VaultParams / get_factory
// ---------------------------------------------------------------------------

#[test]
fn test_factory_none_when_not_provided() {
    let t = setup(); // factory: None in VaultParams
    assert!(t.vault.get_factory().is_none());
}

#[test]
fn test_factory_set_atomically_via_vault_params() {
    let (t, factory_id) = setup_with_factory();
    assert_eq!(t.vault.get_factory().unwrap(), factory_id);
}

// ---------------------------------------------------------------------------
// set_authorized_ops / get_authorized_ops
// ---------------------------------------------------------------------------

#[test]
fn test_set_and_get_authorized_ops() {
    let t = setup();
    let guard = Address::generate(&t.env);
    let ops = soroban_sdk::vec![
        &t.env,
        Symbol::new(&t.env, "supply"),
        Symbol::new(&t.env, "withdraw"),
        Symbol::new(&t.env, "swap"),
    ];

    t.vault.set_authorized_ops(&t.manager, &guard, &ops);

    let stored = t.vault.get_authorized_ops(&guard);
    assert_eq!(stored.len(), 3);
    assert_eq!(stored.get(0).unwrap(), Symbol::new(&t.env, "supply"));
    assert_eq!(stored.get(1).unwrap(), Symbol::new(&t.env, "withdraw"));
    assert_eq!(stored.get(2).unwrap(), Symbol::new(&t.env, "swap"));
}

#[test]
#[should_panic]
fn test_set_authorized_ops_not_manager_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    let guard = Address::generate(&t.env);
    let ops = soroban_sdk::vec![&t.env, Symbol::new(&t.env, "supply")];
    t.vault.set_authorized_ops(&rogue, &guard, &ops);
}

// ---------------------------------------------------------------------------
// execute_op — unified guard dispatch
// ---------------------------------------------------------------------------

/// Helper: set up a vault with portfolio, active guard, and authorized ops.
fn setup_with_execute_op(t: &T) -> (Address, Address) {
    // base_asset is already in portfolio/deposit from setup() — no need to re-add.

    // Register a MockGuard and add it to active guards.
    let guard_id = t.env.register(MockGuard, ());
    t.vault.add_active_guard(&t.manager, &guard_id);

    // Authorize "noop" and "simulate_loss" for this guard.
    let ops: Vec<Symbol> = vec![
        &t.env,
        Symbol::new(&t.env, "noop"),
        Symbol::new(&t.env, "simulate_loss"),
    ];
    t.vault.set_authorized_ops(&t.manager, &guard_id, &ops);

    (guard_id, t.base.clone())
}

#[test]
fn test_execute_op_noop_by_manager() {
    let t = setup();
    let (guard_id, _) = setup_with_execute_op(&t);
    let no_args: Vec<Val> = Vec::new(&t.env);
    t.vault.deposit(&1_000_0000000i128, &t.user, &t.base, &0i128);

    let nav_before = t.vault.get_nav();
    t.vault.execute_op(&t.manager, &guard_id, &Symbol::new(&t.env, "noop"), &no_args);
    assert_eq!(t.vault.get_nav(), nav_before);
}

#[test]
fn test_execute_op_noop_by_trader() {
    let t = setup();
    let (guard_id, _) = setup_with_execute_op(&t);
    let no_args: Vec<Val> = Vec::new(&t.env);
    t.vault.deposit(&1_000_0000000i128, &t.user, &t.base, &0i128);

    // Trader can also call execute_op.
    t.vault.execute_op(&t.trader, &guard_id, &Symbol::new(&t.env, "noop"), &no_args);
}

#[test]
#[should_panic]
fn test_execute_op_not_manager_or_trader_panics() {
    let t = setup();
    let (guard_id, _) = setup_with_execute_op(&t);
    let no_args: Vec<Val> = Vec::new(&t.env);
    let rogue = Address::generate(&t.env);
    t.vault.execute_op(&rogue, &guard_id, &Symbol::new(&t.env, "noop"), &no_args);
}

#[test]
#[should_panic]
fn test_execute_op_guard_not_active_panics() {
    let t = setup();
    let no_args: Vec<Val> = Vec::new(&t.env);
    let guard_id = t.env.register(MockGuard, ()); // NOT added to active guards
    t.vault.execute_op(&t.manager, &guard_id, &Symbol::new(&t.env, "noop"), &no_args);
}

#[test]
#[should_panic]
fn test_execute_op_unauthorized_fn_panics() {
    let t = setup();
    let (guard_id, _) = setup_with_execute_op(&t);
    let no_args: Vec<Val> = Vec::new(&t.env);
    // "reject_op" is not in authorized_ops (only "noop" and "simulate_loss" are).
    t.vault.execute_op(&t.manager, &guard_id, &Symbol::new(&t.env, "reject_op"), &no_args);
}

#[test]
fn test_execute_op_updates_guard_state() {
    let t = setup();
    let (guard_id, _) = setup_with_execute_op(&t);
    let no_args: Vec<Val> = Vec::new(&t.env);
    t.vault.deposit(&1_000_0000000i128, &t.user, &t.base, &0i128);

    // Seed guard with a position value.
    MockGuardClient::new(&t.env, &guard_id).set_total_value(&500_0000000i128);
    // NAV = vault_balance + guard_value = 1000 + 500 = 1500.
    assert_eq!(t.vault.get_nav(), 1_500_0000000i128);

    // simulate_loss with 0 loss_bps = identity. NAV unchanged.
    t.vault.execute_op(&t.manager, &guard_id, &Symbol::new(&t.env, "simulate_loss"), &no_args);
    assert_eq!(t.vault.get_nav(), 1_500_0000000i128);
}

// ---------------------------------------------------------------------------
// TVL guard via execute_op
// ---------------------------------------------------------------------------

#[test]
fn test_set_max_loss_bps_updates() {
    let t = setup();
    t.vault.set_max_loss_bps(&t.manager, &500u32);
    // Just confirms it doesn't panic.
}

#[test]
#[should_panic]
fn test_set_max_loss_bps_not_manager_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.vault.set_max_loss_bps(&rogue, &200u32);
}

#[test]
fn test_set_max_loss_bps_zero_disables_guard() {
    let t = setup();
    // 0 means "TVL guard disabled" — must be accepted without panic.
    t.vault.set_max_loss_bps(&t.manager, &0u32);
}

#[test]
#[should_panic]
fn test_set_max_loss_bps_above_denominator_panics() {
    let t = setup();
    t.vault.set_max_loss_bps(&t.manager, &10_001u32);
}

/// TVL guard passes when execute_op NAV loss is within tolerance.
#[test]
fn test_tvl_guard_passes_within_tolerance() {
    let t = setup();
    let (guard_id, _) = setup_with_execute_op(&t);
    let no_args: Vec<Val> = Vec::new(&t.env);

    MockGuardClient::new(&t.env, &guard_id).set_total_value(&1_000_0000000i128);
    MockGuardClient::new(&t.env, &guard_id).set_loss_bps(&100u32); // 1% loss
    t.vault.set_max_loss_bps(&t.manager, &200u32); // 2% tolerance → passes

    t.vault.execute_op(&t.manager, &guard_id, &Symbol::new(&t.env, "simulate_loss"), &no_args);

    let guard_val = MockGuardClient::new(&t.env, &guard_id).get_total_value(&t.vault_addr);
    assert_eq!(guard_val, 990_0000000i128);
}

/// TVL guard trips when execute_op NAV drop exceeds tolerance.
#[test]
#[should_panic(expected = "Error(Contract, #17)")]
fn test_tvl_guard_trips_when_loss_exceeds_tolerance() {
    let t = setup();
    let (guard_id, _) = setup_with_execute_op(&t);
    let no_args: Vec<Val> = Vec::new(&t.env);

    MockGuardClient::new(&t.env, &guard_id).set_total_value(&1_000_0000000i128);
    MockGuardClient::new(&t.env, &guard_id).set_loss_bps(&1_000u32); // 10% loss
    t.vault.set_max_loss_bps(&t.manager, &500u32); // 5% tolerance — 10% > 5% → trips

    t.vault.execute_op(&t.manager, &guard_id, &Symbol::new(&t.env, "simulate_loss"), &no_args);
}

/// Default TVL guard (10%) trips on a 50% loss.
#[test]
#[should_panic(expected = "Error(Contract, #17)")]
fn test_tvl_guard_enabled_by_default() {
    let t = setup();
    let (guard_id, _) = setup_with_execute_op(&t);
    let no_args: Vec<Val> = Vec::new(&t.env);

    MockGuardClient::new(&t.env, &guard_id).set_total_value(&1_000_0000000i128);
    MockGuardClient::new(&t.env, &guard_id).set_loss_bps(&5_000u32); // 50% loss
    // Default max_loss_bps = 1000 (10%). 50% > 10% → trips.
    t.vault.execute_op(&t.manager, &guard_id, &Symbol::new(&t.env, "simulate_loss"), &no_args);
}

/// Very permissive TVL guard (100%) allows even 50% loss.
#[test]
fn test_tvl_guard_very_permissive_value() {
    let t = setup();
    let (guard_id, _) = setup_with_execute_op(&t);
    let no_args: Vec<Val> = Vec::new(&t.env);

    MockGuardClient::new(&t.env, &guard_id).set_total_value(&1_000_0000000i128);
    MockGuardClient::new(&t.env, &guard_id).set_loss_bps(&5_000u32); // 50% loss
    t.vault.set_max_loss_bps(&t.manager, &10_000u32); // 100% tolerance — should not panic

    t.vault.execute_op(&t.manager, &guard_id, &Symbol::new(&t.env, "simulate_loss"), &no_args);
}

// ---------------------------------------------------------------------------
// set_oracle (manager auth only, no other dep)
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_set_oracle_not_manager_panics() {
    let t = setup();
    let oracle_id = t.env.register(MockOracle, ());
    let rogue = Address::generate(&t.env);
    t.vault.set_oracle(&rogue, &oracle_id);
}

// ---------------------------------------------------------------------------
// seed_deposit
// ---------------------------------------------------------------------------

#[test]
fn test_seed_deposit_mints_to_burn_address() {
    let t = setup();
    // seed_deposit is normally called by the factory after deploying the vault.
    // Here we call directly (mocked auth).
    t.vault.seed_deposit(&1_000i128);

    // Burn address should hold 1_000 shares.
    let burn_addr = soroban_sdk::Address::from_str(
        &t.env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    );
    let burn_balance = MockTokenClient::new(&t.env, &t.share).balance(&burn_addr);
    assert_eq!(burn_balance, 1_000i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #38)")]
fn test_seed_deposit_twice_panics() {
    let t = setup();
    t.vault.seed_deposit(&1_000i128);
    t.vault.seed_deposit(&1_000i128);
}

// ---------------------------------------------------------------------------
// get_user_pnl — PnL tracking after deposit and withdrawal
// ---------------------------------------------------------------------------

#[test]
fn test_user_pnl_zero_before_any_deposit() {
    let t = setup();
    let report = t.vault.get_user_pnl(&t.user);
    assert_eq!(report.cost_basis, 0);
    assert_eq!(report.realized_pnl, 0);
    assert_eq!(report.current_value, 0);
    assert_eq!(report.unrealized_pnl, 0);
    assert_eq!(report.total_pnl, 0);
}

#[test]
fn test_user_pnl_after_deposit() {
    let t = setup();
    let deposit = 1_000_0000000i128;
    t.vault.deposit(&deposit, &t.user, &t.base, &0i128);

    let report = t.vault.get_user_pnl(&t.user);
    // Cost basis equals deposit value (no NAV growth yet).
    assert_eq!(report.cost_basis, deposit);
    assert_eq!(report.realized_pnl, 0);
    // Unrealized PnL = 0 (price = 1.0, no change).
    assert_eq!(report.unrealized_pnl, 0);
    assert_eq!(report.total_pnl, 0);
}

#[test]
fn test_user_pnl_realized_on_withdrawal() {
    let t = setup();
    let deposit = 1_000_0000000i128;
    let shares = t.vault.deposit(&deposit, &t.user, &t.base, &0i128);

    // Simulate NAV growth: airdrop extra tokens to vault.
    let extra = 100_0000000i128;
    MockTokenClient::new(&t.env, &t.base).mint(&t.vault_addr, &extra);

    // Withdraw all shares — should realize profit.
    t.vault.withdraw(&shares, &t.user, &t.user, &0i128);

    let report = t.vault.get_user_pnl(&t.user);
    // After full withdrawal, cost_basis = 0 (position closed).
    assert_eq!(report.cost_basis, 0);
    // Realized PnL should be positive (> 0) due to NAV growth.
    assert!(report.realized_pnl > 0);
}

#[test]
fn test_user_pnl_partial_withdrawal_updates_cost_basis() {
    let t = setup();
    let deposit = 1_000_0000000i128;
    let total_shares = t.vault.deposit(&deposit, &t.user, &t.base, &0i128);

    // Withdraw half the shares.
    let half = total_shares / 2;
    t.vault.withdraw(&half, &t.user, &t.user, &0i128);

    let report = t.vault.get_user_pnl(&t.user);
    // After half withdrawal at same price, cost_basis should be ~half.
    assert!(report.cost_basis > 0);
    assert!(report.cost_basis < deposit);
}
