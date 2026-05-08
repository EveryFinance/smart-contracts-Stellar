//! Shared helpers and mock contracts used across all integration tests.
//!
//! Rather than importing real external protocol contracts (Blend pool,
//! Soroswap router, Phoenix pool — which don't exist as local crates),
//! each integration module registers lightweight mock implementations
//! alongside the *real* protocol contracts (vault, share_token, strategies,
//! guards, factory).  This lets us verify the full cross-contract call chain
//! without needing live network contracts.

#![allow(dead_code)]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String, Vec};

// ---------------------------------------------------------------------------
// MockToken  (minimal SEP-41)
// ---------------------------------------------------------------------------

#[contracttype]
pub enum TKey {
    Balance(Address),
    Allowance(Address, Address),
    TotalSupply,
    Admin,
}

#[contract]
pub struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn initialize(env: Env, admin: Address) {
        assert!(
            !env.storage().instance().has(&TKey::Admin),
            "already initialized"
        );
        env.storage().instance().set(&TKey::Admin, &admin);
    }
    pub fn mint(env: Env, to: Address, amount: i128) {
        assert!(amount >= 0, "mint: negative amount");
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
    pub fn burn(env: Env, from: Address, amount: i128) {
        from.require_auth();
        assert!(amount >= 0, "burn: negative amount");
        let b: i128 = env
            .storage()
            .persistent()
            .get(&TKey::Balance(from.clone()))
            .unwrap_or(0);
        assert!(b >= amount, "burn > balance");
        env.storage()
            .persistent()
            .set(&TKey::Balance(from.clone()), &(b - amount));
        let s: i128 = env
            .storage()
            .persistent()
            .get(&TKey::TotalSupply)
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&TKey::TotalSupply, &(s - amount));
    }
    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&TKey::Balance(id))
            .unwrap_or(0)
    }
    pub fn total_supply(env: Env) -> i128 {
        env.storage()
            .persistent()
            .get(&TKey::TotalSupply)
            .unwrap_or(0)
    }
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        assert!(amount >= 0, "transfer: negative amount");
        let fb: i128 = env
            .storage()
            .persistent()
            .get(&TKey::Balance(from.clone()))
            .unwrap_or(0);
        assert!(fb >= amount, "transfer: insufficient balance");
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
    pub fn approve(env: Env, from: Address, spender: Address, amount: i128, _expiry: u32) {
        assert!(amount >= 0, "approve: negative amount");
        from.require_auth();
        env.storage()
            .persistent()
            .set(&TKey::Allowance(from, spender), &amount);
    }
    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&TKey::Allowance(from, spender))
            .unwrap_or(0)
    }
    pub fn transfer_from(env: Env, sp: Address, from: Address, to: Address, amount: i128) {
        assert!(amount >= 0, "transfer_from: negative amount");
        sp.require_auth();
        let allowance: i128 = env
            .storage()
            .persistent()
            .get(&TKey::Allowance(from.clone(), sp.clone()))
            .unwrap_or(0);
        assert!(allowance >= amount, "transfer_from: insufficient allowance");
        env.storage()
            .persistent()
            .set(&TKey::Allowance(from.clone(), sp), &(allowance - amount));
        let fb: i128 = env
            .storage()
            .persistent()
            .get(&TKey::Balance(from.clone()))
            .unwrap_or(0);
        assert!(fb >= amount, "transfer_from: insufficient balance");
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
    pub fn decimals(_env: Env) -> u32 {
        7
    }
    pub fn name(env: Env) -> String {
        String::from_str(&env, "Mock")
    }
    pub fn symbol(env: Env) -> String {
        String::from_str(&env, "MCK")
    }
}

// ---------------------------------------------------------------------------
// MockBlendPool
// ---------------------------------------------------------------------------

#[contracttype]
pub enum BlendKey {
    Supply(Address),
}

#[contract]
pub struct MockBlendPool;

/// Simplified Blend pool that tracks supply positions per account.
///
/// Supply (request_type 2): credits the strategy's position.
/// Withdraw (request_type 3): debits position and mints tokens to `to`.
#[contractimpl]
impl MockBlendPool {
    /// One-time initialization — stores the underlying token address.
    /// Panics if already initialized so the token cannot be swapped after setup.
    pub fn blend_init(env: Env, token: Address) {
        assert!(
            !env.storage().instance().has(&BlendPoolKey::Token),
            "blend_init: already initialized"
        );
        env.storage().instance().set(&BlendPoolKey::Token, &token);
    }

