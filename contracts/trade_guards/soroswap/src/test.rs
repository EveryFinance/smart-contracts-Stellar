#![cfg(test)]

use soroban_sdk::{contract, contractimpl, contracttype, testutils::Address as _, vec, Address, Env, Vec};

use crate::{SoroswapTradeGuard, SoroswapTradeGuardClient};

// ---------------------------------------------------------------------------
// MockVault — satisfies guard.initialize()'s vault.get_manager() cross-call
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
}

// ---------------------------------------------------------------------------
// MockStrategy — returns a fixed 1:1 quote so slippage tests are predictable
// ---------------------------------------------------------------------------

#[contract]
pub struct MockStrategy;

#[contractimpl]
impl MockStrategy {
    /// Returns amount_in as the quote (1:1 exchange rate for test simplicity).
    pub fn quote_exact_in(_env: Env, amount_in: i128, _path: Vec<Address>) -> i128 {
        amount_in
    }
    /// Returns amount_out as the quoted input cost (1:1 exchange rate for test simplicity).
    pub fn quote_exact_out(_env: Env, amount_out: i128, _path: Vec<Address>) -> i128 {
        amount_out
    }
}

struct T {
    env: Env,
    guard: SoroswapTradeGuardClient<'static>,
    vault: Address,
    manager: Address,
    strategy: Address,
    token_a: Address,
    token_b: Address,
    token_c: Address,
}

fn setup() -> T {
    let env = Env::default();
    env.mock_all_auths();

    let manager = Address::generate(&env);
    let vault = env.register(MockVault, ());
    MockVaultClient::new(&env, &vault).set_manager(&manager);

    let strategy = env.register(MockStrategy, ());

    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);
    let token_c = Address::generate(&env);

    let gid = env.register(SoroswapTradeGuard, ());
    let guard = SoroswapTradeGuardClient::new(&env, &gid);

    let tokens: Vec<Address> = vec![&env, token_a.clone(), token_b.clone(), token_c.clone()];
    guard.initialize(&vault, &manager, &tokens, &strategy);

    let guard: SoroswapTradeGuardClient<'static> = unsafe { core::mem::transmute(guard) };

    T {
        env,
        guard,
        vault,
        manager,
        strategy,
        token_a,
        token_b,
        token_c,
    }
}

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_stores_data() {
    let t = setup();
    assert_eq!(t.guard.get_vault(), t.vault);
    assert_eq!(t.guard.get_manager(), t.manager);
    let wl = t.guard.get_whitelist();
    assert_eq!(wl.len(), 3);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_double_initialize_panics() {
    let t = setup();
    let tokens: Vec<Address> = vec![&t.env, t.token_a.clone()];
    t.guard.initialize(&t.vault, &t.manager, &tokens, &t.strategy);
}

// ---------------------------------------------------------------------------
// set_whitelist
// ---------------------------------------------------------------------------

#[test]
fn test_set_whitelist_by_manager() {
    let t = setup();
    let new_tokens: Vec<Address> = vec![&t.env, t.token_a.clone()];
    t.guard.set_whitelist(&t.manager, &new_tokens);
    assert_eq!(t.guard.get_whitelist().len(), 1);
}

#[test]
#[should_panic]
fn test_set_whitelist_not_manager_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    let tokens: Vec<Address> = vec![&t.env, t.token_a.clone()];
    t.guard.set_whitelist(&rogue, &tokens);
}

// ---------------------------------------------------------------------------
// validate_swap_exact_in
// ---------------------------------------------------------------------------

#[test]
fn test_validate_exact_in_valid() {
    let t = setup();
    let path: Vec<Address> = vec![&t.env, t.token_a.clone(), t.token_b.clone()];
    // amount_in=1000, min_out=950 → slippage=5% which is <= 10%
    t.guard
        .validate_swap_exact_in(&t.vault, &1_000i128, &950i128, &path);
}

#[test]
#[should_panic]
fn test_validate_exact_in_not_vault_panics() {
    let t = setup();
    let path: Vec<Address> = vec![&t.env, t.token_a.clone(), t.token_b.clone()];
    let rogue = Address::generate(&t.env);
    t.guard
        .validate_swap_exact_in(&rogue, &1_000i128, &950i128, &path);
}

