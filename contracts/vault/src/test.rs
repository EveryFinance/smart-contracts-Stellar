#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger as _},
    vec, Address, Env, String, Vec,
};

use crate::{Vault, VaultClient, VaultParams};

// ---------------------------------------------------------------------------
// MockToken — minimal SEP-41 for testing
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
            manager: manager.clone(),
            trader: trader.clone(),
            base_asset: base.clone(),
            share_token: share.clone(),
            share_token_admin: manager.clone(),
            entry_fee_bps: entry,
            exit_fee_bps: exit,
            mgmt_fee_bps: mgmt,
            perf_fee_bps: perf,
        },),
    );
    let vault = VaultClient::new(&env, &vid);

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
    assert_eq!(t.vault.get_strategies().len(), 0);
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
            manager: manager.clone(),
            trader: trader.clone(),
            base_asset: base,
            share_token: share,
            share_token_admin: manager.clone(),
            entry_fee_bps: 501, // > MAX
            exit_fee_bps: 0,
            mgmt_fee_bps: 0,
            perf_fee_bps: 0,
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
    let shares_minted = t.vault.deposit(&amount, &t.user, &0i128);
    // Bootstrap: shares_minted = net_amount = amount (no fees).
    assert_eq!(shares_minted, amount);
    assert_eq!(shares(&t, &t.user), amount);
    assert_eq!(total_supply(&t), amount);
}

#[test]
fn test_deposit_with_entry_fee() {
    let t = setup_with_fees(100, 0, 0, 0); // 1% entry
    let amount = 1_000_0000000i128;
    let user_shares = t.vault.deposit(&amount, &t.user, &0i128);
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
    t.vault.deposit(&first, &t.user, &0i128);

    let second = 500_0000000i128;
    let shares2 = t.vault.deposit(&second, &t.user2, &0i128);
    // NAV = first, total_supply = first → price = 1.0
    // shares2 should equal second.
    assert_eq!(shares2, second);
}

#[test]
#[should_panic]
fn test_deposit_paused_panics() {
    let t = setup();
    t.vault.pause(&t.manager);
    t.vault.deposit(&1_000i128, &t.user, &0i128);
}

#[test]
#[should_panic]
fn test_deposit_zero_panics() {
    let t = setup();
    t.vault.deposit(&0i128, &t.user, &0i128);
}

// ---------------------------------------------------------------------------
// Withdraw tests
// ---------------------------------------------------------------------------

#[test]
fn test_withdraw_burns_shares_returns_base() {
    let t = setup();
    let amount = 1_000_0000000i128;
    t.vault.deposit(&amount, &t.user, &0i128);

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
    t.vault.deposit(&amount, &t.user, &0i128);

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
    t.vault.deposit(&amount, &t.user, &0i128);
    t.vault.withdraw(&(amount + 1), &t.user, &t.user, &0i128);
}

#[test]
#[should_panic]
fn test_withdraw_paused_panics() {
    let t = setup();
    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    t.vault.pause(&t.manager);
    t.vault.withdraw(&1i128, &t.user, &t.user, &0i128);
}

#[test]
#[should_panic]
fn test_withdraw_zero_panics() {
    let t = setup();
    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    t.vault.withdraw(&0i128, &t.user, &t.user, &0i128);
}

// ---------------------------------------------------------------------------
// Strategy management tests
// ---------------------------------------------------------------------------

#[test]
fn test_set_strategies_by_manager() {
    let t = setup();
    let strategy = Address::generate(&t.env);
    let strategies: Vec<Address> = vec![&t.env, strategy.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    assert_eq!(t.vault.get_strategies().len(), 1);
}

#[test]
#[should_panic]
fn test_set_strategies_not_manager_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    let strategies: Vec<Address> = vec![&t.env, Address::generate(&t.env)];
    t.vault.set_strategies(&rogue, &strategies);
}

// ---------------------------------------------------------------------------
// Invest / Unwind tests
// ---------------------------------------------------------------------------

#[test]
fn test_invest_and_unwind() {
    let t = setup();

    // Deploy a mock strategy.
    let sid = env_register_strategy(&t);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);

    // Deposit to vault.
    let deposit_amount = 1_000_0000000i128;
    t.vault.deposit(&deposit_amount, &t.user, &0i128);

    let vault_base_before = base(&t, &t.vault_addr);
    assert_eq!(vault_base_before, deposit_amount);

    // Invest half.
    let invest_amount = 500_0000000i128;
    t.vault.invest(&t.manager, &sid, &invest_amount);

    assert_eq!(base(&t, &t.vault_addr), deposit_amount - invest_amount);
    // NAV should remain the same (strategy reports the invested value).
    let nav = t.vault.get_nav();
    assert_eq!(nav, deposit_amount);

    // Unwind.
    t.vault.unwind(&t.manager, &sid, &invest_amount);
    assert_eq!(base(&t, &t.vault_addr), deposit_amount);
}

#[test]
#[should_panic]
fn test_invest_not_manager_panics() {
    let t = setup();
    let sid = env_register_strategy(&t);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    let rogue = Address::generate(&t.env);
    t.vault.invest(&rogue, &sid, &100i128);
}

