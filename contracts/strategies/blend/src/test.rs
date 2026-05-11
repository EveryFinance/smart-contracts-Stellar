//! Tests for the BlendStrategy contract.
//!
//! A lightweight `MockBlendPool` and `MockToken` are registered inside the
//! Soroban test environment so that cross-contract calls resolve without
//! needing real WASM binaries.

#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl, contracttype, testutils::Address as _, Address, Env, String, Vec,
};

use crate::{BlendRequest, BlendStrategy, BlendStrategyClient};

// ---------------------------------------------------------------------------
// MockToken  — minimal SEP-41 token for testing
// ---------------------------------------------------------------------------

#[contracttype]
enum TokenKey {
    Balance(Address),
    Admin,
}

#[contract]
pub struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn initialize(env: Env, admin: Address) {
        env.storage().instance().set(&TokenKey::Admin, &admin);
    }
    pub fn mint(env: Env, to: Address, amount: i128) {
        let bal: i128 = env
            .storage()
            .persistent()
            .get(&TokenKey::Balance(to.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&TokenKey::Balance(to), &(bal + amount));
    }
    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&TokenKey::Balance(id))
            .unwrap_or(0)
    }
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        let from_bal: i128 = env
            .storage()
            .persistent()
            .get(&TokenKey::Balance(from.clone()))
            .unwrap_or(0);
        assert!(from_bal >= amount, "insufficient balance");
        env.storage()
            .persistent()
            .set(&TokenKey::Balance(from), &(from_bal - amount));
        let to_bal: i128 = env
            .storage()
            .persistent()
            .get(&TokenKey::Balance(to.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&TokenKey::Balance(to), &(to_bal + amount));
    }
    pub fn approve(_env: Env, _from: Address, _spender: Address, _amount: i128, _exp: u32) {}
    pub fn allowance(_env: Env, _from: Address, _spender: Address) -> i128 {
        i128::MAX
    }
    pub fn transfer_from(env: Env, _spender: Address, from: Address, to: Address, amount: i128) {
        let from_bal: i128 = env
            .storage()
            .persistent()
            .get(&TokenKey::Balance(from.clone()))
            .unwrap_or(0);
        assert!(from_bal >= amount, "insufficient balance");
        env.storage()
            .persistent()
            .set(&TokenKey::Balance(from), &(from_bal - amount));
        let to_bal: i128 = env
            .storage()
            .persistent()
            .get(&TokenKey::Balance(to.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&TokenKey::Balance(to), &(to_bal + amount));
    }
    pub fn burn(env: Env, from: Address, amount: i128) {
        from.require_auth();
        let bal: i128 = env
            .storage()
            .persistent()
            .get(&TokenKey::Balance(from.clone()))
            .unwrap_or(0);
        assert!(bal >= amount);
        env.storage()
            .persistent()
            .set(&TokenKey::Balance(from), &(bal - amount));
    }
    pub fn decimals(_env: Env) -> u32 {
        7
    }
    pub fn name(env: Env) -> String {
        String::from_str(&env, "MockToken")
    }
    pub fn symbol(env: Env) -> String {
        String::from_str(&env, "MOCK")
    }
}

// ---------------------------------------------------------------------------
// MockBlendPool — minimal Blend pool for testing
// ---------------------------------------------------------------------------

#[contracttype]
enum BlendKey {
    Supply(Address), // account → supplied balance
    Token,
}

#[contract]
pub struct MockBlendPool;

#[contractimpl]
impl MockBlendPool {
    /// Store the underlying token address so `submit` can transfer on withdraw.
    pub fn set_token(env: Env, token: Address) {
        env.storage().instance().set(&BlendKey::Token, &token);
    }

    /// Mimics `blend_pool.submit(from, spender, to, requests)`.
    /// Supply: credits `from`'s Blend position.
    /// Withdraw: reduces `from`'s position and transfers token to `to`.
    pub fn submit(
        env: Env,
        from: Address,
        _spender: Address,
        to: Address,
        requests: Vec<BlendRequest>,
    ) {
        // Mirror the real Blend pool: the position owner must authorise.
        from.require_auth();
        for req in requests.iter() {
            if req.request_type == 2 {
                // Supply — record position for `from`.
                let bal: i128 = env
                    .storage()
                    .persistent()
                    .get(&BlendKey::Supply(from.clone()))
                    .unwrap_or(0);
                env.storage()
                    .persistent()
                    .set(&BlendKey::Supply(from.clone()), &(bal + req.amount));
            } else if req.request_type == 3 {
                // Withdraw — reduce `from`'s position and transfer token to `to`.
                let bal: i128 = env
                    .storage()
                    .persistent()
                    .get(&BlendKey::Supply(from.clone()))
                    .unwrap_or(0);
                assert!(bal >= req.amount, "blend: insufficient position");
                env.storage()
                    .persistent()
                    .set(&BlendKey::Supply(from.clone()), &(bal - req.amount));
                let token_addr: Address = env.storage().instance().get(&BlendKey::Token).unwrap();
                MockTokenClient::new(&env, &token_addr).transfer(
                    &env.current_contract_address(),
                    &to,
                    &req.amount,
                );
            }
        }
    }