    pub fn submit(
        env: Env,
        from: Address,
        _spender: Address,
        to: Address,
        requests: Vec<blend_strategy::BlendRequest>,
    ) {
        from.require_auth();
        for req in requests.iter() {
            if req.request_type == 2 {
                // Supply — record position for `from`.
                let bal: i128 = env
                    .storage()
                    .persistent()
                    .get(&BlendKey::Supply(from.clone()))
                    .unwrap_or(0);
                env.storage()
                    .persistent()
                    .set(&BlendKey::Supply(from.clone()), &(bal + req.amount));
            } else if req.request_type == 3 {
                // Withdraw — reduce position, mint tokens to `to`.
                let bal: i128 = env
                    .storage()
                    .persistent()
                    .get(&BlendKey::Supply(from.clone()))
                    .unwrap_or(0);
                assert!(bal >= req.amount, "blend: insufficient position");
                env.storage()
                    .persistent()
                    .set(&BlendKey::Supply(from.clone()), &(bal - req.amount));
                // Mint underlying to `to` to simulate Blend returning funds.
                let token: Address = env.storage().instance().get(&BlendPoolKey::Token).unwrap();
                MockTokenClient::new(&env, &token).mint(&to, &req.amount);
            }
        }
    }

    /// Return the supply position for `account` (mirrors the real Blend pool interface).
    pub fn get_supply(env: Env, account: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&BlendKey::Supply(account))
            .unwrap_or(0)
    }
}

#[contracttype]
pub enum BlendPoolKey {
    Token,
}

// ---------------------------------------------------------------------------
// MockSoroswapRouter
// ---------------------------------------------------------------------------

#[contracttype]
pub enum RouterKey {
    LpToken,
}

#[contract]
pub struct MockSoroswapRouter;

#[contractimpl]
impl MockSoroswapRouter {
    pub fn router_init(env: Env, lp_token: Address) {
        env.storage().instance().set(&RouterKey::LpToken, &lp_token);
    }
    pub fn add_liquidity(
        env: Env,
        _token_a: Address,
        _token_b: Address,
        amount_a: i128,
        amount_b: i128,
        min_a: i128,
        min_b: i128,
        to: Address,
        _deadline: u64,
    ) -> (i128, i128, i128) {
        to.require_auth();
        assert!(amount_a >= min_a, "add_liquidity: amount_a below min");
        assert!(amount_b >= min_b, "add_liquidity: amount_b below min");
        let lp_minted = amount_a.min(amount_b);
        let lp: Address = env.storage().instance().get(&RouterKey::LpToken).unwrap();
        MockTokenClient::new(&env, &lp).mint(&to, &lp_minted);
        (amount_a, amount_b, lp_minted)
    }
    pub fn remove_liquidity(
        env: Env,
        token_a: Address,
        token_b: Address,
        liquidity: i128,
        min_a: i128,
        min_b: i128,
        to: Address,
        _deadline: u64,
    ) -> (i128, i128) {
        to.require_auth();
        let half = liquidity / 2;
        assert!(half >= min_a, "remove_liquidity: amount_a below min");
        assert!(half >= min_b, "remove_liquidity: amount_b below min");
        MockTokenClient::new(&env, &token_a).mint(&to, &half);
        MockTokenClient::new(&env, &token_b).mint(&to, &half);
        (half, half)
    }
}

// ---------------------------------------------------------------------------
// MockPhoenixPool
// ---------------------------------------------------------------------------

#[contracttype]
pub enum PhoenixKey {
    ShareToken,
    UnderlyingA,
    UnderlyingB,
    ReserveA,
    ReserveB,
}

#[contract]
pub struct MockPhoenixPool;

