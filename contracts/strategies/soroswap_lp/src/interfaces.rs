//! External contract adapters for Soroswap LP strategy.
//!
//! This module isolates all cross-contract ABI calls used by the strategy.
//! If Soroswap router/pair interfaces evolve, changes are localized here.

use soroban_sdk::{Address, Env, IntoVal, Symbol};

/// Adapter for Soroswap router calls.
pub struct RouterAdapter<'a> {
    env: &'a Env,
    router: &'a Address,
}

impl<'a> RouterAdapter<'a> {
    pub fn new(env: &'a Env, router: &'a Address) -> Self {
        Self { env, router }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_liquidity(
        &self,
        token_a: Address,
        token_b: Address,
        amount_a_desired: i128,
        amount_b_desired: i128,
        amount_a_min: i128,
        amount_b_min: i128,
        to: Address,
        deadline: u64,
    ) -> (i128, i128, i128) {
        let args = (
            token_a,
            token_b,
            amount_a_desired,
            amount_b_desired,
            amount_a_min,
            amount_b_min,
            to,
            deadline,
        )
            .into_val(self.env);
        self.env
            .invoke_contract(self.router, &Symbol::new(self.env, "add_liquidity"), args)
    }

    /// Swap an exact amount of `from_asset` for as much `to_asset` as possible.
    ///
    /// `path` is `[from_asset, to_asset]`.  The router sends output tokens
    /// directly to `to`.  Returns the output amounts for each hop.
    #[allow(clippy::too_many_arguments)]
    pub fn swap_exact_tokens_for_tokens(
        &self,
        amount_in: i128,
        amount_out_min: i128,
        path: soroban_sdk::Vec<Address>,
        to: Address,
        deadline: u64,
    ) -> soroban_sdk::Vec<i128> {
        let args = (amount_in, amount_out_min, path, to, deadline).into_val(self.env);
        self.env.invoke_contract(
            self.router,
            &Symbol::new(self.env, "swap_exact_tokens_for_tokens"),
            args,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn remove_liquidity(
        &self,
        token_a: Address,
        token_b: Address,
        liquidity: i128,
        amount_a_min: i128,
        amount_b_min: i128,
        to: Address,
        deadline: u64,
    ) -> (i128, i128) {
        let args = (
            token_a,
            token_b,
            liquidity,
            amount_a_min,
            amount_b_min,
            to,
            deadline,
        )
            .into_val(self.env);
        self.env.invoke_contract(
            self.router,
            &Symbol::new(self.env, "remove_liquidity"),
            args,
        )
    }
}

/// Adapter for Soroswap pair (LP token) calls.
pub struct PairAdapter<'a> {
    env: &'a Env,
    pair: &'a Address,
}

impl<'a> PairAdapter<'a> {
    pub fn new(env: &'a Env, pair: &'a Address) -> Self {
        Self { env, pair }
    }

    pub fn get_reserves(&self) -> (i128, i128) {
        let args = ().into_val(self.env);
        self.env
            .invoke_contract(self.pair, &Symbol::new(self.env, "get_reserves"), args)
    }

    pub fn total_supply(&self) -> i128 {
        let args = ().into_val(self.env);
        self.env
            .invoke_contract(self.pair, &Symbol::new(self.env, "total_supply"), args)
    }

    /// Return the pair's internal token0 (the token with the smaller address).
    ///
    /// Soroswap pairs order tokens by address internally; `get_reserves()` returns
    /// `(reserve_token0, reserve_token1)`.  Calling `token0()` lets the strategy
    /// map reserves to the correct asset regardless of the pair's internal order.
    pub fn token0(&self) -> Address {
        let args = ().into_val(self.env);
        self.env
            .invoke_contract(self.pair, &Symbol::new(self.env, "token0"), args)
    }

    /// Return the pair's internal token1 (the token with the larger address).
    #[allow(dead_code)]
    pub fn token1(&self) -> Address {
        let args = ().into_val(self.env);
        self.env
            .invoke_contract(self.pair, &Symbol::new(self.env, "token1"), args)
    }
}

/// Adapter for oracle calls (used in tests).
#[cfg(test)]
pub struct OracleAdapter<'a> {
    env: &'a Env,
    oracle: &'a Address,
}

#[cfg(test)]
impl<'a> OracleAdapter<'a> {
    pub fn new(env: &'a Env, oracle: &'a Address) -> Self {
        Self { env, oracle }
    }

    pub fn get_price(&self, asset: &Address) -> i128 {
        let args = (asset.clone(),).into_val(self.env);
        self.env
            .invoke_contract(self.oracle, &Symbol::new(self.env, "get_price"), args)
    }
}