#[test]
#[should_panic]
fn test_invest_strategy_not_whitelisted_panics() {
    let t = setup();
    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    let unlisted = Address::generate(&t.env);
    t.vault.invest(&t.manager, &unlisted, &100i128);
}

// ---------------------------------------------------------------------------
// Pause / Unpause tests
// ---------------------------------------------------------------------------

#[test]
fn test_pause_unpause() {
    let t = setup();
    t.vault.pause(&t.manager);
    assert!(t.vault.is_paused());
    t.vault.unpause(&t.manager);
    assert!(!t.vault.is_paused());
}

#[test]
#[should_panic]
fn test_pause_not_manager_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.vault.pause(&rogue);
}

// ---------------------------------------------------------------------------
// NAV / Share-price tests
// ---------------------------------------------------------------------------

#[test]
fn test_nav_and_share_price_without_strategies() {
    let t = setup();
    let amount = 1_000_0000000i128;
    t.vault.deposit(&amount, &t.user, &0i128);
    assert_eq!(t.vault.get_nav(), amount);
    // share_price = NAV * PRICE_PRECISION / total_supply = 1.0
    assert_eq!(t.vault.get_share_price(), 10_000_000i128);
}

#[test]
fn test_full_lifecycle() {
    let t = setup();

    let sid = env_register_strategy(&t);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);

    // Two users deposit.
    let dep1 = 1_000_0000000i128;
    let dep2 = 500_0000000i128;
    let s1 = t.vault.deposit(&dep1, &t.user, &0i128);
    let s2 = t.vault.deposit(&dep2, &t.user2, &0i128);

    assert_eq!(total_supply(&t), s1 + s2);
    assert_eq!(t.vault.get_nav(), dep1 + dep2);

    // Manager invests.
    t.vault.invest(&t.manager, &sid, &800_0000000i128);

    // NAV unchanged.
    assert_eq!(t.vault.get_nav(), dep1 + dep2);

    // Unwind strategy.
    t.vault.unwind(&t.manager, &sid, &800_0000000i128);

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
// Helpers
// ---------------------------------------------------------------------------

fn env_register_strategy(t: &T) -> Address {
    let sid = t.env.register(MockStrategy, ());
    MockStrategyClient::new(&t.env, &sid).init(&t.base);
    sid
}

// ---------------------------------------------------------------------------
// MockDexStrategy — strategy that implements execute_trade for spot swaps
// Wrapped in its own mod to avoid symbol conflicts with MockStrategy.
// ---------------------------------------------------------------------------

mod mock_dex_strategy_mod {
    use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Vec};
    #[contracttype]
    pub enum DKey {
        BaseAsset,
        LastAmountIn,
    }
    #[contract]
    pub struct MockDexStrategy;
    #[contractimpl]
    impl MockDexStrategy {
        pub fn init(env: Env, base_asset: Address) {
            env.storage().instance().set(&DKey::BaseAsset, &base_asset);
        }
        pub fn get_value(_env: Env, _vault: Address) -> i128 {
            0i128
        }
        pub fn quote_exact_in(_env: Env, amount_in: i128, _path: Vec<Address>) -> i128 {
            amount_in
        }
        pub fn execute_trade(
            env: Env,
            amount_in: i128,
            _min_out: i128,
            _path: Vec<Address>,
            _vault: Address,
        ) -> i128 {
            env.storage()
                .instance()
                .set(&DKey::LastAmountIn, &amount_in);
            amount_in
        }
    }
}
use mock_dex_strategy_mod::{MockDexStrategy, MockDexStrategyClient};

// ---------------------------------------------------------------------------
// MockPassGuard — validates all trades (always passes)
// Each guard lives in its own mod to avoid __validate_swap_exact_in conflict.
// ---------------------------------------------------------------------------

mod mock_pass_guard_mod {
    use soroban_sdk::{contract, contractimpl, Address, Env, Vec};
    #[contract]
    pub struct MockPassGuard;
    #[contractimpl]
    impl MockPassGuard {
        pub fn validate_swap_exact_in(
            _env: Env,
            _caller: Address,
            _amount_in: i128,
            _min_out: i128,
            _path: Vec<Address>,
        ) {
            // Always passes.
        }
    }
}
use mock_pass_guard_mod::MockPassGuard;

// ---------------------------------------------------------------------------
// MockRejectGuard — rejects all trades
// ---------------------------------------------------------------------------

mod mock_reject_guard_mod {
    use soroban_sdk::{contract, contractimpl, Address, Env, Vec};
    #[contract]
    pub struct MockRejectGuard;
    #[contractimpl]
    impl MockRejectGuard {
        pub fn validate_swap_exact_in(
            _env: Env,
            _caller: Address,
            _amount_in: i128,
            _min_out: i128,
            _path: Vec<Address>,
        ) {
            panic!("guard rejected");
        }
    }
}
use mock_reject_guard_mod::MockRejectGuard;

fn register_dex_strategy(t: &T) -> Address {
    let sid = t.env.register(MockDexStrategy, ());
    MockDexStrategyClient::new(&t.env, &sid).init(&t.base);
    sid
}

