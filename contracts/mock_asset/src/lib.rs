//! Public-mint SEP-41-compatible mock asset for Stellar testnet deployments.
//!
//! This contract is testnet-only. It deliberately lets anyone call `mint` so
//! integrators can fund demo users without an issuer workflow.

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, Address, Env, String,
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Name,
    Symbol,
    Decimals,
    TotalSupply,
    Balance(Address),
    Allowance(Address, Address),
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum MockAssetError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    NegativeAmount = 3,
    ZeroAmount = 4,
    InsufficientBalance = 5,
    InsufficientAllowance = 6,
    Overflow = 7,
}

#[contract]
pub struct MockAsset;

fn require_initialized(env: &Env) {
    if !env.storage().instance().has(&DataKey::Name) {
        panic_with_error!(env, MockAssetError::NotInitialized);
    }
}

fn require_positive(env: &Env, amount: i128) {
    if amount < 0 {
        panic_with_error!(env, MockAssetError::NegativeAmount);
    }
    if amount == 0 {
        panic_with_error!(env, MockAssetError::ZeroAmount);
    }
}

fn balance_of(env: &Env, id: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Balance(id.clone()))
        .unwrap_or(0)
}

fn set_balance(env: &Env, id: &Address, amount: i128) {
    let key = DataKey::Balance(id.clone());
    if amount == 0 {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, &amount);
    }
}

fn allowance_of(env: &Env, from: &Address, spender: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Allowance(from.clone(), spender.clone()))
        .unwrap_or(0)
}

fn set_allowance(env: &Env, from: &Address, spender: &Address, amount: i128) {
    let key = DataKey::Allowance(from.clone(), spender.clone());
    if amount == 0 {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, &amount);
    }
}

fn move_balance(env: &Env, from: &Address, to: &Address, amount: i128) {
    let from_balance = balance_of(env, from);
    if from_balance < amount {
        panic_with_error!(env, MockAssetError::InsufficientBalance);
    }
    if from == to {
        return;
    }

    let new_from = from_balance
        .checked_sub(amount)
        .unwrap_or_else(|| panic_with_error!(env, MockAssetError::Overflow));
    let new_to = balance_of(env, to)
        .checked_add(amount)
        .unwrap_or_else(|| panic_with_error!(env, MockAssetError::Overflow));
    set_balance(env, from, new_from);
    set_balance(env, to, new_to);
}

#[contractimpl]
impl MockAsset {
    pub fn __constructor(env: Env, name: String, symbol: String, decimals: u32) {
        if env.storage().instance().has(&DataKey::Name) {
            panic_with_error!(&env, MockAssetError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Name, &name);
        env.storage().instance().set(&DataKey::Symbol, &symbol);
        env.storage().instance().set(&DataKey::Decimals, &decimals);
        env.storage().instance().set(&DataKey::TotalSupply, &0_i128);
    }

    pub fn name(env: Env) -> String {
        require_initialized(&env);
        env.storage().instance().get(&DataKey::Name).unwrap()
    }

    pub fn symbol(env: Env) -> String {
        require_initialized(&env);
        env.storage().instance().get(&DataKey::Symbol).unwrap()
    }

    pub fn decimals(env: Env) -> u32 {
        require_initialized(&env);
        env.storage().instance().get(&DataKey::Decimals).unwrap()
    }

    pub fn total_supply(env: Env) -> i128 {
        require_initialized(&env);
        env.storage()
            .instance()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0)
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        require_initialized(&env);
        balance_of(&env, &id)
    }

    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        require_initialized(&env);
        allowance_of(&env, &from, &spender)
    }

    pub fn approve(
        env: Env,
        from: Address,
        spender: Address,
        amount: i128,
        expiration_ledger: u32,
    ) {
        require_initialized(&env);
        if amount < 0 {
            panic_with_error!(&env, MockAssetError::NegativeAmount);
        }
        let _ = expiration_ledger;
        from.require_auth();
        set_allowance(&env, &from, &spender, amount);
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        require_initialized(&env);
        require_positive(&env, amount);
        from.require_auth();
        move_balance(&env, &from, &to, amount);
    }

    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        require_initialized(&env);
        require_positive(&env, amount);
        spender.require_auth();
        let allowance = allowance_of(&env, &from, &spender);
        if allowance < amount {
            panic_with_error!(&env, MockAssetError::InsufficientAllowance);
        }
        set_allowance(&env, &from, &spender, allowance - amount);
        move_balance(&env, &from, &to, amount);
    }

    pub fn mint(env: Env, to: Address, amount: i128) {
        require_initialized(&env);
        require_positive(&env, amount);
        let new_balance = balance_of(&env, &to)
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, MockAssetError::Overflow));
        let supply = Self::total_supply(env.clone())
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, MockAssetError::Overflow));
        set_balance(&env, &to, new_balance);
        env.storage().instance().set(&DataKey::TotalSupply, &supply);
    }

    pub fn burn(env: Env, from: Address, amount: i128) {
        require_initialized(&env);
        require_positive(&env, amount);
        from.require_auth();
        let balance = balance_of(&env, &from);
        if balance < amount {
            panic_with_error!(&env, MockAssetError::InsufficientBalance);
        }
        set_balance(&env, &from, balance - amount);
        let supply = Self::total_supply(env.clone())
            .checked_sub(amount)
            .unwrap_or_else(|| panic_with_error!(&env, MockAssetError::Overflow));
        env.storage().instance().set(&DataKey::TotalSupply, &supply);
    }
}
