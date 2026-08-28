# Contributing

Clared is an experimental security reference implementation. Useful contributions make a boundary more precise, falsifiable, or reproducible.

Before opening a pull request:

```bash
cd clared-core
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test

cd ../clared-python
pip install -e ".[dev]"
pytest -v
```

Protocol changes should include wire examples, compatibility impact, and adversarial tests. Adapter changes should explain the provider's real staging and rollback semantics. Do not label a synthetic identifier, in-memory mutation, or compensating action as atomic or live.

Security vulnerabilities should follow [SECURITY.md](SECURITY.md), not a public issue.
