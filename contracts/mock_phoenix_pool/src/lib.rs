//! Testnet-only Phoenix-compatible pool mock.

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, token, Address, Env,
    IntoVal, Symbol,
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    ShareToken,
    TokenA,
    TokenB,
    ReserveA,
    ReserveB,
    LastDepositor,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum MockPhoenixError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidAmount = 3,
    Slippage = 4,
}

#[contract]
pub struct MockPhoenixPool;

fn require_initialized(env: &Env) {
    if !env.storage().instance().has(&DataKey::ShareToken) {
        panic_with_error!(env, MockPhoenixError::NotInitialized);
    }
}

fn mint_asset(env: &Env, token_id: &Address, to: &Address, amount: i128) {
    env.invoke_contract::<()>(
        token_id,
        &Symbol::new(env, "mint"),
        (to.clone(), amount).into_val(env),
    );
}

fn token_total_supply(env: &Env, token_id: &Address) -> i128 {
    env.invoke_contract::<i128>(
        token_id,
        &Symbol::new(env, "total_supply"),
        ().into_val(env),
    )
}

#[contractimpl]
impl MockPhoenixPool {
    pub fn __constructor(env: Env, share_token: Address, token_a: Address, token_b: Address) {
        if env.storage().instance().has(&DataKey::ShareToken) {
            panic_with_error!(&env, MockPhoenixError::AlreadyInitialized);
        }
        env.storage()
            .instance()
            .set(&DataKey::ShareToken, &share_token);
        env.storage().instance().set(&DataKey::TokenA, &token_a);
        env.storage().instance().set(&DataKey::TokenB, &token_b);
        env.storage().instance().set(&DataKey::ReserveA, &0_i128);
        env.storage().instance().set(&DataKey::ReserveB, &0_i128);
    }

    pub fn query_share_token_address(env: Env) -> Address {
        require_initialized(&env);
        env.storage().instance().get(&DataKey::ShareToken).unwrap()
    }

    pub fn query_pool_assets(env: Env) -> (Address, Address) {
        require_initialized(&env);
        let token_a: Address = env.storage().instance().get(&DataKey::TokenA).unwrap();
        let token_b: Address = env.storage().instance().get(&DataKey::TokenB).unwrap();
        (token_a, token_b)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn provide_liquidity(
        env: Env,
        depositor: Address,
        desired_a: Option<i128>,
        min_a: Option<i128>,
        desired_b: Option<i128>,
        min_b: Option<i128>,
        slippage_bps: Option<i64>,
        deadline: Option<u64>,
        auto_stake: bool,
    ) {
        require_initialized(&env);
        let _ = (slippage_bps, deadline, auto_stake);
        depositor.require_auth();
        let amount_a = desired_a.unwrap_or(0);
        let amount_b = desired_b.unwrap_or(0);
        if amount_a <= 0 || amount_b <= 0 {
            panic_with_error!(&env, MockPhoenixError::InvalidAmount);
        }
        if amount_a < min_a.unwrap_or(0) || amount_b < min_b.unwrap_or(0) {
            panic_with_error!(&env, MockPhoenixError::Slippage);
        }

        let pool = env.current_contract_address();
        let token_a: Address = env.storage().instance().get(&DataKey::TokenA).unwrap();
        let token_b: Address = env.storage().instance().get(&DataKey::TokenB).unwrap();
        token::Client::new(&env, &token_a).transfer_from(&pool, &depositor, &pool, &amount_a);
        token::Client::new(&env, &token_b).transfer_from(&pool, &depositor, &pool, &amount_b);

        let shares = amount_a.min(amount_b);
        let share_token: Address = env.storage().instance().get(&DataKey::ShareToken).unwrap();
        mint_asset(&env, &share_token, &depositor, shares);
        env.storage()
            .instance()
            .set(&DataKey::LastDepositor, &depositor);

        let reserve_a: i128 = env
            .storage()
            .instance()
            .get(&DataKey::ReserveA)
            .unwrap_or(0);
        let reserve_b: i128 = env
            .storage()
            .instance()
            .get(&DataKey::ReserveB)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::ReserveA, &(reserve_a + amount_a));
        env.storage()
            .instance()
            .set(&DataKey::ReserveB, &(reserve_b + amount_b));
    }

    pub fn withdraw_liquidity(
        env: Env,
        recipient: Address,
        share_amount: i128,
        min_a: i128,
        min_b: i128,
        deadline: Option<u64>,
    ) -> (i128, i128) {
        require_initialized(&env);
        let _ = deadline;
        if share_amount <= 0 {
            panic_with_error!(&env, MockPhoenixError::InvalidAmount);
        }
        let reserve_a: i128 = env
            .storage()
            .instance()
            .get(&DataKey::ReserveA)
            .unwrap_or(0);
        let reserve_b: i128 = env
            .storage()
            .instance()
            .get(&DataKey::ReserveB)
            .unwrap_or(0);
        let share_token: Address = env.storage().instance().get(&DataKey::ShareToken).unwrap();
        let total_shares = token_total_supply(&env, &share_token);
        let amount_a = reserve_a * share_amount / total_shares;
        let amount_b = reserve_b * share_amount / total_shares;
        if amount_a < min_a || amount_b < min_b {
            panic_with_error!(&env, MockPhoenixError::Slippage);
        }

        let pool = env.current_contract_address();
        let depositor: Address = env
            .storage()
            .instance()
            .get(&DataKey::LastDepositor)
            .unwrap();
        token::Client::new(&env, &share_token).transfer_from(
            &pool,
            &depositor,
            &pool,
            &share_amount,
        );
        token::Client::new(&env, &share_token).burn(&pool, &share_amount);

        let token_a: Address = env.storage().instance().get(&DataKey::TokenA).unwrap();
        let token_b: Address = env.storage().instance().get(&DataKey::TokenB).unwrap();
        token::Client::new(&env, &token_a).transfer(&pool, &recipient, &amount_a);
        token::Client::new(&env, &token_b).transfer(&pool, &recipient, &amount_b);
        env.storage()
            .instance()
            .set(&DataKey::ReserveA, &(reserve_a - amount_a));
        env.storage()
            .instance()
            .set(&DataKey::ReserveB, &(reserve_b - amount_b));
        (amount_a, amount_b)
    }

    pub fn get_reserves(env: Env) -> (i128, i128) {
        require_initialized(&env);
        (
            env.storage()
                .instance()
                .get(&DataKey::ReserveA)
                .unwrap_or(0),
            env.storage()
                .instance()
                .get(&DataKey::ReserveB)
                .unwrap_or(0),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn swap(
        env: Env,
        sender: Address,
        recipient: Address,
        sell_a: bool,
        offer_amount: i128,
        min_ask: i128,
        max_spread_bps: Option<i64>,
        deadline: Option<u64>,
    ) {
        require_initialized(&env);
        let _ = (max_spread_bps, deadline);
        sender.require_auth();
        if offer_amount <= 0 || offer_amount < min_ask {
            panic_with_error!(&env, MockPhoenixError::InvalidAmount);
        }
        let token_a: Address = env.storage().instance().get(&DataKey::TokenA).unwrap();
        let token_b: Address = env.storage().instance().get(&DataKey::TokenB).unwrap();
        let (offer_token, ask_token) = if sell_a {
            (token_a, token_b)
        } else {
            (token_b, token_a)
        };
        let pool = env.current_contract_address();
        token::Client::new(&env, &offer_token).transfer_from(&pool, &sender, &pool, &offer_amount);
        mint_asset(&env, &ask_token, &recipient, offer_amount);
    }
}