fn register_pass_guard(t: &T) -> Address {
    t.env.register(MockPassGuard, ())
}

fn register_reject_guard(t: &T) -> Address {
    t.env.register(MockRejectGuard, ())
}

// ---------------------------------------------------------------------------
// set_trade_guard tests
// ---------------------------------------------------------------------------

#[test]
fn test_set_trade_guard_by_manager() {
    let t = setup();
    let sid = register_dex_strategy(&t);
    let guard = register_pass_guard(&t);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    t.vault.set_trade_guard(&t.manager, &sid, &guard);
    // No panic — trade guard was set.
}

#[test]
#[should_panic]
fn test_set_trade_guard_not_manager_panics() {
    let t = setup();
    let sid = register_dex_strategy(&t);
    let guard = register_pass_guard(&t);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    let rogue = Address::generate(&t.env);
    t.vault.set_trade_guard(&rogue, &sid, &guard);
}

#[test]
#[should_panic]
fn test_set_trade_guard_strategy_not_whitelisted_panics() {
    let t = setup();
    let sid = register_dex_strategy(&t);
    let guard = register_pass_guard(&t);
    // Did NOT call set_strategies → strategy not whitelisted.
    t.vault.set_trade_guard(&t.manager, &sid, &guard);
}

// ---------------------------------------------------------------------------
// execute_trade tests
// ---------------------------------------------------------------------------

#[test]
fn test_execute_trade_valid() {
    let t = setup();
    let sid = register_dex_strategy(&t);
    let guard = register_pass_guard(&t);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    t.vault.set_trade_guard(&t.manager, &sid, &guard);

    // Fund vault so it has base tokens.
    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);

    let token_a = Address::generate(&t.env);
    let token_b = Address::generate(&t.env);
    let path: Vec<Address> = vec![&t.env, token_a, token_b];

    let result = t
        .vault
        .execute_trade(&t.trader, &sid, &100_0000000i128, &90_0000000i128, &path);
    assert_eq!(result, 100_0000000i128); // MockDexStrategy returns amount_in
}

#[test]
#[should_panic]
fn test_execute_trade_not_trader_panics() {
    let t = setup();
    let sid = register_dex_strategy(&t);
    let guard = register_pass_guard(&t);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    t.vault.set_trade_guard(&t.manager, &sid, &guard);

    let rogue = Address::generate(&t.env);
    let path: Vec<Address> = vec![&t.env, Address::generate(&t.env), Address::generate(&t.env)];
    t.vault
        .execute_trade(&rogue, &sid, &100i128, &90i128, &path);
}

#[test]
#[should_panic]
fn test_execute_trade_strategy_not_whitelisted_panics() {
    let t = setup();
    let sid = register_dex_strategy(&t);
    // Did NOT add sid to strategies.
    let path: Vec<Address> = vec![&t.env, Address::generate(&t.env), Address::generate(&t.env)];
    t.vault
        .execute_trade(&t.trader, &sid, &100i128, &90i128, &path);
}

#[test]
#[should_panic]
fn test_execute_trade_guard_not_set_panics() {
    let t = setup();
    let sid = register_dex_strategy(&t);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    // Guard NOT set for this strategy.
    let path: Vec<Address> = vec![&t.env, Address::generate(&t.env), Address::generate(&t.env)];
    t.vault
        .execute_trade(&t.trader, &sid, &100i128, &90i128, &path);
}

#[test]
#[should_panic]
fn test_execute_trade_guard_rejects_panics() {
    let t = setup();
    let sid = register_dex_strategy(&t);
    let guard = register_reject_guard(&t); // Always rejects.
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    t.vault.set_trade_guard(&t.manager, &sid, &guard);

    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    let path: Vec<Address> = vec![&t.env, Address::generate(&t.env), Address::generate(&t.env)];
    t.vault
        .execute_trade(&t.trader, &sid, &100i128, &90i128, &path);
}

#[test]
#[should_panic]
fn test_execute_trade_zero_amount_panics() {
    let t = setup();
    let sid = register_dex_strategy(&t);
    let guard = register_pass_guard(&t);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    t.vault.set_trade_guard(&t.manager, &sid, &guard);
    let path: Vec<Address> = vec![&t.env, Address::generate(&t.env), Address::generate(&t.env)];
    t.vault
        .execute_trade(&t.trader, &sid, &0i128, &0i128, &path);
}

// ---------------------------------------------------------------------------
// Unwind edge cases
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_unwind_not_manager_panics() {
    let t = setup();
    let sid = env_register_strategy(&t);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    t.vault.invest(&t.manager, &sid, &500_0000000i128);
    let rogue = Address::generate(&t.env);
    t.vault.unwind(&rogue, &sid, &100_0000000i128);
}

#[test]
#[should_panic]
fn test_unwind_strategy_not_whitelisted_panics() {
    let t = setup();
    let sid = env_register_strategy(&t);
    // NOT added to whitelist.
    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    t.vault.unwind(&t.manager, &sid, &100_0000000i128);
}

