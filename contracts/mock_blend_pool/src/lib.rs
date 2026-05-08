#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, token, Address, Env, Vec,
};

const REQUEST_SUPPLY: u32 = 2;
const REQUEST_WITHDRAW: u32 = 3;

const PERSISTENT_BUMP_AMOUNT: u32 = 518_400;
const PERSISTENT_LIFETIME_THRESHOLD: u32 = 259_200;

#[contracttype]
#[derive(Clone)]
pub struct BlendRequest {
    pub request_type: u32,
    pub address: Address,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Token,
    Supply(Address),
    TotalSupply,
}

#[derive(Clone, Copy)]
#[repr(u32)]
#[contracterror]
pub enum PoolError {
    NotInitialized = 1,
    NotAdmin = 2,
    InvalidAmount = 3,
    InsufficientSupply = 4,
}

#[contract]
pub struct MockBlendPool;

#[contractimpl]
impl MockBlendPool {
    pub fn initialize(env: Env, admin: Address, token: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, PoolError::NotInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Token, &token);
    }

    pub fn submit(
        env: Env,
        from: Address,
        _spender: Address,
        to: Address,
        requests: Vec<BlendRequest>,
    ) {
        // The account whose positions are being operated on must authorise the
        // call.  In the strategy's cross-contract call chain `from` is always
        // the strategy contract itself (the direct invoker), so Soroban
        // satisfies this automatically.  This mirrors the real Blend pool's
        // auth model and prevents any third party from draining positions or
        // triggering supplies on behalf of an arbitrary `from`.
        from.require_auth();

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .unwrap_or_else(|| panic_with_error!(&env, PoolError::NotInitialized));
        let pool = env.current_contract_address();

        for req in requests.iter() {
            if req.amount <= 0 {
                panic_with_error!(&env, PoolError::InvalidAmount);
            }
            if req.request_type == REQUEST_SUPPLY {
                token::Client::new(&env, &token_addr).transfer_from(
                    &pool,
                    &from,
                    &pool,
                    &req.amount,
                );
                let bal: i128 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::Supply(from.clone()))
                    .unwrap_or(0);
                let supply_key = DataKey::Supply(from.clone());
                env.storage().persistent().set(&supply_key, &(bal + req.amount));
                env.storage().persistent().extend_ttl(
                    &supply_key,
                    PERSISTENT_LIFETIME_THRESHOLD,
                    PERSISTENT_BUMP_AMOUNT,
                );
                let total: i128 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::TotalSupply)
                    .unwrap_or(0);
                env.storage().persistent().set(&DataKey::TotalSupply, &(total + req.amount));
                env.storage().persistent().extend_ttl(
                    &DataKey::TotalSupply,
                    PERSISTENT_LIFETIME_THRESHOLD,
                    PERSISTENT_BUMP_AMOUNT,
                );
            } else if req.request_type == REQUEST_WITHDRAW {
                let bal: i128 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::Supply(from.clone()))
                    .unwrap_or(0);
                // Enforce strict invariant: cannot withdraw more than deposited principal.
                if req.amount > bal {
                    panic_with_error!(&env, PoolError::InsufficientSupply);
                }
                let total: i128 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::TotalSupply)
                    .unwrap_or(0);
                let new_bal = bal - req.amount;
                let supply_key = DataKey::Supply(from.clone());
                if new_bal == 0 {
                    env.storage().persistent().remove(&supply_key);
                } else {
                    env.storage().persistent().set(&supply_key, &new_bal);
                    env.storage().persistent().extend_ttl(
                        &supply_key,
                        PERSISTENT_LIFETIME_THRESHOLD,
                        PERSISTENT_BUMP_AMOUNT,
                    );
                }
                let new_total = total - req.amount;
                if new_total == 0 {
                    env.storage().persistent().remove(&DataKey::TotalSupply);
                } else {
                    env.storage().persistent().set(&DataKey::TotalSupply, &new_total);
                    env.storage().persistent().extend_ttl(
                        &DataKey::TotalSupply,
                        PERSISTENT_LIFETIME_THRESHOLD,
                        PERSISTENT_BUMP_AMOUNT,
                    );
                }
                token::Client::new(&env, &token_addr).transfer(&pool, &to, &req.amount);
            }
        }
    }

    pub fn add_yield(env: Env, caller: Address, amount: i128) {
        if amount <= 0 {
            panic_with_error!(&env, PoolError::InvalidAmount);
        }
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, PoolError::NotInitialized));
        if caller != admin {
            panic_with_error!(&env, PoolError::NotAdmin);
        }
        let token_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .unwrap_or_else(|| panic_with_error!(&env, PoolError::NotInitialized));
        let pool = env.current_contract_address();
        token::Client::new(&env, &token_addr).transfer(&caller, &pool, &amount);
    }

    pub fn get_supply(env: Env, account: Address) -> i128 {
        let key = DataKey::Supply(account);
        let val: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        if val > 0 {
            env.storage().persistent().extend_ttl(
                &key,
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
        }
        val
    }
}