    pub fn get_supply(env: Env, account: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&BlendKey::Supply(account))
            .unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// MockVault — minimal vault stub that satisfies vault.get_manager() calls
// ---------------------------------------------------------------------------

#[contracttype]
enum VaultKey {
    Manager,
}

#[contract]
pub struct MockVault;

#[contractimpl]
impl MockVault {
    pub fn set_manager(env: Env, manager: Address) {
        env.storage().instance().set(&VaultKey::Manager, &manager);
    }
    pub fn get_manager(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&VaultKey::Manager)
            .unwrap()
    }
    pub fn get_factory(_env: Env) -> Option<Address> {
        None
    }
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

struct TestEnv {
    env: Env,
    strategy: BlendStrategyClient<'static>,
    token_id: Address,
    blend_pool: Address,
    vault: Address,
    manager: Address,
    user: Address,
}

fn setup() -> TestEnv {
    let env = Env::default();
    env.mock_all_auths();

    let token_id = env.register(MockToken, ());
    MockTokenClient::new(&env, &token_id).initialize(&Address::generate(&env));

    // For the blend pool mock, we use a simplified approach:
    // MockBlendPool is registered but strategy uses a simplified version
    // that just tracks via TotalDeposited (no real blend calls in test mode).
    // We register a contract at the blend_pool address but strategy's
    // blend_submit will be routed there.
    let blend_pool = env.register(MockBlendPool, ());
    MockBlendPoolClient::new(&env, &blend_pool).set_token(&token_id);
    let manager = Address::generate(&env);
    // Register a mock vault so strategy.initialize() can cross-call
    // vault.get_manager() to derive the authoritative initializer.
    let vault = env.register(MockVault, ());
    MockVaultClient::new(&env, &vault).set_manager(&manager);
    let user = Address::generate(&env);

    let strategy_id = env.register(BlendStrategy, ());
    let strategy = BlendStrategyClient::new(&env, &strategy_id);

    strategy.initialize(
        &vault,
        &token_id,
        &blend_pool,
        &manager,
        &String::from_str(&env, "Blend USDC Strategy"),
    );

    // Mint tokens into vault for deposit tests.
    MockTokenClient::new(&env, &token_id).mint(&vault, &10_000_0000000i128);

    // The unsafe cast is acceptable in test code.
    let strategy: BlendStrategyClient<'static> = unsafe { core::mem::transmute(strategy) };

    TestEnv {
        env,
        strategy,
        token_id,
        blend_pool,
        vault,
        manager,
        user,
    }
}

// ---------------------------------------------------------------------------
// Initialization tests
// ---------------------------------------------------------------------------

#[test]
fn test_initialize() {
    let t = setup();
    assert_eq!(t.strategy.asset(), t.token_id);
    assert_eq!(t.strategy.get_protocol_address(), t.blend_pool);
    assert!(!t.strategy.is_paused());
    assert_eq!(t.strategy.get_value(&t.vault), 0i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_double_initialize_panics() {
    let t = setup();
    t.strategy.initialize(
        &t.vault,
        &t.token_id,
        &t.blend_pool,
        &t.manager,
        &String::from_str(&t.env, "x"),
    );
}

// ---------------------------------------------------------------------------
// Deposit tests
// ---------------------------------------------------------------------------

#[test]
fn test_deposit_tracks_position() {
    let t = setup();
    let deposited = t.strategy.deposit(&1_000_0000000i128, &t.vault);
    assert_eq!(deposited, 1_000_0000000i128);
    assert_eq!(t.strategy.get_value(&t.vault), 1_000_0000000i128);
}

#[test]
fn test_deposit_multiple_accumulates() {
    let t = setup();
    t.strategy.deposit(&500_0000000i128, &t.vault);
    t.strategy.deposit(&300_0000000i128, &t.vault);
    assert_eq!(t.strategy.get_value(&t.vault), 800_0000000i128);
}

#[test]
#[should_panic]
fn test_deposit_not_vault_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.strategy.deposit(&100i128, &rogue);
}

#[test]
#[should_panic]
fn test_deposit_zero_panics() {
    let t = setup();
    t.strategy.deposit(&0i128, &t.vault);
}

#[test]
#[should_panic]
fn test_deposit_negative_panics() {
    let t = setup();
    t.strategy.deposit(&-1i128, &t.vault);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_deposit_when_paused_panics() {
    let t = setup();
    t.strategy.pause(&t.manager);
    t.strategy.deposit(&100i128, &t.vault);
}

// ---------------------------------------------------------------------------
// Withdraw tests
// ---------------------------------------------------------------------------

#[test]
fn test_withdraw_reduces_position() {
    let t = setup();
    t.strategy.deposit(&1_000_0000000i128, &t.vault);
    // Pre-fund MockBlendPool with the token so it can pay the user.
    MockTokenClient::new(&t.env, &t.token_id).mint(&t.blend_pool, &1_000_0000000i128);

    t.strategy.withdraw(&400_0000000i128, &t.vault, &t.user);
    assert_eq!(t.strategy.get_value(&t.vault), 600_0000000i128);
}

#[test]
#[should_panic]
fn test_withdraw_more_than_position_panics() {
    let t = setup();
    t.strategy.deposit(&100i128, &t.vault);
    t.strategy.withdraw(&200i128, &t.vault, &t.user);
}

#[test]
#[should_panic]
fn test_withdraw_not_vault_panics() {
    let t = setup();
    t.strategy.deposit(&100i128, &t.vault);
    let rogue = Address::generate(&t.env);
    t.strategy.withdraw(&50i128, &rogue, &t.user);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_withdraw_when_paused_panics() {
    let t = setup();
    t.strategy.deposit(&100i128, &t.vault);
    t.strategy.pause(&t.manager);
    t.strategy.withdraw(&50i128, &t.vault, &t.user);
}

// ---------------------------------------------------------------------------
// Pause / unpause
// ---------------------------------------------------------------------------

#[test]
fn test_pause_unpause() {
    let t = setup();
    assert!(!t.strategy.is_paused());
    t.strategy.pause(&t.manager);
    assert!(t.strategy.is_paused());
    t.strategy.unpause(&t.manager);
    assert!(!t.strategy.is_paused());
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_pause_not_manager_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.strategy.pause(&rogue);
}

// ---------------------------------------------------------------------------
// Full lifecycle
// ---------------------------------------------------------------------------

#[test]
fn test_full_lifecycle() {
    let t = setup();

    // 1. Deposit
    let after_deposit = t.strategy.deposit(&1_000_0000000i128, &t.vault);
    assert_eq!(after_deposit, 1_000_0000000i128);

    // 2. get_value
    assert_eq!(t.strategy.get_value(&t.vault), 1_000_0000000i128);

    // 3. Partial withdraw — pre-fund pool
    MockTokenClient::new(&t.env, &t.token_id).mint(&t.blend_pool, &1_000_0000000i128);
    let withdrawn = t.strategy.withdraw(&300_0000000i128, &t.vault, &t.user);
    assert_eq!(withdrawn, 300_0000000i128);
    assert_eq!(t.strategy.get_value(&t.vault), 700_0000000i128);

    // 4. Full withdraw
    t.strategy.withdraw(&700_0000000i128, &t.vault, &t.user);
    assert_eq!(t.strategy.get_value(&t.vault), 0i128);
}

// ---------------------------------------------------------------------------
// NotInitialized — calling functions before initialize() panics
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_not_initialized_asset_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(BlendStrategy, ());
    let client = BlendStrategyClient::new(&env, &id);
    client.asset(); // no initialize() called
}

#[test]
#[should_panic]
fn test_not_initialized_deposit_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(BlendStrategy, ());
    let client = BlendStrategyClient::new(&env, &id);
    let vault = Address::generate(&env);
    client.deposit(&100i128, &vault);
}

#[test]
#[should_panic]
fn test_not_initialized_withdraw_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(BlendStrategy, ());
    let client = BlendStrategyClient::new(&env, &id);
    let vault = Address::generate(&env);
    let user = Address::generate(&env);
    client.withdraw(&100i128, &vault, &user);
}

// ---------------------------------------------------------------------------
// NotManager — unpause requires manager
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_unpause_not_manager_panics() {
    let t = setup();
    t.strategy.pause(&t.manager);
    let rogue = Address::generate(&t.env);
    t.strategy.unpause(&rogue);
}

// ---------------------------------------------------------------------------
// Withdraw edge cases
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_withdraw_zero_panics() {
    let t = setup();
    t.strategy.deposit(&100i128, &t.vault);
    t.strategy.withdraw(&0i128, &t.vault, &t.user);
}

#[test]
#[should_panic]
fn test_withdraw_negative_panics() {
    let t = setup();
    t.strategy.deposit(&100i128, &t.vault);
    t.strategy.withdraw(&-1i128, &t.vault, &t.user);
}

// ---------------------------------------------------------------------------
// View functions — name, is_paused
// ---------------------------------------------------------------------------

#[test]
fn test_get_name() {
    let t = setup();
    let name = t.strategy.get_name();
    assert_eq!(
        name,
        soroban_sdk::String::from_str(&t.env, "Blend USDC Strategy")
    );
}

#[test]
fn test_is_paused_false_initially() {
    let t = setup();
    assert!(!t.strategy.is_paused());
}

// ---------------------------------------------------------------------------
// Deposit: tokens moved from vault to strategy
// ---------------------------------------------------------------------------

#[test]
fn test_deposit_reduces_vault_balance() {
    let t = setup();
    let vault_before = MockTokenClient::new(&t.env, &t.token_id).balance(&t.vault);
    let amount = 500_0000000i128;
    t.strategy.deposit(&amount, &t.vault);
    let vault_after = MockTokenClient::new(&t.env, &t.token_id).balance(&t.vault);
    assert_eq!(vault_before - vault_after, amount);
}
