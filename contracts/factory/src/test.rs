#![cfg(test)]

use soroban_sdk::{contract, contractimpl, contracttype, testutils::Address as _, Address, Env};

use crate::{Factory, FactoryClient};

#[contracttype]
enum AhKey {
    Registered(Address),
}

#[contract]
pub struct MockAssetHandler;

#[contractimpl]
impl MockAssetHandler {
    pub fn set_registered(env: Env, asset: Address, registered: bool) {
        env.storage()
            .instance()
            .set(&AhKey::Registered(asset), &registered);
    }

    pub fn is_registered(env: Env, asset: Address) -> bool {
        env.storage()
            .instance()
            .get(&AhKey::Registered(asset))
            .unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// MockVault — minimal vault stub for verify_and_register_vault tests
// ---------------------------------------------------------------------------

#[contracttype]
enum VKey {
    Manager,
    SeedDeposited,
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
    pub fn set_manager(env: Env, _caller: Address, new_manager: Address) {
        env.storage().instance().set(&VKey::Manager, &new_manager);
    }
    pub fn seed_deposit(env: Env, _caller: Address, _amount: i128) {
        if env
            .storage()
            .instance()
            .get::<VKey, bool>(&VKey::SeedDeposited)
            .unwrap_or(false)
        {
            panic!("already seeded");
        }
        env.storage().instance().set(&VKey::SeedDeposited, &true);
    }
}

// ---------------------------------------------------------------------------
// MockToken — minimal SEP-41 stub for create_vault tests
// ---------------------------------------------------------------------------

#[contracttype]
enum TKey {
    Balance(Address),
    Allowance(Address, Address),
    TotalSupply,
}
#[contract]
pub struct MockToken;
#[contractimpl]
impl MockToken {
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
    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&TKey::Balance(id))
            .unwrap_or(0)
    }
    pub fn approve(env: Env, from: Address, spender: Address, amount: i128, _expiry: u32) {
        env.storage()
            .persistent()
            .set(&TKey::Allowance(from, spender), &amount);
    }
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        let fb: i128 = env
            .storage()
            .persistent()
            .get(&TKey::Balance(from.clone()))
            .unwrap_or(0);
        assert!(fb >= amount, "insufficient balance");
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
    pub fn transfer_from(env: Env, _spender: Address, from: Address, to: Address, amount: i128) {
        let fb: i128 = env
            .storage()
            .persistent()
            .get(&TKey::Balance(from.clone()))
            .unwrap_or(0);
        assert!(fb >= amount, "insufficient balance");
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
    let fid = env.register(Factory, (admin.clone(), Option::<Address>::None));
    let factory = FactoryClient::new(&env, &fid);

    let factory: FactoryClient<'static> = unsafe { core::mem::transmute(factory) };

    T {
        env,
        factory,
        admin,
    }
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_constructor_rejects_reinitialization() {
    let t = setup();

    t.env.as_contract(&t.factory.address, || {
        Factory::__constructor(t.env.clone(), t.admin.clone(), Option::<Address>::None);
    });
}

fn setup_with_asset_handler() -> (T, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let asset_handler = env.register(MockAssetHandler, ());
    let fid = env.register(Factory, (admin.clone(), Some(asset_handler.clone())));
    let factory = FactoryClient::new(&env, &fid);
    let factory: FactoryClient<'static> = unsafe { core::mem::transmute(factory) };

    (
        T {
            env,
            factory,
            admin,
        },
        asset_handler,
    )
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
fn test_constructor_and_set_asset_handler_views() {
    let (t, asset_handler) = setup_with_asset_handler();
    assert_eq!(t.factory.get_asset_handler(), Some(asset_handler.clone()));

    let replacement = t.env.register(MockAssetHandler, ());
    t.factory.set_asset_handler(&t.admin, &replacement);
    assert_eq!(t.factory.get_asset_handler(), Some(replacement));
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_set_asset_handler_not_admin_panics() {
    let (t, _asset_handler) = setup_with_asset_handler();
    let rogue = Address::generate(&t.env);
    let replacement = t.env.register(MockAssetHandler, ());
    t.factory.set_asset_handler(&rogue, &replacement);
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
// two-step admin transfer
// ---------------------------------------------------------------------------

#[test]
fn test_set_pending_then_accept_admin() {
    let t = setup();
    let new_admin = Address::generate(&t.env);
    t.factory.set_pending_admin(&t.admin, &new_admin);
    t.factory.accept_admin(&new_admin);
    assert_eq!(t.factory.get_admin(), new_admin);

    // New admin can register.
    let manager = Address::generate(&t.env);
    let vault = deploy_mock_vault(&t.env, &manager);
    t.factory.register_vault(&new_admin, &vault, &manager);
    assert_eq!(t.factory.get_vault_count(), 1);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_accept_admin_wrong_pending_panics() {
    let t = setup();
    let pending = Address::generate(&t.env);
    let rogue = Address::generate(&t.env);
    t.factory.set_pending_admin(&t.admin, &pending);
    t.factory.accept_admin(&rogue);
}

#[test]
#[should_panic]
fn test_set_pending_admin_not_admin_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    let new_admin = Address::generate(&t.env);
    t.factory.set_pending_admin(&rogue, &new_admin);
}

#[test]
#[should_panic]
fn test_accept_admin_without_pending_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    t.factory.accept_admin(&rogue);
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

    t.factory.set_pending_admin(&t.admin, &admin2);
    t.factory.accept_admin(&admin2);
    assert_eq!(t.factory.get_admin(), admin2);

    t.factory.set_pending_admin(&admin2, &admin3);
    t.factory.accept_admin(&admin3);
    assert_eq!(t.factory.get_admin(), admin3);

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

// ---------------------------------------------------------------------------
// AuthorizedAssets management
// ---------------------------------------------------------------------------

#[test]
fn test_add_authorized_asset() {
    let t = setup();
    let asset = Address::generate(&t.env);
    assert!(!t.factory.is_authorized_asset(&asset));

    t.factory.add_authorized_asset(&t.admin, &asset);

    assert!(t.factory.is_authorized_asset(&asset));
    let list = t.factory.get_authorized_assets();
    assert_eq!(list.len(), 1);
    assert_eq!(list.get(0).unwrap(), asset);
}

#[test]
fn test_add_authorized_asset_checks_asset_handler_registration() {
    let (t, asset_handler) = setup_with_asset_handler();
    let asset = Address::generate(&t.env);
    MockAssetHandlerClient::new(&t.env, &asset_handler).set_registered(&asset, &true);

    t.factory.add_authorized_asset(&t.admin, &asset);
    assert!(t.factory.is_authorized_asset(&asset));
}

#[test]
#[should_panic(expected = "Error(Contract, #14)")]
fn test_add_authorized_asset_rejects_unregistered_asset_handler_asset() {
    let (t, _asset_handler) = setup_with_asset_handler();
    let asset = Address::generate(&t.env);
    t.factory.add_authorized_asset(&t.admin, &asset);
}

#[test]
fn test_add_multiple_authorized_assets() {
    let t = setup();
    let a1 = Address::generate(&t.env);
    let a2 = Address::generate(&t.env);
    let a3 = Address::generate(&t.env);
    t.factory.add_authorized_asset(&t.admin, &a1);
    t.factory.add_authorized_asset(&t.admin, &a2);
    t.factory.add_authorized_asset(&t.admin, &a3);

    let list = t.factory.get_authorized_assets();
    assert_eq!(list.len(), 3);
    assert!(t.factory.is_authorized_asset(&a1));
    assert!(t.factory.is_authorized_asset(&a2));
    assert!(t.factory.is_authorized_asset(&a3));
}

#[test]
#[should_panic]
fn test_add_authorized_asset_not_admin_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    let asset = Address::generate(&t.env);
    t.factory.add_authorized_asset(&rogue, &asset);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_add_authorized_asset_duplicate_panics() {
    let t = setup();
    let asset = Address::generate(&t.env);
    t.factory.add_authorized_asset(&t.admin, &asset);
    t.factory.add_authorized_asset(&t.admin, &asset);
}

#[test]
fn test_remove_authorized_asset() {
    let t = setup();
    let a1 = Address::generate(&t.env);
    let a2 = Address::generate(&t.env);
    t.factory.add_authorized_asset(&t.admin, &a1);
    t.factory.add_authorized_asset(&t.admin, &a2);

    t.factory.remove_authorized_asset(&t.admin, &a1);

    assert!(!t.factory.is_authorized_asset(&a1));
    assert!(t.factory.is_authorized_asset(&a2));
    let list = t.factory.get_authorized_assets();
    assert_eq!(list.len(), 1);
}

#[test]
#[should_panic]
fn test_remove_authorized_asset_not_admin_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    let asset = Address::generate(&t.env);
    t.factory.add_authorized_asset(&t.admin, &asset);
    t.factory.remove_authorized_asset(&rogue, &asset);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_remove_unauthorized_asset_panics() {
    let t = setup();
    let asset = Address::generate(&t.env);
    t.factory.remove_authorized_asset(&t.admin, &asset);
}

#[test]
fn test_get_authorized_assets_empty_initially() {
    let t = setup();
    let list = t.factory.get_authorized_assets();
    assert_eq!(list.len(), 0);
}

// ---------------------------------------------------------------------------
// AuthorizedGuards management
// ---------------------------------------------------------------------------

#[test]
fn test_add_authorized_guard() {
    let t = setup();
    let guard = Address::generate(&t.env);
    assert!(!t.factory.is_authorized_guard(&guard));

    t.factory.add_authorized_guard(&t.admin, &guard);

    assert!(t.factory.is_authorized_guard(&guard));
    let list = t.factory.get_authorized_guards();
    assert_eq!(list.len(), 1);
    assert_eq!(list.get(0).unwrap(), guard);
}

#[test]
fn test_add_multiple_authorized_guards() {
    let t = setup();
    let g1 = Address::generate(&t.env);
    let g2 = Address::generate(&t.env);
    t.factory.add_authorized_guard(&t.admin, &g1);
    t.factory.add_authorized_guard(&t.admin, &g2);

    let list = t.factory.get_authorized_guards();
    assert_eq!(list.len(), 2);
    assert!(t.factory.is_authorized_guard(&g1));
    assert!(t.factory.is_authorized_guard(&g2));
}

#[test]
#[should_panic]
fn test_add_authorized_guard_not_admin_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    let guard = Address::generate(&t.env);
    t.factory.add_authorized_guard(&rogue, &guard);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_add_authorized_guard_duplicate_panics() {
    let t = setup();
    let guard = Address::generate(&t.env);
    t.factory.add_authorized_guard(&t.admin, &guard);
    t.factory.add_authorized_guard(&t.admin, &guard);
}

#[test]
fn test_remove_authorized_guard() {
    let t = setup();
    let g1 = Address::generate(&t.env);
    let g2 = Address::generate(&t.env);
    t.factory.add_authorized_guard(&t.admin, &g1);
    t.factory.add_authorized_guard(&t.admin, &g2);

    t.factory.remove_authorized_guard(&t.admin, &g1);

    assert!(!t.factory.is_authorized_guard(&g1));
    assert!(t.factory.is_authorized_guard(&g2));
    let list = t.factory.get_authorized_guards();
    assert_eq!(list.len(), 1);
}

#[test]
#[should_panic]
fn test_remove_authorized_guard_not_admin_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    let guard = Address::generate(&t.env);
    t.factory.add_authorized_guard(&t.admin, &guard);
    t.factory.remove_authorized_guard(&rogue, &guard);
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_remove_unauthorized_guard_panics() {
    let t = setup();
    let guard = Address::generate(&t.env);
    t.factory.remove_authorized_guard(&t.admin, &guard);
}

#[test]
fn test_get_authorized_guards_empty_initially() {
    let t = setup();
    let list = t.factory.get_authorized_guards();
    assert_eq!(list.len(), 0);
}

// ---------------------------------------------------------------------------
// set_vault_manager
// ---------------------------------------------------------------------------

#[test]
fn test_set_vault_manager_updates_factory_and_vault() {
    let t = setup();
    let manager = Address::generate(&t.env);
    let new_manager = Address::generate(&t.env);
    let vault = deploy_mock_vault(&t.env, &manager);
    t.factory.register_vault(&t.admin, &vault, &manager);

    t.factory.set_vault_manager(&t.admin, &vault, &new_manager);

    assert_eq!(t.factory.get_vault_manager(&vault), new_manager);
    // The vault's own manager should have been updated via cross-contract call.
    assert_eq!(
        MockVaultClient::new(&t.env, &vault).get_manager(),
        new_manager
    );
}

#[test]
#[should_panic]
fn test_set_vault_manager_not_admin_panics() {
    let t = setup();
    let manager = Address::generate(&t.env);
    let vault = deploy_mock_vault(&t.env, &manager);
    t.factory.register_vault(&t.admin, &vault, &manager);
    let rogue = Address::generate(&t.env);
    let new_manager = Address::generate(&t.env);
    t.factory.set_vault_manager(&rogue, &vault, &new_manager);
}

#[test]
#[should_panic]
fn test_set_vault_manager_unregistered_vault_panics() {
    let t = setup();
    let vault = Address::generate(&t.env);
    let new_manager = Address::generate(&t.env);
    t.factory.set_vault_manager(&t.admin, &vault, &new_manager);
}

#[test]
#[should_panic]
fn test_get_vault_manager_unregistered_panics() {
    let t = setup();
    let vault = Address::generate(&t.env);
    t.factory.get_vault_manager(&vault);
}

// ---------------------------------------------------------------------------
// create_vault (seed deposit)
// ---------------------------------------------------------------------------

fn deploy_full_mock_vault(env: &Env, manager: &Address) -> Address {
    let vid = env.register(MockVault, ());
    MockVaultClient::new(env, &vid).initialize(manager);
    vid
}

fn deploy_mock_token(env: &Env) -> Address {
    env.register(MockToken, ())
}

#[test]
fn test_create_vault_registers_and_seeds() {
    let t = setup();
    let manager = Address::generate(&t.env);
    let vault = deploy_full_mock_vault(&t.env, &manager);
    let token = deploy_mock_token(&t.env);

    // Mint tokens to admin so it can fund the seed deposit.
    MockTokenClient::new(&t.env, &token).mint(&t.admin, &1_000i128);

    t.factory
        .create_vault(&t.admin, &vault, &manager, &token, &100i128);

    assert!(t.factory.is_registered(&vault));
    assert_eq!(t.factory.get_vault_count(), 1);
    assert_eq!(t.factory.get_vault_manager(&vault), manager);
}

#[test]
#[should_panic(expected = "Error(Contract, #13)")]
fn test_create_vault_zero_seed_panics() {
    let t = setup();
    let manager = Address::generate(&t.env);
    let vault = deploy_full_mock_vault(&t.env, &manager);
    let token = deploy_mock_token(&t.env);

    t.factory
        .create_vault(&t.admin, &vault, &manager, &token, &0i128);
}

#[test]
#[should_panic]
fn test_create_vault_not_admin_panics() {
    let t = setup();
    let rogue = Address::generate(&t.env);
    let manager = Address::generate(&t.env);
    let vault = deploy_full_mock_vault(&t.env, &manager);
    let token = deploy_mock_token(&t.env);

    t.factory
        .create_vault(&rogue, &vault, &manager, &token, &100i128);
}

#[test]
#[should_panic]
fn test_create_vault_manager_mismatch_panics() {
    let t = setup();
    let manager = Address::generate(&t.env);
    let wrong_manager = Address::generate(&t.env);
    let vault = deploy_full_mock_vault(&t.env, &manager);
    let token = deploy_mock_token(&t.env);
    MockTokenClient::new(&t.env, &token).mint(&t.admin, &1_000i128);

    t.factory
        .create_vault(&t.admin, &vault, &wrong_manager, &token, &100i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_create_vault_already_registered_panics() {
    let t = setup();
    let manager = Address::generate(&t.env);
    let vault = deploy_full_mock_vault(&t.env, &manager);
    let token = deploy_mock_token(&t.env);
    MockTokenClient::new(&t.env, &token).mint(&t.admin, &1_000i128);

    t.factory
        .create_vault(&t.admin, &vault, &manager, &token, &100i128);
    // Second call on same vault should fail with VaultAlreadyRegistered.
    t.factory
        .create_vault(&t.admin, &vault, &manager, &token, &100i128);
}
