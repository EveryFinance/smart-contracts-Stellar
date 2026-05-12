//! Tests for the BlendStrategy multi-position contract.

#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl, contracttype, testutils::Address as _, Address, Env, String, Vec,
};

use crate::{BlendRequest, BlendStrategy, BlendStrategyClient};

// ---------------------------------------------------------------------------
// MockToken
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
// MockBlendPool
// ---------------------------------------------------------------------------

#[contracttype]
enum BlendKey {
    Supply(Address),
    Token,
    NoTransferOnWithdraw,
}

#[contract]
pub struct MockBlendPool;

#[contractimpl]
impl MockBlendPool {
    pub fn set_token(env: Env, token: Address) {
        env.storage().instance().set(&BlendKey::Token, &token);
    }

    pub fn set_no_transfer_on_withdraw(env: Env, enabled: bool) {
        env.storage()
            .instance()
            .set(&BlendKey::NoTransferOnWithdraw, &enabled);
    }

    pub fn submit(
        env: Env,
        from: Address,
        _spender: Address,
        to: Address,
        requests: Vec<BlendRequest>,
    ) {
        from.require_auth();
        for req in requests.iter() {
            if req.request_type == 2 {
                let bal: i128 = env
                    .storage()
                    .persistent()
                    .get(&BlendKey::Supply(from.clone()))
                    .unwrap_or(0);
                env.storage()
                    .persistent()
                    .set(&BlendKey::Supply(from.clone()), &(bal + req.amount));
            } else if req.request_type == 3 {
                let bal: i128 = env
                    .storage()
                    .persistent()
                    .get(&BlendKey::Supply(from.clone()))
                    .unwrap_or(0);
                assert!(bal >= req.amount, "blend: insufficient position");
                env.storage()
                    .persistent()
                    .set(&BlendKey::Supply(from.clone()), &(bal - req.amount));
                let no_transfer: bool = env
                    .storage()
                    .instance()
                    .get(&BlendKey::NoTransferOnWithdraw)
                    .unwrap_or(false);
                if no_transfer {
                    continue;
                }
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
// MockVault
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
        env.storage().instance().get(&VaultKey::Manager).unwrap()
    }
    pub fn get_factory(_env: Env) -> Option<Address> {
        None
    }
}

// ---------------------------------------------------------------------------
// Test harness
// ---------------------------------------------------------------------------

struct TestEnv {
    env: Env,
    strategy: BlendStrategyClient<'static>,
    token_id: Address,
    blend_pool: Address,
    vault: Address,
    user: Address,
}

fn setup() -> TestEnv {
    let env = Env::default();
    env.mock_all_auths();

    let token_id = env.register(MockToken, ());
    MockTokenClient::new(&env, &token_id).initialize(&Address::generate(&env));

    let blend_pool = env.register(MockBlendPool, ());
    MockBlendPoolClient::new(&env, &blend_pool).set_token(&token_id);

    let manager = Address::generate(&env);
    let vault = env.register(MockVault, ());
    MockVaultClient::new(&env, &vault).set_manager(&manager);
    let user = Address::generate(&env);

    let strategy_id = env.register(BlendStrategy, ());
    let strategy = BlendStrategyClient::new(&env, &strategy_id);

    // Multi-position initialize: just vault + name; no pool/asset args.
    strategy.initialize(&vault, &String::from_str(&env, "Blend USDC Strategy"));

    MockTokenClient::new(&env, &token_id).mint(&vault, &10_000_0000000i128);

    let strategy: BlendStrategyClient<'static> = unsafe { core::mem::transmute(strategy) };

    TestEnv {
        env,
        strategy,
        token_id,
        blend_pool,
        vault,
        user,
    }
}

// ---------------------------------------------------------------------------
// Initialization tests
// ---------------------------------------------------------------------------

#[test]
fn test_initialize() {
    let t = setup();
    assert_eq!(t.strategy.get_active_positions().len(), 0);
    assert_eq!(t.strategy.get_value(&t.vault), 0i128);
    assert_eq!(
        t.strategy.get_name(),
        String::from_str(&t.env, "Blend USDC Strategy")
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_double_initialize_panics() {
    let t = setup();
    t.strategy
        .initialize(&t.vault, &String::from_str(&t.env, "x"));
}

// ---------------------------------------------------------------------------
// Deposit tests
// ---------------------------------------------------------------------------

#[test]
fn test_deposit_tracks_position() {
    let t = setup();
    let deposited = t
        .strategy
        .deposit(&1_000_0000000i128, &t.blend_pool, &t.token_id, &t.vault);
    assert_eq!(deposited, 1_000_0000000i128);
    assert_eq!(t.strategy.get_value(&t.vault), 1_000_0000000i128);
}

#[test]
fn test_deposit_multiple_accumulates() {
    let t = setup();
    t.strategy
        .deposit(&500_0000000i128, &t.blend_pool, &t.token_id, &t.vault);
    t.strategy
        .deposit(&300_0000000i128, &t.blend_pool, &t.token_id, &t.vault);
    assert_eq!(t.strategy.get_value(&t.vault), 800_0000000i128);
}

#[test]
#[should_panic]
fn test_deposit_not_vault_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.strategy
        .deposit(&100i128, &t.blend_pool, &t.token_id, &rogue);
}

#[test]
#[should_panic]
fn test_deposit_zero_panics() {
    let t = setup();
    t.strategy
        .deposit(&0i128, &t.blend_pool, &t.token_id, &t.vault);
}

#[test]
#[should_panic]
fn test_deposit_negative_panics() {
    let t = setup();
    t.strategy
        .deposit(&-1i128, &t.blend_pool, &t.token_id, &t.vault);
}

// ---------------------------------------------------------------------------
// Withdraw tests
// ---------------------------------------------------------------------------

#[test]
fn test_withdraw_reduces_position() {
    let t = setup();
    t.strategy
        .deposit(&1_000_0000000i128, &t.blend_pool, &t.token_id, &t.vault);
    MockTokenClient::new(&t.env, &t.token_id).mint(&t.blend_pool, &1_000_0000000i128);

    t.strategy.withdraw(
        &400_0000000i128,
        &t.blend_pool,
        &t.token_id,
        &t.vault,
        &t.user,
    );
    assert_eq!(t.strategy.get_value(&t.vault), 600_0000000i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_withdraw_rejects_pool_that_sends_no_tokens() {
    let t = setup();
    t.strategy
        .deposit(&1_000_0000000i128, &t.blend_pool, &t.token_id, &t.vault);
    MockBlendPoolClient::new(&t.env, &t.blend_pool).set_no_transfer_on_withdraw(&true);
    t.strategy.withdraw(
        &400_0000000i128,
        &t.blend_pool,
        &t.token_id,
        &t.vault,
        &t.user,
    );
}

#[test]
#[should_panic]
fn test_withdraw_more_than_position_panics() {
    let t = setup();
    t.strategy
        .deposit(&100i128, &t.blend_pool, &t.token_id, &t.vault);
    t.strategy
        .withdraw(&200i128, &t.blend_pool, &t.token_id, &t.vault, &t.user);
}

#[test]
#[should_panic]
fn test_withdraw_not_vault_panics() {
    let t = setup();
    t.strategy
        .deposit(&100i128, &t.blend_pool, &t.token_id, &t.vault);
    let rogue = Address::generate(&t.env);
    t.strategy
        .withdraw(&50i128, &t.blend_pool, &t.token_id, &rogue, &t.user);
}

// ---------------------------------------------------------------------------
// Full lifecycle
// ---------------------------------------------------------------------------

#[test]
fn test_full_lifecycle() {
    let t = setup();

    let after_deposit =
        t.strategy
            .deposit(&1_000_0000000i128, &t.blend_pool, &t.token_id, &t.vault);
    assert_eq!(after_deposit, 1_000_0000000i128);
    assert_eq!(t.strategy.get_value(&t.vault), 1_000_0000000i128);

    MockTokenClient::new(&t.env, &t.token_id).mint(&t.blend_pool, &1_000_0000000i128);

    let withdrawn = t.strategy.withdraw(
        &300_0000000i128,
        &t.blend_pool,
        &t.token_id,
        &t.vault,
        &t.user,
    );
    assert_eq!(withdrawn, 300_0000000i128);
    assert_eq!(t.strategy.get_value(&t.vault), 700_0000000i128);

    t.strategy.withdraw(
        &700_0000000i128,
        &t.blend_pool,
        &t.token_id,
        &t.vault,
        &t.user,
    );
    assert_eq!(t.strategy.get_value(&t.vault), 0i128);
}

// ---------------------------------------------------------------------------
// NotInitialized panics
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_not_initialized_get_name_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(BlendStrategy, ());
    let client = BlendStrategyClient::new(&env, &id);
    client.get_name();
}

#[test]
#[should_panic]
fn test_not_initialized_deposit_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(BlendStrategy, ());
    let client = BlendStrategyClient::new(&env, &id);
    let vault = Address::generate(&env);
    let pool = Address::generate(&env);
    let asset = Address::generate(&env);
    client.deposit(&100i128, &pool, &asset, &vault);
}

#[test]
#[should_panic]
fn test_not_initialized_withdraw_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(BlendStrategy, ());
    let client = BlendStrategyClient::new(&env, &id);
    let vault = Address::generate(&env);
    let pool = Address::generate(&env);
    let asset = Address::generate(&env);
    let user = Address::generate(&env);
    client.withdraw(&100i128, &pool, &asset, &vault, &user);
}

// ---------------------------------------------------------------------------
// Withdraw edge cases
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_withdraw_zero_panics() {
    let t = setup();
    t.strategy
        .deposit(&100i128, &t.blend_pool, &t.token_id, &t.vault);
    t.strategy
        .withdraw(&0i128, &t.blend_pool, &t.token_id, &t.vault, &t.user);
}

#[test]
#[should_panic]
fn test_withdraw_negative_panics() {
    let t = setup();
    t.strategy
        .deposit(&100i128, &t.blend_pool, &t.token_id, &t.vault);
    t.strategy
        .withdraw(&-1i128, &t.blend_pool, &t.token_id, &t.vault, &t.user);
}

// ---------------------------------------------------------------------------
// View functions
// ---------------------------------------------------------------------------

#[test]
fn test_get_name() {
    let t = setup();
    assert_eq!(
        t.strategy.get_name(),
        String::from_str(&t.env, "Blend USDC Strategy")
    );
}

#[test]
fn test_deposit_reduces_vault_balance() {
    let t = setup();
    let vault_before = MockTokenClient::new(&t.env, &t.token_id).balance(&t.vault);
    let amount = 500_0000000i128;
    t.strategy
        .deposit(&amount, &t.blend_pool, &t.token_id, &t.vault);
    let vault_after = MockTokenClient::new(&t.env, &t.token_id).balance(&t.vault);
    assert_eq!(vault_before - vault_after, amount);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_checked_mul_div_rejects_zero_denominator() {
    let env = Env::default();
    super::checked_mul_div(&env, 1, 1, 0);
}

// ---------------------------------------------------------------------------
// withdraw_fraction
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_withdraw_fraction_not_vault_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.strategy
        .withdraw_fraction(&rogue, &1i128, &2i128, &t.user);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_withdraw_fraction_invalid_fraction_panics() {
    let t = setup();
    t.strategy
        .withdraw_fraction(&t.vault, &2i128, &1i128, &t.user);
}

#[test]
fn test_withdraw_fraction_no_position_is_noop() {
    let t = setup();
    t.strategy
        .withdraw_fraction(&t.vault, &1i128, &2i128, &t.user);
    assert_eq!(t.strategy.get_value(&t.vault), 0i128);
}

#[test]
fn test_withdraw_fraction_rounds_to_zero_is_noop() {
    let t = setup();
    t.strategy
        .deposit(&1i128, &t.blend_pool, &t.token_id, &t.vault);
    t.strategy
        .withdraw_fraction(&t.vault, &1i128, &2i128, &t.user);
    assert_eq!(t.strategy.get_value(&t.vault), 1i128);
}

// ---------------------------------------------------------------------------
// asset_in_use
// ---------------------------------------------------------------------------

#[test]
fn test_asset_in_use_false_for_wrong_vault_and_asset() {
    let t = setup();
    t.strategy
        .deposit(&100i128, &t.blend_pool, &t.token_id, &t.vault);
    let rogue_vault = Address::generate(&t.env);
    let rogue_asset = Address::generate(&t.env);

    assert!(!t.strategy.asset_in_use(&rogue_vault, &t.token_id));
    assert!(!t.strategy.asset_in_use(&t.vault, &rogue_asset));
    assert!(t.strategy.asset_in_use(&t.vault, &t.token_id));
}

// ---------------------------------------------------------------------------
// supply / withdraw_from_lending (execute_op path)
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_supply_zero_panics() {
    let t = setup();
    t.strategy
        .supply(&t.vault, &t.blend_pool, &t.token_id, &0i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_supply_not_vault_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.strategy
        .supply(&rogue, &t.blend_pool, &t.token_id, &100i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_withdraw_from_lending_zero_panics() {
    let t = setup();
    t.strategy
        .withdraw_from_lending(&t.vault, &t.blend_pool, &t.token_id, &0i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_withdraw_from_lending_not_vault_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.strategy
        .withdraw_from_lending(&rogue, &t.blend_pool, &t.token_id, &1i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_withdraw_from_lending_insufficient_position_panics() {
    let t = setup();
    t.strategy
        .withdraw_from_lending(&t.vault, &t.blend_pool, &t.token_id, &1i128);
}

// ---------------------------------------------------------------------------
// get_share_balance
// ---------------------------------------------------------------------------

#[test]
fn test_get_share_balance_returns_position_count() {
    let t = setup();
    assert_eq!(t.strategy.get_share_balance(), 0i128);
    t.strategy
        .deposit(&100i128, &t.blend_pool, &t.token_id, &t.vault);
    assert_eq!(t.strategy.get_share_balance(), 1i128);
}
