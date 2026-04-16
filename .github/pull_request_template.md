## Summary
- What changed and why?

## Risk Level
- [ ] Low
- [ ] Medium
- [ ] High

## Security Checklist
- [ ] No secrets/private keys added
- [ ] Access control impact reviewed
- [ ] Economic/accounting impact reviewed (if applicable)
- [ ] Upgrade/deployment impact reviewed (if applicable)

## Testing Evidence
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-features -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] Added/updated tests for new behavior

## Deployment Notes
- Any migration, config, or env updates needed?
