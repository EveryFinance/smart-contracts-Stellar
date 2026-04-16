use soroban_sdk::{Address, Env, Symbol};

// ---------------------------------------------------------------------------
// SEP-41 compliant event publishers
//
// The three-tuple topic (event_name, arg1, arg2) follows the SEP-41 standard
// so that indexers and wallets can parse token activity uniformly.
// ---------------------------------------------------------------------------

/// Publish a `transfer` event following the SEP-41 token standard.
///
/// # Arguments
/// * `env`    – The current contract environment.
/// * `from`   – The sender address (tokens deducted here).
/// * `to`     – The recipient address (tokens credited here).
/// * `amount` – The number of token units transferred (always > 0).
pub fn transfer_event(env: &Env, from: Address, to: Address, amount: i128) {
    let topics = (Symbol::new(env, "transfer"), from, to);
    env.events().publish(topics, amount);
}

/// Publish an `approve` event following the SEP-41 token standard.
///
/// # Arguments
/// * `env`               – The current contract environment.
/// * `from`              – The token holder granting the allowance.
/// * `spender`           – The address being approved to spend.
/// * `amount`            – The new allowance amount (0 revokes).
/// * `expiration_ledger` – The ledger at which the allowance expires.
///   Stored for informational purposes only; enforcement is the caller's
///   responsibility in this implementation.
pub fn approve_event(
    env: &Env,
    from: Address,
    spender: Address,
    amount: i128,
    expiration_ledger: u32,
) {
    let topics = (Symbol::new(env, "approve"), from, spender);
    env.events().publish(topics, (amount, expiration_ledger));
}

/// Publish a `mint` event (admin-only operation).
///
/// # Arguments
/// * `env`    – The current contract environment.
/// * `admin`  – The admin address that authorized the mint.
/// * `to`     – The recipient address.
/// * `amount` – The number of newly minted token units (always > 0).
pub fn mint_event(env: &Env, admin: Address, to: Address, amount: i128) {
    let topics = (Symbol::new(env, "mint"), admin, to);
    env.events().publish(topics, amount);
}

/// Publish a `burn` event.
///
/// # Arguments
/// * `env`    – The current contract environment.
/// * `from`   – The address whose tokens were destroyed.
/// * `amount` – The number of token units burned (always > 0).
pub fn burn_event(env: &Env, from: Address, amount: i128) {
    let topics = (Symbol::new(env, "burn"), from);
    env.events().publish(topics, amount);
}