#[test]
#[should_panic]
fn test_unwind_zero_amount_panics() {
    let t = setup();
    let sid = env_register_strategy(&t);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    t.vault.invest(&t.manager, &sid, &500_0000000i128);
    t.vault.unwind(&t.manager, &sid, &0i128);
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
            manager: manager.clone(),
            trader: trader.clone(),
            base_asset: base,
            share_token: share,
            share_token_admin: manager.clone(),
            entry_fee_bps: 0,
            exit_fee_bps: 501, // > MAX_ENTRY_EXIT_FEE_BPS
            mgmt_fee_bps: 0,
            perf_fee_bps: 0,
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
            manager: manager.clone(),
            trader: trader.clone(),
            base_asset: base,
            share_token: share,
            share_token_admin: manager.clone(),
            entry_fee_bps: 0,
            exit_fee_bps: 0,
            mgmt_fee_bps: 301, // > MAX_MGMT_FEE_BPS
            perf_fee_bps: 0,
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
            manager: manager.clone(),
            trader: trader.clone(),
            base_asset: base,
            share_token: share,
            share_token_admin: manager.clone(),
            entry_fee_bps: 0,
            exit_fee_bps: 0,
            mgmt_fee_bps: 0,
            perf_fee_bps: 3_001, // > MAX_PERF_FEE_BPS
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
    t.vault.deposit(&amount, &t.user, &0i128);

    let recipient = Address::generate(&t.env);
    let share_amount = shares(&t, &t.user);
    let before_recipient = base(&t, &recipient);
    t.vault.withdraw(&share_amount, &t.user, &recipient, &0i128);

    assert_eq!(base(&t, &recipient), before_recipient + amount);
    assert_eq!(shares(&t, &t.user), 0);
}

// ---------------------------------------------------------------------------
// Invest zero panics
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_invest_zero_panics() {
    let t = setup();
    let sid = env_register_strategy(&t);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    t.vault.invest(&t.manager, &sid, &0i128);
}

// ---------------------------------------------------------------------------
// Pause blocks invest/unwind as well? (invest and unwind are manager-only,
// but not gated by paused — this confirms that)
// ---------------------------------------------------------------------------

#[test]
fn test_invest_works_while_paused() {
    let t = setup();
    let sid = env_register_strategy(&t);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);

    // Deposit before pausing.
    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);

    t.vault.pause(&t.manager);
    // Manager can still invest even while paused (only deposit/withdraw are blocked).
    t.vault.invest(&t.manager, &sid, &500_0000000i128);
}

// ---------------------------------------------------------------------------
// Unpause by non-manager panics
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_unpause_not_manager_panics() {
    let t = setup();
    t.vault.pause(&t.manager);
    let rogue = Address::generate(&t.env);
    t.vault.unpause(&rogue);
}

// ---------------------------------------------------------------------------
// NAV with multiple strategies
// ---------------------------------------------------------------------------

#[test]
fn test_nav_sums_multiple_strategies() {
    let t = setup();
    let s1 = env_register_strategy(&t);
    let s2 = env_register_strategy(&t);
    let strategies: Vec<Address> = vec![&t.env, s1.clone(), s2.clone()];
    t.vault.set_strategies(&t.manager, &strategies);

    let deposit = 2_000_0000000i128;
    t.vault.deposit(&deposit, &t.user, &0i128);

    t.vault.invest(&t.manager, &s1, &600_0000000i128);
    t.vault.invest(&t.manager, &s2, &400_0000000i128);

    // NAV = vault_balance(1000) + s1_value(600) + s2_value(400) = 2000
    assert_eq!(t.vault.get_nav(), deposit);
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
    t.vault.deposit(&deposit, &t.user, &0i128);
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
    t.vault.deposit(&deposit, &t.user, &0i128);
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
    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #15)")]
fn test_deposit_cap_exceeded_panics() {
    let t = setup();
    t.vault.set_deposit_cap(&t.manager, &500_0000000i128);
    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
}

#[test]
fn test_deposit_cap_zero_means_uncapped() {
    let t = setup();
    t.vault.set_deposit_cap(&t.manager, &0i128);
    t.vault.deposit(&10_000_0000000i128, &t.user, &0i128);
    t.vault.deposit(&10_000_0000000i128, &t.user2, &0i128);
}

#[test]
#[should_panic]
fn test_set_deposit_cap_not_manager_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.vault.set_deposit_cap(&rogue, &1_000_0000000i128);
}

// ---------------------------------------------------------------------------
// Strategy removal safety
// ---------------------------------------------------------------------------

#[test]
fn test_set_strategies_can_remove_zero_position_strategy() {
    let t = setup();
    let s1 = env_register_strategy(&t);
    let s2 = env_register_strategy(&t);
    let both: Vec<Address> = vec![&t.env, s1.clone(), s2.clone()];
    t.vault.set_strategies(&t.manager, &both);
    // s1 has no invested position — removing it should succeed.
    let only_s2: Vec<Address> = vec![&t.env, s2.clone()];
    t.vault.set_strategies(&t.manager, &only_s2);
}

