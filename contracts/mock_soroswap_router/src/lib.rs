//! Testnet-only Soroswap-compatible mock router and LP token.

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, token, Address, Env,
    IntoVal, Symbol, Vec,
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Token0,
    Token1,
    Price0,
    Price1,
    Reserve0,
    Reserve1,
    TotalSupply,
    Balance(Address),
    Allowance(Address, Address),
    LastLpHolder,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum MockDexError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidAmount = 3,
    InsufficientBalance = 4,
    InsufficientAllowance = 5,
    Slippage = 6,
    InvalidPath = 7,
    Overflow = 8,
}

#[contract]
pub struct MockSoroswapRouter;

fn require_initialized(env: &Env) {
    if !env.storage().instance().has(&DataKey::Token0) {
        panic_with_error!(env, MockDexError::NotInitialized);
    }
}

fn require_positive(env: &Env, amount: i128) {
    if amount <= 0 {
        panic_with_error!(env, MockDexError::InvalidAmount);
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

fn total_supply_of(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::TotalSupply)
        .unwrap_or(0)
}

fn mint_lp(env: &Env, to: &Address, amount: i128) {
    let balance = balance_of(env, to)
        .checked_add(amount)
        .unwrap_or_else(|| panic_with_error!(env, MockDexError::Overflow));
    let supply = total_supply_of(env)
        .checked_add(amount)
        .unwrap_or_else(|| panic_with_error!(env, MockDexError::Overflow));
    set_balance(env, to, balance);
    env.storage().instance().set(&DataKey::TotalSupply, &supply);
}

fn burn_lp(env: &Env, from: &Address, amount: i128) {
    let balance = balance_of(env, from);
    if balance < amount {
        panic_with_error!(env, MockDexError::InsufficientBalance);
    }
    set_balance(env, from, balance - amount);
    let supply = total_supply_of(env)
        .checked_sub(amount)
        .unwrap_or_else(|| panic_with_error!(env, MockDexError::Overflow));
    env.storage().instance().set(&DataKey::TotalSupply, &supply);
}

fn mint_asset(env: &Env, token_id: &Address, to: &Address, amount: i128) {
    env.invoke_contract::<()>(
        token_id,
        &Symbol::new(env, "mint"),
        (to.clone(), amount).into_val(env),
    );
}

#[contractimpl]
impl MockSoroswapRouter {
    pub fn __constructor(env: Env, token0: Address, token1: Address, price0: i128, price1: i128) {
        if env.storage().instance().has(&DataKey::Token0) {
            panic_with_error!(&env, MockDexError::AlreadyInitialized);
        }
        require_positive(&env, price0);
        require_positive(&env, price1);
        env.storage().instance().set(&DataKey::Token0, &token0);
        env.storage().instance().set(&DataKey::Token1, &token1);
        env.storage().instance().set(&DataKey::Price0, &price0);
        env.storage().instance().set(&DataKey::Price1, &price1);
        env.storage().instance().set(&DataKey::Reserve0, &0_i128);
        env.storage().instance().set(&DataKey::Reserve1, &0_i128);
        env.storage().instance().set(&DataKey::TotalSupply, &0_i128);
    }

    pub fn token0(env: Env) -> Address {
        require_initialized(&env);
        env.storage().instance().get(&DataKey::Token0).unwrap()
    }

    pub fn token1(env: Env) -> Address {
        require_initialized(&env);
        env.storage().instance().get(&DataKey::Token1).unwrap()
    }

    pub fn get_reserves(env: Env) -> (i128, i128) {
        require_initialized(&env);
        (
            env.storage()
                .instance()
                .get(&DataKey::Reserve0)
                .unwrap_or(0),
            env.storage()
                .instance()
                .get(&DataKey::Reserve1)
                .unwrap_or(0),
        )
    }

    pub fn name(env: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&env, "Mock Soroswap LP")
    }

    pub fn symbol(env: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&env, "mSLP")
    }

    pub fn decimals(_env: Env) -> u32 {
        7
    }

    pub fn total_supply(env: Env) -> i128 {
        require_initialized(&env);
        total_supply_of(&env)
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
            panic_with_error!(&env, MockDexError::InvalidAmount);
        }
        let _ = expiration_ledger;
        from.require_auth();
        set_allowance(&env, &from, &spender, amount);
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        require_initialized(&env);
        require_positive(&env, amount);
        from.require_auth();
        let from_balance = balance_of(&env, &from);
        if from_balance < amount {
            panic_with_error!(&env, MockDexError::InsufficientBalance);
        }
        set_balance(&env, &from, from_balance - amount);
        set_balance(&env, &to, balance_of(&env, &to) + amount);
    }

    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        require_initialized(&env);
        require_positive(&env, amount);
        spender.require_auth();
        let allowance = allowance_of(&env, &from, &spender);
        if allowance < amount {
            panic_with_error!(&env, MockDexError::InsufficientAllowance);
        }
        set_allowance(&env, &from, &spender, allowance - amount);
        let from_balance = balance_of(&env, &from);
        if from_balance < amount {
            panic_with_error!(&env, MockDexError::InsufficientBalance);
        }
        set_balance(&env, &from, from_balance - amount);
        set_balance(&env, &to, balance_of(&env, &to) + amount);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_liquidity(
        env: Env,
        token_a: Address,
        token_b: Address,
        amount_a_desired: i128,
        amount_b_desired: i128,
        amount_a_min: i128,
        amount_b_min: i128,
        to: Address,
        deadline: u64,
    ) -> (i128, i128, i128) {
        require_initialized(&env);
        let _ = (deadline, token_a, token_b);
        require_positive(&env, amount_a_desired);
        require_positive(&env, amount_b_desired);
        if amount_a_desired < amount_a_min || amount_b_desired < amount_b_min {
            panic_with_error!(&env, MockDexError::Slippage);
        }

        let router = env.current_contract_address();
        let token0 = Self::token0(env.clone());
        let token1 = Self::token1(env.clone());
        token::Client::new(&env, &token0).transfer_from(&router, &to, &router, &amount_a_desired);
        token::Client::new(&env, &token1).transfer_from(&router, &to, &router, &amount_b_desired);

        let lp_minted = amount_a_desired.min(amount_b_desired);
        mint_lp(&env, &to, lp_minted);
        env.storage().instance().set(&DataKey::LastLpHolder, &to);
        let reserve0: i128 = env
            .storage()
            .instance()
            .get(&DataKey::Reserve0)
            .unwrap_or(0);
        let reserve1: i128 = env
            .storage()
            .instance()
            .get(&DataKey::Reserve1)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::Reserve0, &(reserve0 + amount_a_desired));
        env.storage()
            .instance()
            .set(&DataKey::Reserve1, &(reserve1 + amount_b_desired));
        (amount_a_desired, amount_b_desired, lp_minted)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn remove_liquidity(
        env: Env,
        token_a: Address,
        token_b: Address,
        liquidity: i128,
        amount_a_min: i128,
        amount_b_min: i128,
        to: Address,
        deadline: u64,
    ) -> (i128, i128) {
        require_initialized(&env);
        let _ = deadline;
        require_positive(&env, liquidity);
        let supply = total_supply_of(&env);
        if supply <= 0 || liquidity > supply {
            panic_with_error!(&env, MockDexError::InsufficientBalance);
        }

        let reserve0: i128 = env
            .storage()
            .instance()
            .get(&DataKey::Reserve0)
            .unwrap_or(0);
        let reserve1: i128 = env
            .storage()
            .instance()
            .get(&DataKey::Reserve1)
            .unwrap_or(0);
        let amount_a = reserve0 * liquidity / supply;
        let amount_b = reserve1 * liquidity / supply;
        if amount_a < amount_a_min || amount_b < amount_b_min {
            panic_with_error!(&env, MockDexError::Slippage);
        }

        let holder: Address = env
            .storage()
            .instance()
            .get(&DataKey::LastLpHolder)
            .unwrap();
        burn_lp(&env, &holder, liquidity);
        env.storage()
            .instance()
            .set(&DataKey::Reserve0, &(reserve0 - amount_a));
        env.storage()
            .instance()
            .set(&DataKey::Reserve1, &(reserve1 - amount_b));
        let router = env.current_contract_address();
        token::Client::new(&env, &token_a).transfer(&router, &to, &amount_a);
        token::Client::new(&env, &token_b).transfer(&router, &to, &amount_b);
        (amount_a, amount_b)
    }

    pub fn swap_exact_tokens_for_tokens(
        env: Env,
        amount_in: i128,
        amount_out_min: i128,
        path: Vec<Address>,
        to: Address,
        deadline: u64,
    ) -> Vec<i128> {
        require_initialized(&env);
        let _ = deadline;
        require_positive(&env, amount_in);
        if path.len() < 2 {
            panic_with_error!(&env, MockDexError::InvalidPath);
        }
        let in_token = path.get(0).unwrap();
        let out_token = path.get(path.len() - 1).unwrap();
        let token0 = Self::token0(env.clone());
        let token1 = Self::token1(env.clone());
        let price0: i128 = env.storage().instance().get(&DataKey::Price0).unwrap();
        let price1: i128 = env.storage().instance().get(&DataKey::Price1).unwrap();
        let (price_in, price_out) = if in_token == token0 && out_token == token1 {
            (price0, price1)
        } else if in_token == token1 && out_token == token0 {
            (price1, price0)
        } else {
            panic_with_error!(&env, MockDexError::InvalidPath);
        };
        let amount_out = amount_in
            .checked_mul(price_in)
            .map(|v| v / price_out)
            .unwrap_or_else(|| panic_with_error!(&env, MockDexError::Overflow));
        if amount_out < amount_out_min {
            panic_with_error!(&env, MockDexError::Slippage);
        }
        mint_asset(&env, &out_token, &to, amount_out);
        soroban_sdk::vec![&env, amount_in, amount_out]
    }
}