#[test]
#[should_panic]
fn test_validate_exact_in_zero_amount_panics() {
    let t = setup();
    let path: Vec<Address> = vec![&t.env, t.token_a.clone(), t.token_b.clone()];
    t.guard
        .validate_swap_exact_in(&t.vault, &0i128, &0i128, &path);
}

#[test]
#[should_panic]
fn test_validate_exact_in_negative_amount_panics() {
    let t = setup();
    let path: Vec<Address> = vec![&t.env, t.token_a.clone(), t.token_b.clone()];
    t.guard
        .validate_swap_exact_in(&t.vault, &-1i128, &0i128, &path);
}

#[test]
#[should_panic]
fn test_validate_exact_in_path_too_short_panics() {
    let t = setup();
    let path: Vec<Address> = vec![&t.env, t.token_a.clone()];
    t.guard
        .validate_swap_exact_in(&t.vault, &1_000i128, &900i128, &path);
}

#[test]
#[should_panic]
fn test_validate_exact_in_path_too_long_panics() {
    let t = setup();
    let extra_1 = Address::generate(&t.env);
    let extra_2 = Address::generate(&t.env);
    let extra_3 = Address::generate(&t.env);
    // Add extras to whitelist first.
    let tokens: Vec<Address> = vec![
        &t.env,
        t.token_a.clone(),
        t.token_b.clone(),
        t.token_c.clone(),
        extra_1.clone(),
        extra_2.clone(),
        extra_3.clone(),
    ];
    t.guard.set_whitelist(&t.manager, &tokens);
    // Path of 6 tokens (> MAX_PATH_LEN=5).
    let path: Vec<Address> = vec![
        &t.env,
        t.token_a.clone(),
        t.token_b.clone(),
        t.token_c.clone(),
        extra_1,
        extra_2,
        extra_3,
    ];
    t.guard
        .validate_swap_exact_in(&t.vault, &1_000i128, &900i128, &path);
}

#[test]
#[should_panic]
fn test_validate_exact_in_token_not_whitelisted_panics() {
    let t = setup();
    let unlisted = Address::generate(&t.env);
    let path: Vec<Address> = vec![&t.env, t.token_a.clone(), unlisted];
    t.guard
        .validate_swap_exact_in(&t.vault, &1_000i128, &900i128, &path);
}

#[test]
#[should_panic]
fn test_validate_exact_in_slippage_too_high_panics() {
    let t = setup();
    let path: Vec<Address> = vec![&t.env, t.token_a.clone(), t.token_b.clone()];
    // amount_in=1000, min_out=0 → slippage=100% which is > 10%
    t.guard
        .validate_swap_exact_in(&t.vault, &1_000i128, &0i128, &path);
}

#[test]
fn test_validate_exact_in_exactly_at_slippage_limit() {
    let t = setup();
    let path: Vec<Address> = vec![&t.env, t.token_a.clone(), t.token_b.clone()];
    // MAX_SLIPPAGE_BPS = 1000 = 10 %
    // amount_in=1000, min_out=900 → (1000-900)/1000 = 10% exactly → should pass
    t.guard
        .validate_swap_exact_in(&t.vault, &1_000i128, &900i128, &path);
}

// ---------------------------------------------------------------------------
// validate_swap_exact_out
// ---------------------------------------------------------------------------

#[test]
fn test_validate_exact_out_valid() {
    let t = setup();
    let path: Vec<Address> = vec![&t.env, t.token_a.clone(), t.token_b.clone()];
    t.guard
        .validate_swap_exact_out(&t.vault, &950i128, &1_000i128, &path);
}

#[test]
#[should_panic]
fn test_validate_exact_out_not_vault_panics() {
    let t = setup();
    let path: Vec<Address> = vec![&t.env, t.token_a.clone(), t.token_b.clone()];
    let rogue = Address::generate(&t.env);
    t.guard
        .validate_swap_exact_out(&rogue, &950i128, &1_000i128, &path);
}

