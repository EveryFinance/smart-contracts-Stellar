// Integration test entry point.
// All tests live in submodules; this file only re-exports them for `cargo test`.
#[cfg(test)]
mod common;
#[cfg(test)]
mod test_factory_registry;
#[cfg(test)]
mod test_vault_blend;
#[cfg(test)]
mod test_vault_lifecycle;
#[cfg(test)]
mod test_vault_mixed_positions;
#[cfg(test)]
mod test_vault_multiasset;
#[cfg(test)]
mod test_vault_phoenix_lp;
#[cfg(test)]
mod test_vault_soroswap_lp;
#[cfg(test)]
mod test_vault_trade;