#[test]
#[should_panic(expected = "Error(Contract, #14)")]
fn test_set_strategies_cannot_remove_strategy_with_active_position() {
    let t = setup();
    let s1 = env_register_strategy(&t);
    let s2 = env_register_strategy(&t);
    let both: Vec<Address> = vec![&t.env, s1.clone(), s2.clone()];
    t.vault.set_strategies(&t.manager, &both);

    t.vault.deposit(&2_000_0000000i128, &t.user, &0i128);
    t.vault.invest(&t.manager, &s1, &500_0000000i128);

    // Attempt to remove s1 while it has a position — must revert.
    let only_s2: Vec<Address> = vec![&t.env, s2.clone()];
    t.vault.set_strategies(&t.manager, &only_s2);
}

// ---------------------------------------------------------------------------
// GAP 1 — Guard invest/unwind operations
// ---------------------------------------------------------------------------

// Guards for invest/unwind need validate_invest and validate_unwind methods.
mod mock_invest_pass_guard_mod {
    use soroban_sdk::{contract, contractimpl, Address, Env};
    #[contract]
    pub struct MockInvestPassGuard;
    #[contractimpl]
    impl MockInvestPassGuard {
        pub fn validate_invest(_env: Env, _vault: Address, _amount: i128) { /* always passes */
        }
        pub fn validate_unwind(_env: Env, _vault: Address, _units: i128) { /* always passes */
        }
        pub fn validate_swap_exact_in(
            _env: Env,
            _caller: Address,
            _amount_in: i128,
            _min_out: i128,
            _path: soroban_sdk::Vec<Address>,
        ) {
        }
    }
}
use mock_invest_pass_guard_mod::MockInvestPassGuard;

mod mock_invest_reject_guard_mod {
    use soroban_sdk::{contract, contractimpl, Address, Env};
    #[contract]
    pub struct MockInvestRejectGuard;
    #[contractimpl]
    impl MockInvestRejectGuard {
        pub fn validate_invest(_env: Env, _vault: Address, _amount: i128) {
            panic!("guard rejected invest");
        }
        pub fn validate_unwind(_env: Env, _vault: Address, _units: i128) {
            panic!("guard rejected unwind");
        }
        pub fn validate_swap_exact_in(
            _env: Env,
            _caller: Address,
            _amount_in: i128,
            _min_out: i128,
            _path: soroban_sdk::Vec<Address>,
        ) {
        }
    }
}
use mock_invest_reject_guard_mod::MockInvestRejectGuard;

#[test]
fn test_invest_with_pass_guard_succeeds() {
    let t = setup();
    let sid = env_register_strategy(&t);
    let guard = t.env.register(MockInvestPassGuard, ());
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    t.vault.set_trade_guard(&t.manager, &sid, &guard);

    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    t.vault.invest(&t.manager, &sid, &200_0000000i128);
    // Guard passed — strategy received funds.
    assert_eq!(
        MockStrategyClient::new(&t.env, &sid).get_value(&t.vault_addr),
        200_0000000i128
    );
}

#[test]
#[should_panic]
fn test_invest_with_reject_guard_panics() {
    let t = setup();
    let sid = env_register_strategy(&t);
    let guard = t.env.register(MockInvestRejectGuard, ());
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    t.vault.set_trade_guard(&t.manager, &sid, &guard);

    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    t.vault.invest(&t.manager, &sid, &200_0000000i128);
}

#[test]
fn test_unwind_with_pass_guard_succeeds() {
    let t = setup();
    let sid = env_register_strategy(&t);
    let guard = t.env.register(MockInvestPassGuard, ());
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    t.vault.set_trade_guard(&t.manager, &sid, &guard);

    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    t.vault.invest(&t.manager, &sid, &200_0000000i128);
    t.vault.unwind(&t.manager, &sid, &200_0000000i128);
    assert_eq!(
        MockStrategyClient::new(&t.env, &sid).get_value(&t.vault_addr),
        0
    );
}

#[test]
#[should_panic]
fn test_unwind_with_reject_guard_panics() {
    let t = setup();
    let sid = env_register_strategy(&t);
    let guard = t.env.register(MockInvestRejectGuard, ());
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    t.vault.set_trade_guard(&t.manager, &sid, &guard);

    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    // invest bypasses guard for setup (guard blocks invest too — but we need to
    // invest first; use a fresh strategy without guard for setup then swap).
    // Simpler: no guard during invest, set guard afterwards for unwind test.
    t.vault.set_trade_guard(&t.manager, &sid, &guard); // guard set
    t.vault.unwind(&t.manager, &sid, &1i128); // guard rejects → panic
}

#[test]
fn test_invest_without_guard_succeeds() {
    // Verify invest still works when NO guard is set (guard is optional).
    let t = setup();
    let sid = env_register_strategy(&t);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    // No guard set.
    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    t.vault.invest(&t.manager, &sid, &300_0000000i128);
    assert_eq!(
        MockStrategyClient::new(&t.env, &sid).get_value(&t.vault_addr),
        300_0000000i128
    );
}

// ---------------------------------------------------------------------------
// GAP 2 — Proportional auto-unwind on withdrawal
// ---------------------------------------------------------------------------

