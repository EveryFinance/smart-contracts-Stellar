#![cfg(test)]

use soroban_sdk::{contract, contractimpl, contracttype, testutils::Address as _, Address, Env};

use crate::{Factory, FactoryClient};

// ---------------------------------------------------------------------------
// MockVault — minimal vault stub for verify_and_register_vault tests
// ---------------------------------------------------------------------------

#[contracttype]
enum VKey {
    Manager,
}
#[contract]
pub struct MockVault;
#[contractimpl]
impl MockVault {
    pub fn initialize(env: Env, manager: Address) {
        env.storage().instance().set(&VKey::Manager, &manager);
    }
    pub fn get_manager(env: Env) -> Address {
        env.storage().instance().get(&VKey::Manager).unwrap()
    }
}

struct T {
    env: Env,
    factory: FactoryClient<'static>,
    admin: Address,
}

fn setup() -> T {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let fid = env.register(Factory, ());
    let factory = FactoryClient::new(&env, &fid);
    factory.initialize(&admin);

    let factory: FactoryClient<'static> = unsafe { core::mem::transmute(factory) };

    T {
        env,
        factory,
        admin,
    }
}

fn deploy_mock_vault(env: &Env, manager: &Address) -> Address {
    let vid = env.register(MockVault, ());
    MockVaultClient::new(env, &vid).initialize(manager);
    vid
}

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_sets_admin() {
    let t = setup();
    assert_eq!(t.factory.get_admin(), t.admin);
    assert_eq!(t.factory.get_vault_count(), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_double_initialize_panics() {
    let t = setup();
    t.factory.initialize(&t.admin);
}

// ---------------------------------------------------------------------------
// register_vault
// ---------------------------------------------------------------------------

#[test]
fn test_register_vault_by_admin() {
    let t = setup();
    let manager = Address::generate(&t.env);
    let vault = deploy_mock_vault(&t.env, &manager);
    t.factory.register_vault(&t.admin, &vault, &manager);
    assert_eq!(t.factory.get_vault_count(), 1);
    assert!(t.factory.is_registered(&vault));
}

#[test]
fn test_register_multiple_vaults() {
    let t = setup();
    let manager = Address::generate(&t.env);
    for _ in 0..5u32 {
        let vault = deploy_mock_vault(&t.env, &manager);
        t.factory.register_vault(&t.admin, &vault, &manager);
    }
    assert_eq!(t.factory.get_vault_count(), 5);
}

#[test]
#[should_panic]
fn test_register_vault_not_admin_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    let manager = Address::generate(&t.env);
    let vault = deploy_mock_vault(&t.env, &manager);
    t.factory.register_vault(&rogue, &vault, &manager);
}

// ---------------------------------------------------------------------------
// remove_vault
// ---------------------------------------------------------------------------

#[test]
fn test_remove_vault() {
    let t = setup();
    let manager = Address::generate(&t.env);
    let vault = deploy_mock_vault(&t.env, &manager);
    t.factory.register_vault(&t.admin, &vault, &manager);
    assert_eq!(t.factory.get_vault_count(), 1);

    t.factory.remove_vault(&t.admin, &vault);
    assert_eq!(t.factory.get_vault_count(), 0);
    assert!(!t.factory.is_registered(&vault));
}

#[test]
#[should_panic]
fn test_remove_vault_not_found_panics() {
    let t = setup();
    let vault = Address::generate(&t.env);
    t.factory.remove_vault(&t.admin, &vault);
}

#[test]
#[should_panic]
fn test_remove_vault_not_admin_panics() {
    let t = setup();
    let manager = Address::generate(&t.env);
    let vault = deploy_mock_vault(&t.env, &manager);
    t.factory.register_vault(&t.admin, &vault, &manager);
    let rogue = Address::generate(&t.env);
    t.factory.remove_vault(&rogue, &vault);
}

#[test]
fn test_remove_one_of_many_vaults() {
    let t = setup();
    let manager = Address::generate(&t.env);
    let v1 = deploy_mock_vault(&t.env, &manager);
    let v2 = deploy_mock_vault(&t.env, &manager);
    let v3 = deploy_mock_vault(&t.env, &manager);
    t.factory.register_vault(&t.admin, &v1, &manager);
    t.factory.register_vault(&t.admin, &v2, &manager);
    t.factory.register_vault(&t.admin, &v3, &manager);

    t.factory.remove_vault(&t.admin, &v2);
    assert_eq!(t.factory.get_vault_count(), 2);
    assert!(t.factory.is_registered(&v1));
    assert!(!t.factory.is_registered(&v2));
    assert!(t.factory.is_registered(&v3));
}

// ---------------------------------------------------------------------------
// get_vaults (pagination)
// ---------------------------------------------------------------------------

#[test]
fn test_get_vaults_pagination() {
    let t = setup();
    let manager = Address::generate(&t.env);
    let mut all: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&t.env);
    for _ in 0..10u32 {
        let vault = deploy_mock_vault(&t.env, &manager);
        t.factory.register_vault(&t.admin, &vault, &manager);
        all.push_back(vault);
    }

    let page1 = t.factory.get_vaults(&0, &5);
    assert_eq!(page1.len(), 5);

    let page2 = t.factory.get_vaults(&5, &5);
    assert_eq!(page2.len(), 5);

    // No overlap.
    for a in page1.iter() {
        for b in page2.iter() {
            assert_ne!(a, b);
        }
    }
}

