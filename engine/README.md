# BoilerMarket matching engine

This crate is the Phase 1 pure matching core described by the repository's `CLAUDE.md`.
It performs no I/O, clock reads, floating-point work, or balance accounting. Callers submit
sequenced commands to `Engine::apply` and persist or publish the returned events themselves.

Unknown-market commands return one rejection with `event_no = 0`; because the market does not
exist, no per-market event counter is advanced. Read-only helpers return `None` for an unknown
market.

Run the validation suite from this directory:

```text
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```