#[contractimpl]
impl MockPhoenixPool {
    pub fn phoenix_init(env: Env, share_token: Address, token_a: Address, token_b: Address) {
        env.storage()
            .instance()
            .set(&PhoenixKey::ShareToken, &share_token);
        env.storage()
            .instance()
            .set(&PhoenixKey::UnderlyingA, &token_a);
        env.storage()
            .instance()
            .set(&PhoenixKey::UnderlyingB, &token_b);
        env.storage().instance().set(&PhoenixKey::ReserveA, &0i128);
        env.storage().instance().set(&PhoenixKey::ReserveB, &0i128);
    }
    pub fn query_share_token_address(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&PhoenixKey::ShareToken)
            .unwrap()
    }
    #[allow(clippy::too_many_arguments)]
    pub fn provide_liquidity(
        env: Env,
        depositor: Address,
        desired_a: Option<i128>,
        min_a: Option<i128>,
        desired_b: Option<i128>,
        min_b: Option<i128>,
        _slippage_bps: Option<i64>,
        _deadline: Option<u64>,
        _auto_stake: bool,
    ) {
        depositor.require_auth();
        let amount_a = desired_a.unwrap_or(0);
        let amount_b = desired_b.unwrap_or(0);
        assert!(amount_a >= 0 && amount_b >= 0, "provide_liquidity: negative amount");
        if let Some(min) = min_a {
            assert!(amount_a >= min, "provide_liquidity: amount_a below min");
        }
        if let Some(min) = min_b {
            assert!(amount_b >= min, "provide_liquidity: amount_b below min");
        }
        let shares = amount_a.min(amount_b);
        let share_token: Address = env
            .storage()
            .instance()
            .get(&PhoenixKey::ShareToken)
            .unwrap();
        let token_a: Address = env
            .storage()
            .instance()
            .get(&PhoenixKey::UnderlyingA)
            .unwrap();
        let token_b_addr: Address = env
            .storage()
            .instance()
            .get(&PhoenixKey::UnderlyingB)
            .unwrap();
        let pool = env.current_contract_address();
        // Transfer tokens from depositor to pool (depositor pre-approved via token.approve).
        // This models the real Phoenix pool behavior and ensures residual-token logic
        // in the strategy is correctly exercised.
        if amount_a > 0 {
            MockTokenClient::new(&env, &token_a).transfer_from(&pool, &depositor, &pool, &amount_a);
        }
        if amount_b > 0 {
            MockTokenClient::new(&env, &token_b_addr)
                .transfer_from(&pool, &depositor, &pool, &amount_b);
        }
        MockTokenClient::new(&env, &share_token).mint(&depositor, &shares);

        let reserve_a: i128 = env
            .storage()
            .instance()
            .get(&PhoenixKey::ReserveA)
            .unwrap_or(0);
        let reserve_b: i128 = env
            .storage()
            .instance()
            .get(&PhoenixKey::ReserveB)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&PhoenixKey::ReserveA, &(reserve_a + amount_a));
        env.storage()
            .instance()
            .set(&PhoenixKey::ReserveB, &(reserve_b + amount_b));
    }
    pub fn withdraw_liquidity(
        env: Env,
        recipient: Address,
        share_amount: i128,
        min_a: i128,
        min_b: i128,
        _deadline: Option<u64>,
    ) -> (i128, i128) {
        recipient.require_auth();
        assert!(share_amount > 0, "withdraw_liquidity: zero share_amount");
        let reserve_a: i128 = env
            .storage()
            .instance()
            .get(&PhoenixKey::ReserveA)
            .unwrap_or(0);
        let reserve_b: i128 = env
            .storage()
            .instance()
            .get(&PhoenixKey::ReserveB)
            .unwrap_or(0);
        let total_shares = {
            let share_token: Address = env
                .storage()
                .instance()
                .get(&PhoenixKey::ShareToken)
                .unwrap();
            MockTokenClient::new(&env, &share_token).total_supply()
        };
        let (amount_a, amount_b) = if total_shares > 0 {
            (
                reserve_a.saturating_mul(share_amount) / total_shares,
                reserve_b.saturating_mul(share_amount) / total_shares,
            )
        } else {
            let half = share_amount / 2;
            (half, half)
        };
        assert!(amount_a >= min_a, "withdraw_liquidity: amount_a below min");
        assert!(amount_b >= min_b, "withdraw_liquidity: amount_b below min");
        let token_a: Address = env
            .storage()
            .instance()
            .get(&PhoenixKey::UnderlyingA)
            .unwrap();
        let token_b: Address = env
            .storage()
            .instance()
            .get(&PhoenixKey::UnderlyingB)
            .unwrap();
        MockTokenClient::new(&env, &token_a).mint(&recipient, &amount_a);
        MockTokenClient::new(&env, &token_b).mint(&recipient, &amount_b);

        let new_reserve_a = reserve_a
            .checked_sub(amount_a)
            .expect("withdraw_liquidity: reserve_a underflow");
        let new_reserve_b = reserve_b
            .checked_sub(amount_b)
            .expect("withdraw_liquidity: reserve_b underflow");
        env.storage()
            .instance()
            .set(&PhoenixKey::ReserveA, &new_reserve_a);
        env.storage()
            .instance()
            .set(&PhoenixKey::ReserveB, &new_reserve_b);
        (amount_a, amount_b)
    }

    pub fn get_reserves(env: Env) -> (i128, i128) {
        let reserve_a: i128 = env
            .storage()
            .instance()
            .get(&PhoenixKey::ReserveA)
            .unwrap_or(0);
        let reserve_b: i128 = env
            .storage()
            .instance()
            .get(&PhoenixKey::ReserveB)
            .unwrap_or(0);
        (reserve_a, reserve_b)
    }
}

// ---------------------------------------------------------------------------
// MockOracle
// ---------------------------------------------------------------------------

#[contracttype]
pub enum OracleKey {
    Admin,
    Price(Address),
}

#[contract]
pub struct MockOracle;

#[contractimpl]
impl MockOracle {
    /// One-time initialization — stores the admin address.
    /// Panics if called more than once.
    pub fn oracle_init(env: Env, admin: Address) {
        assert!(
            !env.storage().instance().has(&OracleKey::Admin),
            "oracle_init: already initialized"
        );
        env.storage().instance().set(&OracleKey::Admin, &admin);
    }

    /// Update the price for `asset`. Admin only.
    pub fn set_price(env: Env, asset: Address, price: i128) {
        let admin: Address = env.storage().instance().get(&OracleKey::Admin).unwrap();
        admin.require_auth();
        env.storage()
            .instance()
            .set(&OracleKey::Price(asset), &price);
    }

    pub fn get_price(env: Env, asset: Address) -> i128 {
        env.storage()
            .instance()
            .get(&OracleKey::Price(asset))
            .unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Helper: token balance shorthand
// ---------------------------------------------------------------------------

pub fn token_balance(env: &Env, token: &Address, account: &Address) -> i128 {
    MockTokenClient::new(env, token).balance(account)
}
