//! Integration tests: Vault ↔ Trade guards (execute_op API)
//!
//! These tests are pending the trade guard contracts being updated to
//! implement the `execute_op` / `get_total_value` / `withdraw_fraction`
//! guard interface.  The old execute_trade/set_trade_guard API has been removed.

#![cfg(test)]
