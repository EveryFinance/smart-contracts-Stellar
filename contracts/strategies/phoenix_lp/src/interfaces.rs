//! External contract adapters for Phoenix LP strategy.
//!
//! This module centralizes all cross-contract ABI calls used by the strategy.
//! If the Phoenix pool or oracle signatures change, updates stay isolated here.

use soroban_sdk::{Address, Env, IntoVal, Symbol};

/// Adapter for Phoenix pool contract calls.
pub struct PhoenixPoolAdapter<'a> {
    env: &'a Env,
    pool: &'a Address,
}

impl<'a> PhoenixPoolAdapter<'a> {
    pub fn new(env: &'a Env, pool: &'a Address) -> Self {
        Self { env, pool }
    }

    pub fn query_share_token_address(&self) -> Address {
        let args = ().into_val(self.env);
        self.env.invoke_contract(
            self.pool,
            &Symbol::new(self.env, "query_share_token_address"),
            args,
        )
    }

    /// Returns (token_a, token_b) for this pool — used to determine swap direction.
    pub fn query_pool_assets(&self) -> (Address, Address) {
        let args = ().into_val(self.env);
        self.env.invoke_contract(
            self.pool,
            &Symbol::new(self.env, "query_pool_assets"),
            args,
        )
    }

    pub fn get_reserves(&self) -> (i128, i128) {
        let args = ().into_val(self.env);
        self.env
            .invoke_contract(self.pool, &Symbol::new(self.env, "get_reserves"), args)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn provide_liquidity(
        &self,
        depositor: Address,
        desired_a: Option<i128>,
        min_a: Option<i128>,
        desired_b: Option<i128>,
        min_b: Option<i128>,
        custom_slippage_bps: Option<i64>,
        deadline: Option<u64>,
    ) {
        let args = (
            depositor,
            desired_a,
            min_a,
            desired_b,
            min_b,
            custom_slippage_bps,
            deadline,
            false,
        )
            .into_val(self.env);
        self.env.invoke_contract::<()>(
            self.pool,
            &Symbol::new(self.env, "provide_liquidity"),
            args,
        );
    }

    pub fn withdraw_liquidity(
        &self,
        recipient: Address,
        share_amount: i128,
        min_a: i128,
        min_b: i128,
        deadline: Option<u64>,
    ) -> (i128, i128) {
        let args = (recipient, share_amount, min_a, min_b, deadline).into_val(self.env);
        self.env.invoke_contract(
            self.pool,
            &Symbol::new(self.env, "withdraw_liquidity"),
            args,
        )
    }

    /// Swap `offer_amount` of one pool token for at least `min_ask` of the other.
    /// `sell_a = true` means selling asset A to receive asset B, and vice versa.
    /// Tokens are pulled from `sender`; output goes to `recipient`.
    pub fn swap(
        &self,
        sender: Address,
        recipient: Address,
        sell_a: bool,
        offer_amount: i128,
        min_ask: i128,
        max_spread_bps: Option<i64>,
        deadline: Option<u64>,
    ) {
        let args = (
            sender,
            recipient,
            sell_a,
            offer_amount,
            min_ask,
            max_spread_bps,
            deadline,
        )
            .into_val(self.env);
        self.env
            .invoke_contract::<()>(self.pool, &Symbol::new(self.env, "swap"), args);
    }
}

/// Adapter for oracle contract calls.
pub struct OracleAdapter<'a> {
    env: &'a Env,
    oracle: &'a Address,
}

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

/// Adapter for SEP-41 token calls used by LP share tokens.
pub struct Sep41TokenAdapter<'a> {
    env: &'a Env,
    token: &'a Address,
}

impl<'a> Sep41TokenAdapter<'a> {
    pub fn new(env: &'a Env, token: &'a Address) -> Self {
        Self { env, token }
    }

    pub fn total_supply(&self) -> i128 {
        let args = ().into_val(self.env);
        self.env
            .invoke_contract(self.token, &Symbol::new(self.env, "total_supply"), args)
    }
}
