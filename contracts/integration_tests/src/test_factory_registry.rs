//! Integration tests: Factory ↔ Vault
//!
//! Verifies the full factory registry lifecycle using real Factory and Vault
//! contracts: registration, is_registered, paginated queries, vault removal,
//! admin transfer, and count consistency.

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env, String};

use factory::{Factory, FactoryClient};
use share_token::{ShareTokenContract, ShareTokenContractClient};
use vault::{Vault, VaultClient, VaultParams};

use crate::common::{MockToken, MockTokenClient};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Deploy and fully initialize a vault.  Returns (vault_id, manager).
fn deploy_vault(env: &Env) -> (Address, Address) {
    let manager = Address::generate(env);
    let trader = Address::generate(env);

    let base = env.register(MockToken, ());
    MockTokenClient::new(env, &base).initialize(&manager);

    let share_id = env.register(
        ShareTokenContract,
        (
            manager.clone(),
            String::from_str(env, "VS"),
            String::from_str(env, "VS"),
            7u32,
        ),
    );

    let vault_id = env.register(
        Vault,
        (VaultParams {
            manager: manager.clone(),
            trader: trader.clone(),
            base_asset: base,
            share_token: share_id,
            share_token_admin: manager.clone(),
            entry_fee_bps: 0,
            exit_fee_bps: 0,
            mgmt_fee_bps: 0,
            perf_fee_bps: 0,
        },),
    );

    (vault_id, manager)
}

struct World {
    env: Env,
    factory_id: Address,
    admin: Address,
}

impl World {
    /// Construct a `FactoryClient` on demand. The client borrows from `self.env`
    /// so its lifetime is tied to the `World` reference — no unsafe transmute needed.
    fn factory(&self) -> FactoryClient<'_> {
        FactoryClient::new(&self.env, &self.factory_id)
    }
}

