#![cfg(test)]

use soroban_sdk::{contract, contractimpl, contracttype, testutils::Address as _, vec, Address, Env, Vec};

use crate::{PhoenixTradeGuard, PhoenixTradeGuardClient, SwapOperation};

// ---------------------------------------------------------------------------
// MockVault — satisfies guard.__constructor's vault.get_manager() cross-call
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
// MockRouter — returns amount_in as quote (1:1 exchange rate for tests)
// ---------------------------------------------------------------------------

#[contract]
pub struct MockRouter;

#[contractimpl]
impl MockRouter {
    pub fn quote_exact_in(_env: Env, amount_in: i128, _path: Vec<Address>) -> i128 {
        amount_in
    }
}

struct T {
    env: Env,
    guard: PhoenixTradeGuardClient<'static>,
    vault: Address,
    manager: Address,
    router: Address,
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
    let router = env.register(MockRouter, ());
    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);
    let token_c = Address::generate(&env);

    let tokens: Vec<Address> = vec![&env, token_a.clone(), token_b.clone(), token_c.clone()];
    let gid = env.register(PhoenixTradeGuard, (&vault, &manager, tokens.clone(), &router));
    let guard = PhoenixTradeGuardClient::new(&env, &gid);

    let guard: PhoenixTradeGuardClient<'static> = unsafe { core::mem::transmute(guard) };

    T {
        env,
        guard,
        vault,
        manager,
        router,
        token_a,
        token_b,
        token_c,
    }
}

fn op(_env: &Env, offer: &Address, ask: &Address) -> SwapOperation {
    SwapOperation {
        offer_asset: offer.clone(),
        ask_asset: ask.clone(),
    }
}

// Double-initialize test is not applicable to __constructor: the Soroban host
// enforces that constructors run exactly once at CreateContractV2 time.  The
// application-level is_initialized guard inside __constructor is defense-in-depth
// against any host that does not enforce this (older protocol versions).

// ---------------------------------------------------------------------------
// set_whitelist
// ---------------------------------------------------------------------------

#[test]
fn test_set_whitelist_by_manager() {
    let t = setup();
    let new_tokens: Vec<Address> = vec![&t.env, t.token_a.clone(), t.token_b.clone()];
    t.guard.set_whitelist(&t.manager, &new_tokens);
    assert_eq!(t.guard.get_whitelist().len(), 2);
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
// validate_swap — happy paths
// ---------------------------------------------------------------------------

#[test]
fn test_validate_single_hop_valid() {
    let t = setup();
    let ops = vec![&t.env, op(&t.env, &t.token_a, &t.token_b)];
    // 5% slippage — within limit.
    t.guard
        .validate_swap(&t.vault, &1_000i128, &950i128, &ops);
}

#[test]
fn test_validate_multi_hop_valid() {
    let t = setup();
    let ops = vec![
        &t.env,
        op(&t.env, &t.token_a, &t.token_b),
        op(&t.env, &t.token_b, &t.token_c),
    ];
    t.guard
        .validate_swap(&t.vault, &1_000i128, &900i128, &ops);
}

#[test]
fn test_validate_exactly_at_slippage_limit() {
    let t = setup();
    let ops = vec![&t.env, op(&t.env, &t.token_a, &t.token_b)];
    // 10% slippage exactly — should pass (boundary inclusive).
    t.guard
        .validate_swap(&t.vault, &1_000i128, &900i128, &ops);
}

// ---------------------------------------------------------------------------
// validate_swap — rejection cases
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_validate_not_vault_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    let ops = vec![&t.env, op(&t.env, &t.token_a, &t.token_b)];
    t.guard
        .validate_swap(&rogue, &1_000i128, &950i128, &ops);
}

#[test]
#[should_panic]
fn test_validate_zero_amount_panics() {
    let t = setup();
    let ops = vec![&t.env, op(&t.env, &t.token_a, &t.token_b)];
    t.guard
        .validate_swap(&t.vault, &0i128, &0i128, &ops);
}

#[test]
#[should_panic]
fn test_validate_negative_amount_panics() {
    let t = setup();
    let ops = vec![&t.env, op(&t.env, &t.token_a, &t.token_b)];
    t.guard
        .validate_swap(&t.vault, &-100i128, &0i128, &ops);
}

#[test]
#[should_panic]
fn test_validate_empty_operations_panics() {
    let t = setup();
    let ops: Vec<SwapOperation> = Vec::new(&t.env);
    t.guard
        .validate_swap(&t.vault, &1_000i128, &900i128, &ops);
}

#[test]
#[should_panic]
fn test_validate_too_many_operations_panics() {
    let t = setup();
    let extra_1 = Address::generate(&t.env);
    let extra_2 = Address::generate(&t.env);
    let all_tokens: Vec<Address> = vec![
        &t.env,
        t.token_a.clone(),
        t.token_b.clone(),
        t.token_c.clone(),
        extra_1.clone(),
        extra_2.clone(),
    ];
    t.guard.set_whitelist(&t.manager, &all_tokens);
    // MAX_OPERATIONS = 4; build 5 ops.
    let ops = vec![
        &t.env,
        op(&t.env, &t.token_a, &t.token_b),
        op(&t.env, &t.token_b, &t.token_c),
        op(&t.env, &t.token_c, &extra_1),
        op(&t.env, &extra_1, &extra_2),
        op(&t.env, &extra_2, &t.token_a),
    ];
    t.guard
        .validate_swap(&t.vault, &1_000i128, &900i128, &ops);
}

