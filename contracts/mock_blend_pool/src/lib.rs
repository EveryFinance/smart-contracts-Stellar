#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, token, Address, Env,
    Vec,
};

const REQUEST_SUPPLY: u32 = 2;
const REQUEST_WITHDRAW: u32 = 3;

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
                token::Client::new(&env, &token_addr).transfer_from(&pool, &from, &pool, &req.amount);
                let bal: i128 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::Supply(from.clone()))
                    .unwrap_or(0);
                env.storage()
                    .persistent()
                    .set(&DataKey::Supply(from.clone()), &(bal + req.amount));
                let total: i128 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::TotalSupply)
                    .unwrap_or(0);
                env.storage()
                    .persistent()
                    .set(&DataKey::TotalSupply, &(total + req.amount));
            } else if req.request_type == REQUEST_WITHDRAW {
                let bal: i128 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::Supply(from.clone()))
                    .unwrap_or(0);
                let pool_bal = token::Client::new(&env, &token_addr).balance(&pool);
                if pool_bal < req.amount {
                    panic_with_error!(&env, PoolError::InsufficientSupply);
                }
                let total: i128 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::TotalSupply)
                    .unwrap_or(0);
                // Allow withdrawing accrued yield when pool balance exceeds tracked principal.
                // Principal accounting for this depositor is floored at zero.
                let principal_reduction = if req.amount > bal { bal } else { req.amount };
                env.storage()
                    .persistent()
                    .set(&DataKey::Supply(from.clone()), &(bal - principal_reduction));
                let new_total = if total >= principal_reduction {
                    total - principal_reduction
                } else {
                    0
                };
                env.storage()
                    .persistent()
                    .set(&DataKey::TotalSupply, &new_total);
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
        env.storage()
            .persistent()
            .get(&DataKey::Supply(account))
            .unwrap_or(0)
    }
}