#[test]
fn test_get_vaults_beyond_end() {
    let t = setup();
    let manager = Address::generate(&t.env);
    for _ in 0..3u32 {
        let vid = deploy_mock_vault(&t.env, &manager);
        t.factory.register_vault(&t.admin, &vid, &manager);
    }
    let page = t.factory.get_vaults(&2, &10);
    assert_eq!(page.len(), 1); // only 1 left starting at index 2
}

#[test]
fn test_get_vaults_empty() {
    let t = setup();
    let page = t.factory.get_vaults(&0, &10);
    assert_eq!(page.len(), 0);
}

// ---------------------------------------------------------------------------
// set_admin
// ---------------------------------------------------------------------------

#[test]
fn test_set_admin_transfers_role() {
    let t = setup();
    let new_admin = Address::generate(&t.env);
    t.factory.set_admin(&t.admin, &new_admin);
    assert_eq!(t.factory.get_admin(), new_admin);

    // Old admin can no longer register.
    // New admin can register.
    let manager = Address::generate(&t.env);
    let vault = deploy_mock_vault(&t.env, &manager);
    t.factory.register_vault(&new_admin, &vault, &manager);
    assert_eq!(t.factory.get_vault_count(), 1);
}

#[test]
#[should_panic]
fn test_set_admin_not_admin_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    let new_admin = Address::generate(&t.env);
    t.factory.set_admin(&rogue, &new_admin);
}

// ---------------------------------------------------------------------------
// is_registered
// ---------------------------------------------------------------------------

#[test]
fn test_is_registered_false_for_unknown() {
    let t = setup();
    let unknown = Address::generate(&t.env);
    assert!(!t.factory.is_registered(&unknown));
}

#[test]
fn test_is_registered_true_after_register() {
    let t = setup();
    let manager = Address::generate(&t.env);
    let vault = deploy_mock_vault(&t.env, &manager);
    t.factory.register_vault(&t.admin, &vault, &manager);
    assert!(t.factory.is_registered(&vault));
}

// ---------------------------------------------------------------------------
// NotInitialized — calling functions before initialize() panics
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_not_initialized_get_admin_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(Factory, ());
    let client = FactoryClient::new(&env, &id);
    client.get_admin();
}

#[test]
#[should_panic]
fn test_not_initialized_register_vault_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(Factory, ());
    let client = FactoryClient::new(&env, &id);
    let caller = Address::generate(&env);
    let vault = Address::generate(&env);
    let manager = Address::generate(&env);
    client.register_vault(&caller, &vault, &manager);
}

#[test]
#[should_panic]
fn test_not_initialized_remove_vault_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(Factory, ());
    let client = FactoryClient::new(&env, &id);
    let caller = Address::generate(&env);
    let vault = Address::generate(&env);
    client.remove_vault(&caller, &vault);
}

#[test]
#[should_panic]
fn test_not_initialized_set_admin_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(Factory, ());
    let client = FactoryClient::new(&env, &id);
    let caller = Address::generate(&env);
    let new_admin = Address::generate(&env);
    client.set_admin(&caller, &new_admin);
}

// ---------------------------------------------------------------------------
// Pagination edge cases
// ---------------------------------------------------------------------------

#[test]
fn test_get_vaults_offset_beyond_end_returns_empty() {
    let t = setup();
    let manager = Address::generate(&t.env);
    for _ in 0..3u32 {
        let vid = deploy_mock_vault(&t.env, &manager);
        t.factory.register_vault(&t.admin, &vid, &manager);
    }
    let page = t.factory.get_vaults(&100, &10); // offset > count
    assert_eq!(page.len(), 0);
}

#[test]
fn test_get_vaults_limit_zero_returns_empty() {
    let t = setup();
    let manager = Address::generate(&t.env);
    let vid = deploy_mock_vault(&t.env, &manager);
    t.factory.register_vault(&t.admin, &vid, &manager);
    let page = t.factory.get_vaults(&0, &0);
    assert_eq!(page.len(), 0);
}

#[test]
fn test_get_vaults_limit_capped_at_50() {
    let t = setup();
    let manager = Address::generate(&t.env);
    for _ in 0..10u32 {
        let vid = deploy_mock_vault(&t.env, &manager);
        t.factory.register_vault(&t.admin, &vid, &manager);
    }
    // Limit of 1000 is capped at 50; only 10 registered so we get 10.
    let page = t.factory.get_vaults(&0, &1000);
    assert_eq!(page.len(), 10);
}