#[test]
fn test_auto_unwind_covers_withdrawal_shortfall() {
    let t = setup();
    let sid = env_register_strategy(&t);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);

    // Deposit, invest all funds into strategy, leaving vault with zero base balance.
    let deposit_amount = 1_000_0000000i128;
    t.vault.deposit(&deposit_amount, &t.user, &0i128);
    t.vault.invest(&t.manager, &sid, &deposit_amount);

    // Vault base balance is now 0; strategy holds 1000.
    assert_eq!(base(&t, &t.vault_addr), 0);

    // Withdraw shares — auto-unwind should pull from strategy.
    let user_shares = shares(&t, &t.user);
    let returned = t.vault.withdraw(&user_shares, &t.user, &t.user, &0i128);
    assert!(returned > 0);
    // User received base asset from strategy unwind.
    assert!(base(&t, &t.user) > 1_000_000_0000000i128 - deposit_amount);
}

#[test]
fn test_auto_unwind_skips_lp_strategies() {
    let t = setup();
    let sa_sid = env_register_strategy(&t); // single-asset
    let lp_sid = env_register_strategy(&t); // will be marked LP

    let strategies: Vec<Address> = vec![&t.env, sa_sid.clone(), lp_sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    // Mark lp_sid as LP.
    t.vault.set_lp_strategy(&t.manager, &lp_sid, &true);
    // LP strategies now require internal oracle-backed valuation.
    MockStrategyClient::new(&t.env, &lp_sid).set_oracle_enabled(&true);

    let deposit_amount = 1_000_0000000i128;
    t.vault.deposit(&deposit_amount, &t.user, &0i128);
    // Invest half into single-asset, half into LP.
    t.vault.invest(&t.manager, &sa_sid, &500_0000000i128);
    t.vault.invest(&t.manager, &lp_sid, &500_0000000i128);

    // Vault base = 0 now. Withdraw should only unwind sa_sid.
    let user_shares = shares(&t, &t.user);
    // Only sa_sid (500) can be auto-unwound; lp_sid stays invested.
    // This test verifies the call doesn't panic — full amount may not be covered
    // since LP portion is skipped, but the call proceeds without error.
    let _returned = t.vault.withdraw(&(user_shares / 2), &t.user, &t.user, &0i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #20)")]
fn test_withdraw_panics_when_only_lp_liquidity_remains() {
    let t = setup();
    let sa_sid = env_register_strategy(&t);
    let lp_sid = env_register_strategy(&t);

    let strategies: Vec<Address> = vec![&t.env, sa_sid.clone(), lp_sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    t.vault.set_lp_strategy(&t.manager, &lp_sid, &true);
    MockStrategyClient::new(&t.env, &lp_sid).set_oracle_enabled(&true);

    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    t.vault.invest(&t.manager, &sa_sid, &100_0000000i128);
    t.vault.invest(&t.manager, &lp_sid, &900_0000000i128);

    // Full withdraw needs both positions, but auto-unwind can only use single-asset liquidity.
    let user_shares = shares(&t, &t.user);
    t.vault.withdraw(&user_shares, &t.user, &t.user, &0i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #21)")]
fn test_lp_flag_cannot_be_cleared_once_set() {
    let t = setup();
    let sid = env_register_strategy(&t);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    t.vault.set_lp_strategy(&t.manager, &sid, &true);
    t.vault.set_lp_strategy(&t.manager, &sid, &false);
}

// ---------------------------------------------------------------------------
// GAP 3 — Oracle-priced NAV
// ---------------------------------------------------------------------------

// Minimal mock oracle: stores prices per asset address.
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

#[test]
fn test_oracle_nav_scales_strategy_value() {
    let t = setup();
    let sid = env_register_strategy(&t);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);

    // Deploy mock oracle and set price for strategy = 2x (price = 2 * PRICE_PRECISION).
    let oracle_id = t.env.register(MockOracle, ());
    // Use a synthetic "lp token" address as price token.
    let lp_token = Address::generate(&t.env);
    mock_oracle_mod::MockOracleClient::new(&t.env, &oracle_id)
        .set_price(&lp_token, &(2 * 10_000_000i128)); // 2.0x

    t.vault.set_oracle(&t.manager, &oracle_id);
    t.vault
        .set_strategy_oracle_token(&t.manager, &sid, &lp_token);

    // Deposit and invest.
    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    t.vault.invest(&t.manager, &sid, &500_0000000i128);

    // NAV should be: 500 (vault cash) + 500*2 (oracle-priced strategy) = 1500
    let nav = t.vault.get_nav();
    assert_eq!(nav, 1_500_0000000i128);
}

#[test]
fn test_nav_without_oracle_uses_raw_value() {
    let t = setup();
    let sid = env_register_strategy(&t);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);

    // No oracle set.
    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    t.vault.invest(&t.manager, &sid, &500_0000000i128);

    // NAV = 500 (vault) + 500 (raw strategy value) = 1000.
    let nav = t.vault.get_nav();
    assert_eq!(nav, 1_000_0000000i128);
}

#[test]
#[should_panic]
fn test_set_oracle_not_manager_panics() {
    let t = setup();
    let oracle_id = t.env.register(MockOracle, ());
    let rogue = Address::generate(&t.env);
    t.vault.set_oracle(&rogue, &oracle_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #19)")]
fn test_lp_strategy_without_oracle_rejected_in_nav() {
    let t = setup();
    let sid = env_register_strategy(&t);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    t.vault.set_lp_strategy(&t.manager, &sid, &true);

    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    t.vault.invest(&t.manager, &sid, &500_0000000i128);

    // LP strategy has a non-zero position but no internal oracle configured.
    let _ = t.vault.get_nav();
}

#[test]
fn test_lp_strategy_with_oracle_allows_nav() {
    let t = setup();
    let sid = env_register_strategy(&t);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    t.vault.set_lp_strategy(&t.manager, &sid, &true);
    MockStrategyClient::new(&t.env, &sid).set_oracle_enabled(&true);

    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    t.vault.invest(&t.manager, &sid, &500_0000000i128);

    // NAV = 500 (vault) + 500 (LP strategy internal valuation path).
    assert_eq!(t.vault.get_nav(), 1_000_0000000i128);
}

// ---------------------------------------------------------------------------
// GAP 4 — Per-strategy concentration limit
// ---------------------------------------------------------------------------

#[test]
fn test_concentration_limit_allows_within_cap() {
    let t = setup();
    let sid = env_register_strategy(&t);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    t.vault.set_max_concentration_bps(&t.manager, &5_000u32); // 50% cap

    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    // Invest 40% of NAV — should be fine.
    t.vault.invest(&t.manager, &sid, &400_0000000i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #16)")]
fn test_concentration_limit_exceeded_panics() {
    let t = setup();
    let sid = env_register_strategy(&t);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    t.vault.set_max_concentration_bps(&t.manager, &5_000u32); // 50% cap

    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    // Invest 60% of NAV — exceeds 50% cap → should panic.
    t.vault.invest(&t.manager, &sid, &600_0000000i128);
}

#[test]
fn test_concentration_limit_zero_means_uncapped() {
    let t = setup();
    let sid = env_register_strategy(&t);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    t.vault.set_max_concentration_bps(&t.manager, &0u32); // uncapped

    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    // Invest 100% — no cap in effect.
    t.vault.invest(&t.manager, &sid, &1_000_0000000i128);
}

#[test]
#[should_panic]
fn test_set_max_concentration_bps_not_manager_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.vault.set_max_concentration_bps(&rogue, &3_000u32);
}

// ---------------------------------------------------------------------------
// GAP A — Post-execution TVL (NAV) guard
// ---------------------------------------------------------------------------

/// A "lossy" strategy: accepts a deposit of `amount` base tokens but reports
/// only `amount * (10_000 - loss_bps) / 10_000` via `get_value()`.
/// This simulates slippage or an exploitable price gap so we can test the
/// NAV guard without touching real DeFi protocols.
mod mock_lossy_strategy_mod {
    use super::MockTokenClient;
    use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};
    #[contracttype]
    pub enum LossyKey {
        Value,
        BaseAsset,
        LossBps,
    }
    #[contract]
    pub struct MockLossyStrategy;
    #[contractimpl]
    impl MockLossyStrategy {
        pub fn init(env: Env, base_asset: Address, loss_bps: u32) {
            env.storage()
                .instance()
                .set(&LossyKey::BaseAsset, &base_asset);
            env.storage().instance().set(&LossyKey::LossBps, &loss_bps);
            env.storage().instance().set(&LossyKey::Value, &0i128);
        }
        pub fn deposit(env: Env, amount: i128, from: Address) -> i128 {
            let base: Address = env.storage().instance().get(&LossyKey::BaseAsset).unwrap();
            let strategy = env.current_contract_address();
            MockTokenClient::new(&env, &base).transfer(&from, &strategy, &amount);
            let loss_bps: u32 = env
                .storage()
                .instance()
                .get(&LossyKey::LossBps)
                .unwrap_or(0);
            let reported = amount * (10_000 - loss_bps as i128) / 10_000;
            let v: i128 = env.storage().instance().get(&LossyKey::Value).unwrap_or(0);
            env.storage()
                .instance()
                .set(&LossyKey::Value, &(v + reported));
            reported
        }
        pub fn withdraw(env: Env, amount: i128, _from: Address, to: Address) -> i128 {
            let base: Address = env.storage().instance().get(&LossyKey::BaseAsset).unwrap();
            let strategy = env.current_contract_address();
            MockTokenClient::new(&env, &base).transfer(&strategy, &to, &amount);
            let v: i128 = env.storage().instance().get(&LossyKey::Value).unwrap_or(0);
            let new_v = if v >= amount { v - amount } else { 0 };
            env.storage().instance().set(&LossyKey::Value, &new_v);
            amount
        }
        pub fn get_value(env: Env, _vault: Address) -> i128 {
            env.storage().instance().get(&LossyKey::Value).unwrap_or(0)
        }
    }
}
use mock_lossy_strategy_mod::MockLossyStrategy;

fn env_register_lossy_strategy(t: &T, loss_bps: u32) -> Address {
    let sid = t.env.register(MockLossyStrategy, ());
    mock_lossy_strategy_mod::MockLossyStrategyClient::new(&t.env, &sid).init(&t.base, &loss_bps);
    // No extra mint — user deposit provides the vault balance needed for invest.
    sid
}

#[test]
fn test_set_max_loss_bps_updates() {
    let t = setup();
    // Set to 500 bps (5%) and verify a normal invest still passes.
    t.vault.set_max_loss_bps(&t.manager, &500u32);

    let sid = env_register_strategy(&t);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);

    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    // Invest via a normal (non-lossy) strategy — NAV is preserved → guard passes.
    t.vault.invest(&t.manager, &sid, &400_0000000i128);
}

#[test]
#[should_panic]
fn test_set_max_loss_bps_not_manager_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.vault.set_max_loss_bps(&rogue, &200u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #17)")]
fn test_tvl_guard_enabled_by_default() {
    let t = setup();
    // 50% lossy strategy; default guard is 10%, so this must fail.
    let sid = env_register_lossy_strategy(&t, 5_000);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);

    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    // Invest 500 — strategy reports only 250 back, breaching default loss guard.
    t.vault.invest(&t.manager, &sid, &500_0000000i128);
}

/// Explicitly setting max_loss_bps = 0 disables the guard.
#[test]
fn test_tvl_guard_can_be_disabled_explicitly() {
    let t = setup();
    let sid = env_register_lossy_strategy(&t, 5_000);
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    t.vault.set_max_loss_bps(&t.manager, &0u32);

    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    t.vault.invest(&t.manager, &sid, &500_0000000i128);
}

/// Guard allows invest when NAV loss is within the configured tolerance.
#[test]
fn test_tvl_guard_passes_within_tolerance() {
    let t = setup();
    // 1% lossy strategy; guard tolerance is 2% — loss is within tolerance.
    let sid = env_register_lossy_strategy(&t, 100); // 1% loss
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    t.vault.set_max_loss_bps(&t.manager, &200u32); // 2% tolerance

    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    // Invest 1000: strategy takes 1000, reports 990 → NAV drops by 10 (1%).
    // Guard allows up to 2% drop → passes.
    t.vault.invest(&t.manager, &sid, &1_000_0000000i128);
}

/// Guard trips when NAV loss exceeds the configured tolerance.
#[test]
#[should_panic(expected = "Error(Contract, #17)")]
fn test_tvl_guard_trips_when_loss_exceeds_tolerance() {
    let t = setup();
    // 10% lossy strategy; guard tolerance is 5%.
    let sid = env_register_lossy_strategy(&t, 1_000); // 10% loss
    let strategies: Vec<Address> = vec![&t.env, sid.clone()];
    t.vault.set_strategies(&t.manager, &strategies);
    t.vault.set_max_loss_bps(&t.manager, &500u32); // 5% tolerance

    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    // Invest 1000: strategy takes 1000, reports 900 → NAV drops by 100 (10%).
    // Guard allows only 5% → TvlGuardTripped (#17).
    t.vault.invest(&t.manager, &sid, &1_000_0000000i128);
}

#[test]
#[should_panic]
fn test_set_max_loss_bps_above_denominator_panics() {
    let t = setup();
    t.vault.set_max_loss_bps(&t.manager, &10_001u32);
}

#[test]
#[should_panic]
fn test_set_max_concentration_bps_above_denominator_panics() {
    let t = setup();
    t.vault.set_max_concentration_bps(&t.manager, &10_001u32);
}

// ---------------------------------------------------------------------------
// dHedge parity: private pool, cooldown, fee timelock, value manipulation
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #24)")]
fn test_private_pool_blocks_non_member_deposit() {
    let t = setup();
    t.vault.set_private_pool(&t.manager, &true);
    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
}

#[test]
fn test_private_pool_allows_manager_and_member() {
    let t = setup();
    t.vault.set_private_pool(&t.manager, &true);
    t.vault.deposit(&1_000_0000000i128, &t.manager, &0i128);
    t.vault.add_member(&t.manager, &t.user);
    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
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
    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    let user_shares = shares(&t, &t.user);
    t.vault.withdraw(&user_shares, &t.user, &t.user, &0i128);
}

#[test]
fn test_withdraw_after_cooldown_succeeds() {
    let t = setup();
    t.vault.set_exit_cooldown_secs(&t.manager, &120u64);
    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
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
    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    let user_shares = shares(&t, &t.user);
    // Same ledger + same actor but different op type (deposit -> withdraw).
    t.vault.withdraw(&user_shares, &t.user, &t.user, &0i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #28)")]
fn test_value_guard_same_ledger_nav_mismatch_panics() {
    let t = setup();
    t.vault.set_value_guard_enabled(&t.manager, &true);
    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
    // External NAV mutation in same ledger (simulated mint to vault).
    MockTokenClient::new(&t.env, &t.base).mint(&t.vault_addr, &1i128);
    // Same op type (deposit), same ledger, but nav_before != expected_nav_after.
    t.vault.deposit(&1_000_0000000i128, &t.user, &0i128);
}