fn setup() -> World {
    let env = Env::default();
    // NOTE: mock_all_auths() bypasses all authorization checks in these tests.
    // This means auth-specific failure paths (e.g. non-admin calling admin-only
    // functions) are not verified here. Auth enforcement is tested in unit tests
    // for each contract where specific require_auth paths are exercised without
    // this blanket override.
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let factory_id = env.register(Factory, (admin.clone(),));
    World {
        env,
        factory_id,
        admin,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Factory initializes with zero vaults and correct admin.
#[test]
fn test_factory_initializes_empty() {
    let w = setup();
    assert_eq!(w.factory().get_vault_count(), 0);
    assert_eq!(w.factory().get_admin(), w.admin);
    let page = w.factory().get_vaults(&0, &10);
    assert_eq!(page.len(), 0);
}

/// Registering one vault makes it findable via is_registered and get_vault_count.
#[test]
fn test_register_single_vault() {
    let w = setup();
    let (vault_id, manager) = deploy_vault(&w.env);

    w.factory().register_vault(&w.admin, &vault_id, &manager);

    assert_eq!(w.factory().get_vault_count(), 1);
    assert!(w.factory().is_registered(&vault_id));
}

/// Registering three vaults accumulates count correctly.
#[test]
fn test_register_multiple_vaults_count() {
    let w = setup();

    for _ in 0..3u32 {
        let (vault_id, manager) = deploy_vault(&w.env);
        w.factory().register_vault(&w.admin, &vault_id, &manager);
    }

    assert_eq!(w.factory().get_vault_count(), 3);
}

/// is_registered returns false for an address never registered.
#[test]
fn test_is_registered_unknown_vault_false() {
    let w = setup();
    let random = Address::generate(&w.env);
    assert!(!w.factory().is_registered(&random));
}

/// Removing a registered vault decrements the count and is_registered → false.
#[test]
fn test_remove_vault_decrements_count() {
    let w = setup();
    let (v1, m1) = deploy_vault(&w.env);
    let (v2, m2) = deploy_vault(&w.env);

    w.factory().register_vault(&w.admin, &v1, &m1);
    w.factory().register_vault(&w.admin, &v2, &m2);
    assert_eq!(w.factory().get_vault_count(), 2);

    w.factory().remove_vault(&w.admin, &v1);

    assert_eq!(w.factory().get_vault_count(), 1);
    assert!(!w.factory().is_registered(&v1));
    assert!(w.factory().is_registered(&v2));
}

/// Removing a vault not in the registry panics with VaultNotFound.
#[test]
#[should_panic]
fn test_remove_nonexistent_vault_panics() {
    let w = setup();
    let random = Address::generate(&w.env);
    w.factory().remove_vault(&w.admin, &random);
}

/// Non-admin cannot register a vault.
#[test]
#[should_panic]
fn test_register_vault_non_admin_panics() {
    let w = setup();
    let (vault_id, manager) = deploy_vault(&w.env);
    let rogue = Address::generate(&w.env);
    w.factory().register_vault(&rogue, &vault_id, &manager);
}

/// Non-admin cannot remove a vault.
#[test]
#[should_panic]
fn test_remove_vault_non_admin_panics() {
    let w = setup();
    let (vault_id, manager) = deploy_vault(&w.env);
    w.factory().register_vault(&w.admin, &vault_id, &manager);

    let rogue = Address::generate(&w.env);
    w.factory().remove_vault(&rogue, &vault_id);
}

/// Paginated get_vaults returns correct slice (offset=0, limit=2 of 4).
#[test]
fn test_get_vaults_pagination_first_page() {
    let w = setup();

    let (v0, m0) = deploy_vault(&w.env);
    let (v1, m1) = deploy_vault(&w.env);
    let (v2, m2) = deploy_vault(&w.env);
    let (v3, m3) = deploy_vault(&w.env);

    w.factory().register_vault(&w.admin, &v0, &m0);
    w.factory().register_vault(&w.admin, &v1, &m1);
    w.factory().register_vault(&w.admin, &v2, &m2);
    w.factory().register_vault(&w.admin, &v3, &m3);

    let page = w.factory().get_vaults(&0, &2);
    assert_eq!(page.len(), 2);
    assert_eq!(page.get(0).unwrap(), v0);
    assert_eq!(page.get(1).unwrap(), v1);
}

/// Paginated get_vaults returns correct slice (offset=2, limit=2 of 4).
#[test]
fn test_get_vaults_pagination_second_page() {
    let w = setup();

    let (v0, m0) = deploy_vault(&w.env);
    let (v1, m1) = deploy_vault(&w.env);
    let (v2, m2) = deploy_vault(&w.env);
    let (v3, m3) = deploy_vault(&w.env);

    w.factory().register_vault(&w.admin, &v0, &m0);
    w.factory().register_vault(&w.admin, &v1, &m1);
    w.factory().register_vault(&w.admin, &v2, &m2);
    w.factory().register_vault(&w.admin, &v3, &m3);

    let page = w.factory().get_vaults(&2, &2);
    assert_eq!(page.len(), 2);
    assert_eq!(page.get(0).unwrap(), v2);
    assert_eq!(page.get(1).unwrap(), v3);
}

/// get_vaults with offset past the end returns empty.
#[test]
fn test_get_vaults_offset_past_end_returns_empty() {
    let w = setup();
    let (v, m) = deploy_vault(&w.env);
    w.factory().register_vault(&w.admin, &v, &m);

    let page = w.factory().get_vaults(&10, &5);
    assert_eq!(page.len(), 0);
}

/// limit is capped at 50 — requesting more still returns at most 50 entries.
#[test]
fn test_get_vaults_limit_capped_at_50() {
    let w = setup();

    // Register 55 vaults.
    for _ in 0..55u32 {
        let (v, m) = deploy_vault(&w.env);
        w.factory().register_vault(&w.admin, &v, &m);
    }

    let page = w.factory().get_vaults(&0, &100); // ask for 100, cap is 50
    assert_eq!(page.len(), 50);
}

/// Admin transfer: new admin can register vaults after two-step transfer.
#[test]
fn test_admin_transfer_changes_effective_admin() {
    let w = setup();
    let new_admin = Address::generate(&w.env);

    w.factory().set_pending_admin(&w.admin, &new_admin);
    w.factory().accept_admin(&new_admin);
    assert_eq!(w.factory().get_admin(), new_admin);

    // New admin can register.
    let (v, m) = deploy_vault(&w.env);
    w.factory().register_vault(&new_admin, &v, &m);
    assert!(w.factory().is_registered(&v));
}

/// Old admin cannot act after transfer completes.
#[test]
#[should_panic]
fn test_old_admin_cannot_act_after_transfer() {
    let w = setup();
    let new_admin = Address::generate(&w.env);

    w.factory().set_pending_admin(&w.admin, &new_admin);
    w.factory().accept_admin(&new_admin);

    // Old admin tries to register — NotAdmin.
    let (v, m) = deploy_vault(&w.env);
    w.factory().register_vault(&w.admin, &v, &m);
}

/// Non-admin cannot call set_pending_admin.
#[test]
#[should_panic]
fn test_set_admin_non_admin_panics() {
    let w = setup();
    let rogue = Address::generate(&w.env);
    let new_admin = Address::generate(&w.env);
    w.factory().set_pending_admin(&rogue, &new_admin);
}

/// Register → remove → re-register same vault works.
#[test]
fn test_reregister_after_remove() {
    let w = setup();
    let (vault_id, manager) = deploy_vault(&w.env);

    w.factory().register_vault(&w.admin, &vault_id, &manager);
    assert_eq!(w.factory().get_vault_count(), 1);

    w.factory().remove_vault(&w.admin, &vault_id);
    assert_eq!(w.factory().get_vault_count(), 0);
    assert!(!w.factory().is_registered(&vault_id));

    w.factory().register_vault(&w.admin, &vault_id, &manager);
    assert_eq!(w.factory().get_vault_count(), 1);
    assert!(w.factory().is_registered(&vault_id));
}

/// Removing middle element preserves order of remaining vaults.
#[test]
fn test_remove_middle_vault_preserves_order() {
    let w = setup();
    let (v1, m1) = deploy_vault(&w.env);
    let (v2, m2) = deploy_vault(&w.env);
    let (v3, m3) = deploy_vault(&w.env);

    w.factory().register_vault(&w.admin, &v1, &m1);
    w.factory().register_vault(&w.admin, &v2, &m2);
    w.factory().register_vault(&w.admin, &v3, &m3);

    // Remove the middle one.
    w.factory().remove_vault(&w.admin, &v2);

    let page = w.factory().get_vaults(&0, &10);
    assert_eq!(page.len(), 2);
    assert_eq!(page.get(0).unwrap(), v1);
    assert_eq!(page.get(1).unwrap(), v3);
}

/// Count is consistent with paginated query length.
#[test]
fn test_count_consistent_with_paginated_total() {
    let w = setup();

    for _ in 0..7u32 {
        let (v, m) = deploy_vault(&w.env);
        w.factory().register_vault(&w.admin, &v, &m);
    }

    let count = w.factory().get_vault_count();
    let page = w.factory().get_vaults(&0, &50);
    assert_eq!(count, page.len());
}

/// touch_vaults is a no-op on an empty registry and does not panic.
#[test]
fn test_touch_vaults_empty_registry_is_noop() {
    let w = setup();
    // No vaults registered — any range should silently return.
    w.factory().touch_vaults(&0, &10);
    assert_eq!(w.factory().get_vault_count(), 0);
}

/// touch_vaults with start >= end is a no-op.
#[test]
fn test_touch_vaults_start_ge_end_is_noop() {
    let w = setup();
    let (v, m) = deploy_vault(&w.env);
    w.factory().register_vault(&w.admin, &v, &m);

    w.factory().touch_vaults(&5, &3); // start > end
    w.factory().touch_vaults(&2, &2); // start == end
    assert_eq!(w.factory().get_vault_count(), 1);
}

/// touch_vaults over all registered entries succeeds without error.
#[test]
fn test_touch_vaults_full_range() {
    let w = setup();

    let mut vaults = soroban_sdk::vec![&w.env];
    for _ in 0..5u32 {
        let (v, m) = deploy_vault(&w.env);
        w.factory().register_vault(&w.admin, &v, &m);
        vaults.push_back(v);
    }

    // Touching the full range must not panic and leave registry intact.
    w.factory().touch_vaults(&0, &5);
    assert_eq!(w.factory().get_vault_count(), 5);

    // Vault order is preserved.
    let page = w.factory().get_vaults(&0, &10);
    assert_eq!(page.len(), 5);
}

/// touch_vaults clamps end to vault_count when u32::MAX is passed.
#[test]
fn test_touch_vaults_end_clamped_to_count() {
    let w = setup();
    let (v, m) = deploy_vault(&w.env);
    w.factory().register_vault(&w.admin, &v, &m);

    // u32::MAX as end should be clamped to count=1, touching only index 0.
    w.factory().touch_vaults(&0, &u32::MAX);
    assert_eq!(w.factory().get_vault_count(), 1);
}

/// touch_vaults with start past the last index is a no-op.
#[test]
fn test_touch_vaults_start_past_count_is_noop() {
    let w = setup();
    let (v, m) = deploy_vault(&w.env);
    w.factory().register_vault(&w.admin, &v, &m);

    // count=1, start=5 — no entries to touch.
    w.factory().touch_vaults(&5, &10);
    assert_eq!(w.factory().get_vault_count(), 1);
}