#[test]
fn test_validate_exact_out_multi_hop() {
    let t = setup();
    // Three-hop path: A → B → C, all whitelisted.
    // MockStrategy returns 1:1 quote, so quoted_in = amount_out = 900.
    // max_in = 990 → (990-900)/900 = 10% exactly = MAX_SLIPPAGE_BPS → passes.
    let path: Vec<Address> = vec![
        &t.env,
        t.token_a.clone(),
        t.token_b.clone(),
        t.token_c.clone(),
    ];
    t.guard
        .validate_swap_exact_out(&t.vault, &900i128, &990i128, &path);
}

#[test]
#[should_panic]
fn test_validate_exact_out_slippage_too_high_panics() {
    let t = setup();
    let path: Vec<Address> = vec![&t.env, t.token_a.clone(), t.token_b.clone()];
    // MockStrategy returns 1:1 quote: quoted_in = 800. max_in = 1000.
    // slippage = (1000-800)/800 = 25% > 10% → rejected.
    t.guard
        .validate_swap_exact_out(&t.vault, &800i128, &1_000i128, &path);
}

// ---------------------------------------------------------------------------
// NotInitialized — calling functions before initialize() panics
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_not_initialized_get_vault_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(SoroswapTradeGuard, ());
    let client = SoroswapTradeGuardClient::new(&env, &id);
    client.get_vault();
}

#[test]
#[should_panic]
fn test_not_initialized_validate_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(SoroswapTradeGuard, ());
    let client = SoroswapTradeGuardClient::new(&env, &id);
    let vault = Address::generate(&env);
    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);
    let path: Vec<Address> = vec![&env, token_a, token_b];
    client.validate_swap_exact_in(&vault, &1_000i128, &900i128, &path);
}

#[test]
#[should_panic]
fn test_not_initialized_set_whitelist_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(SoroswapTradeGuard, ());
    let client = SoroswapTradeGuardClient::new(&env, &id);
    let manager = Address::generate(&env);
    let tokens: Vec<Address> = Vec::new(&env);
    client.set_whitelist(&manager, &tokens);
}

// ---------------------------------------------------------------------------
// Edge: whitelist updated mid-test — previously valid path now rejected
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_whitelist_update_invalidates_previously_valid_path() {
    let t = setup();
    // token_c is initially whitelisted; after update it's removed.
    let new_tokens: Vec<Address> = vec![&t.env, t.token_a.clone(), t.token_b.clone()];
    t.guard.set_whitelist(&t.manager, &new_tokens);

    let path: Vec<Address> = vec![&t.env, t.token_a.clone(), t.token_c.clone()]; // token_c removed
    t.guard
        .validate_swap_exact_in(&t.vault, &1_000i128, &900i128, &path);
}

// ---------------------------------------------------------------------------
// validate_swap_exact_out — zero amount panics
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_validate_exact_out_zero_amount_panics() {
    let t = setup();
    let path: Vec<Address> = vec![&t.env, t.token_a.clone(), t.token_b.clone()];
    t.guard
        .validate_swap_exact_out(&t.vault, &0i128, &0i128, &path);
}

#[test]
#[should_panic]
fn test_validate_exact_out_path_too_short_panics() {
    let t = setup();
    let path: Vec<Address> = vec![&t.env, t.token_a.clone()];
    t.guard
        .validate_swap_exact_out(&t.vault, &1_000i128, &1_000i128, &path);
}

// ---------------------------------------------------------------------------
// Edge: negative min_out in exact-in — slippage denominator issue
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_validate_exact_in_negative_min_out_panics() {
    let t = setup();
    let path: Vec<Address> = vec![&t.env, t.token_a.clone(), t.token_b.clone()];
    t.guard
        .validate_swap_exact_in(&t.vault, &1_000i128, &-1i128, &path);
}

// ---------------------------------------------------------------------------
// Whitelist empty — any path is rejected
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_empty_whitelist_rejects_all_tokens() {
    let t = setup();
    let empty: Vec<Address> = Vec::new(&t.env);
    t.guard.set_whitelist(&t.manager, &empty);

    let path: Vec<Address> = vec![&t.env, t.token_a.clone(), t.token_b.clone()];
    t.guard
        .validate_swap_exact_in(&t.vault, &1_000i128, &900i128, &path);
}
