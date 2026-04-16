# Contributing

## Local Quality Gates

Run before opening a PR:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace
```

## Branch & PR Flow (Solo-friendly)

1. Create a feature branch from `main`.
2. Make focused changes and keep commits atomic.
3. Open a PR and ensure CI passes.
4. Squash-merge to `main`.

## Security Hygiene

- Never commit private keys, mnemonics, or real `.env` secrets.
- Use `.env.example` and `deployments/*.example.env` for templates only.
