# BoilerMarket matching engine

This crate is the Phase 1 pure matching core described by the repository's `CLAUDE.md`.
It performs no I/O, clock reads, floating-point work, or balance accounting. Callers submit
sequenced commands to `Engine::apply` and persist or publish the returned events themselves.

Unknown-market commands return one rejection with `event_no = 0`; because the market does not
exist, no per-market event counter is advanced. Read-only helpers return `None` for an unknown
market.

## Sequence numbers

`seq` must be strictly greater than the `seq` of every command applied before it, across all
markets. An order's id is its `seq`, so this is what keeps order ids unique. A command that
breaks the rule (a duplicate, a replay, or one arriving out of order) is rejected with
`RejectReason::StaleSeq`: one `OrderRejected` event with `event_no = 0`, and no state change.
For a `PlaceOrder` the rejection's `order` is `None`, because the stale `seq` may be the id of
a real live order. `event_no = 0` therefore means "no event stream was advanced", either an
unknown market or a stale `seq`.

Callers that replay a journal must feed commands in their original order with their original
`seq` values, starting from a fresh `Engine`.

Run the validation suite from this directory:

```text
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```