#[test]
#[should_panic]
fn test_validate_offer_not_whitelisted_panics() {
    let t = setup();
    let unlisted = Address::generate(&t.env);
    let ops = vec![&t.env, op(&t.env, &unlisted, &t.token_b)];
    t.guard
        .validate_swap(&t.vault, &1_000i128, &900i128, &ops);
}

#[test]
#[should_panic]
fn test_validate_ask_not_whitelisted_panics() {
    let t = setup();
    let unlisted = Address::generate(&t.env);
    let ops = vec![&t.env, op(&t.env, &t.token_a, &unlisted)];
    t.guard
        .validate_swap(&t.vault, &1_000i128, &900i128, &ops);
}

#[test]
#[should_panic]
fn test_validate_slippage_too_high_panics() {
    let t = setup();
    let ops = vec![&t.env, op(&t.env, &t.token_a, &t.token_b)];
    // 50% slippage → rejected.
    t.guard
        .validate_swap(&t.vault, &1_000i128, &500i128, &ops);
}

#[test]
#[should_panic]
fn test_validate_negative_min_out_panics() {
    let t = setup();
    let ops = vec![&t.env, op(&t.env, &t.token_a, &t.token_b)];
    t.guard
        .validate_swap(&t.vault, &1_000i128, &-1i128, &ops);
}

#[test]
fn test_validate_max_operations_exactly() {
    let t = setup();
    let extra_1 = Address::generate(&t.env);
    let extra_2 = Address::generate(&t.env);
    let all_tokens: Vec<Address> = vec![
        &t.env,
        t.token_a.clone(),
        t.token_b.clone(),
        t.token_c.clone(),
        extra_1.clone(),
        extra_2.clone(),
    ];
    t.guard.set_whitelist(&t.manager, &all_tokens);
    // Exactly MAX_OPERATIONS=4 ops — should pass.
    let ops = vec![
        &t.env,
        op(&t.env, &t.token_a, &t.token_b),
        op(&t.env, &t.token_b, &t.token_c),
        op(&t.env, &t.token_c, &extra_1),
        op(&t.env, &extra_1, &extra_2),
    ];
    t.guard
        .validate_swap(&t.vault, &1_000i128, &900i128, &ops);
}

// ---------------------------------------------------------------------------
// Constructor stores data correctly
// ---------------------------------------------------------------------------

#[test]
fn test_constructor_stores_data() {
    let t = setup();
    assert_eq!(t.guard.get_vault(), t.vault);
    assert_eq!(t.guard.get_manager(), t.manager);
    assert_eq!(t.guard.get_router(), t.router);
    assert_eq!(t.guard.get_whitelist().len(), 3);
}

// ---------------------------------------------------------------------------
// Whitelist update invalidates previously valid operations
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_whitelist_update_invalidates_ops() {
    let t = setup();
    // Remove token_c from whitelist.
    let new_tokens: Vec<Address> = vec![&t.env, t.token_a.clone(), t.token_b.clone()];
    t.guard.set_whitelist(&t.manager, &new_tokens);

    let ops = vec![&t.env, op(&t.env, &t.token_a, &t.token_c)]; // token_c removed
    t.guard
        .validate_swap(&t.vault, &1_000i128, &900i128, &ops);
}

// ---------------------------------------------------------------------------
// Empty whitelist rejects all operations
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_empty_whitelist_rejects_all() {
    let t = setup();
    let empty: Vec<Address> = Vec::new(&t.env);
    t.guard.set_whitelist(&t.manager, &empty);

    let ops = vec![&t.env, op(&t.env, &t.token_a, &t.token_b)];
    t.guard
        .validate_swap(&t.vault, &1_000i128, &900i128, &ops);
}

// ---------------------------------------------------------------------------
// Boundary: amount = 1 (minimum valid)
// ---------------------------------------------------------------------------

#[test]
fn test_validate_amount_one_valid() {
    let t = setup();
    let ops = vec![&t.env, op(&t.env, &t.token_a, &t.token_b)];
    // min_out = 0 would exceed slippage, but amount=1, min_out=1 → 0% slippage.
    t.guard
        .validate_swap(&t.vault, &1i128, &1i128, &ops);
}

// ---------------------------------------------------------------------------
// Single-operation round-trip (offer == ask allowed structurally)
// ---------------------------------------------------------------------------

#[test]
fn test_validate_same_token_both_sides() {
    let t = setup();
    // Not a practical swap but structurally valid for the guard.
    let ops = vec![&t.env, op(&t.env, &t.token_a, &t.token_a)];
    t.guard
        .validate_swap(&t.vault, &1_000i128, &900i128, &ops);
}

// ---------------------------------------------------------------------------
// Hop-continuity enforcement (M4)
// ---------------------------------------------------------------------------

/// Non-contiguous hops must be rejected: operations[0].ask_asset ≠ operations[1].offer_asset
/// allows a crafted path that passes whitelist checks but routes through a different asset.
#[test]
#[should_panic]
fn test_non_contiguous_hops_rejected() {
    let t = setup();
    // operations[0]: A → B
    // operations[1]: C → A  ← gap: B ≠ C
    let ops = vec![
        &t.env,
        op(&t.env, &t.token_a, &t.token_b),
        op(&t.env, &t.token_c, &t.token_a), // offer_asset=C != prev ask_asset=B
    ];
    t.guard
        .validate_swap(&t.vault, &1_000i128, &900i128, &ops);
}