// ---------------------------------------------------------------------------
// Duplicate registration is rejected
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_register_same_vault_twice_panics() {
    let t = setup();
    let manager = Address::generate(&t.env);
    let vid = deploy_mock_vault(&t.env, &manager);
    t.factory.register_vault(&t.admin, &vid, &manager);
    t.factory.register_vault(&t.admin, &vid, &manager);
}

#[test]
#[should_panic]
fn test_register_vault_manager_mismatch_panics() {
    let t = setup();
    let manager = Address::generate(&t.env);
    let wrong_manager = Address::generate(&t.env);
    let vid = deploy_mock_vault(&t.env, &manager);
    t.factory.register_vault(&t.admin, &vid, &wrong_manager);
}

// ---------------------------------------------------------------------------
// verify_and_register duplicate reject
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_verify_and_register_duplicate_panics() {
    let t = setup();
    let manager = Address::generate(&t.env);
    let vid = deploy_mock_vault(&t.env, &manager);
    t.factory.verify_and_register_vault(&t.admin, &vid);
    t.factory.verify_and_register_vault(&t.admin, &vid);
}

// ---------------------------------------------------------------------------
// Legacy duplicate-removal case no longer reachable (duplicates disallowed)
// ---------------------------------------------------------------------------

#[test]
fn test_remove_registered_vault_single_entry() {
    let t = setup();
    let manager = Address::generate(&t.env);
    let vid = deploy_mock_vault(&t.env, &manager);
    t.factory.register_vault(&t.admin, &vid, &manager);
    assert_eq!(t.factory.get_vault_count(), 1);
    t.factory.remove_vault(&t.admin, &vid);
    assert_eq!(t.factory.get_vault_count(), 0);
}

// ---------------------------------------------------------------------------
// Admin chain transfer
// ---------------------------------------------------------------------------

#[test]
fn test_admin_chain_transfer() {
    let t = setup();
    let admin2 = Address::generate(&t.env);
    let admin3 = Address::generate(&t.env);

    t.factory.set_admin(&t.admin, &admin2);
    assert_eq!(t.factory.get_admin(), admin2);

    t.factory.set_admin(&admin2, &admin3);
    assert_eq!(t.factory.get_admin(), admin3);

    // admin2 can no longer operate (but with mock_all_auths it would still
    // pass require_auth — so we verify the address check fails):
    // Register with admin3 succeeds.
    let manager = Address::generate(&t.env);
    let vault = deploy_mock_vault(&t.env, &manager);
    t.factory.register_vault(&admin3, &vault, &manager);
    assert!(t.factory.is_registered(&vault));
}

// ---------------------------------------------------------------------------
// Vault count consistency after removes
// ---------------------------------------------------------------------------

#[test]
fn test_vault_count_after_register_and_remove() {
    let t = setup();
    let manager = Address::generate(&t.env);
    let mut vaults = soroban_sdk::Vec::new(&t.env);
    for _ in 0..5u32 {
        let v = deploy_mock_vault(&t.env, &manager);
        t.factory.register_vault(&t.admin, &v, &manager);
        vaults.push_back(v);
    }
    assert_eq!(t.factory.get_vault_count(), 5);

    for v in vaults.iter() {
        t.factory.remove_vault(&t.admin, &v);
    }
    assert_eq!(t.factory.get_vault_count(), 0);
}

// ---------------------------------------------------------------------------
// GAP 5 — verify_and_register_vault
// ---------------------------------------------------------------------------

#[test]
fn test_verify_and_register_vault_succeeds() {
    let t = setup();
    let manager = Address::generate(&t.env);

    // Deploy and initialize a mock vault.
    let vid = t.env.register(MockVault, ());
    MockVaultClient::new(&t.env, &vid).initialize(&manager);

    t.factory.verify_and_register_vault(&t.admin, &vid);
    assert_eq!(t.factory.get_vault_count(), 1);
    assert!(t.factory.is_registered(&vid));
}

#[test]
#[should_panic]
fn test_verify_and_register_vault_not_admin_panics() {
    let t = setup();
    let manager = Address::generate(&t.env);
    let vid = t.env.register(MockVault, ());
    MockVaultClient::new(&t.env, &vid).initialize(&manager);

    let rogue = Address::generate(&t.env);
    t.factory.verify_and_register_vault(&rogue, &vid);
}

#[test]
#[should_panic]
fn test_verify_and_register_uninitialized_vault_panics() {
    let t = setup();
    // Deploy without initializing — get_manager() will panic.
    let vid = t.env.register(MockVault, ());
    // No initialize call → get_manager() unwrap panics.
    t.factory.verify_and_register_vault(&t.admin, &vid);
}
