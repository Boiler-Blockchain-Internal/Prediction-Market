# BoilerMarket

BoilerMarket is a binary prediction market with a reputation layer, built by a Purdue student team. This repository currently contains the Phase 1 matching engine: a deterministic Rust library that accepts commands and returns an exact, reproducible event stream.

Users trade YES and NO shares on questions with binary outcomes. Prices range from 1 to 99 and represent the market's estimated probability. The beta uses play money only.

## Phase 1 features

- One engine managing multiple markets
- A single YES-priced order book per market, with NO orders translated at entry
- GTC and IOC limit orders
- Price-time priority with execution at the resting order's price
- Full and partial fills across multiple price levels
- Order cancellation and self-trade prevention
- Market locking with deterministic cancellation order
- Per-market, gap-free event numbering
- Safe slab-backed linked lists with no `unsafe` code
- Read-only best-price and book snapshot helpers

The core performs no file, network, database, balance, settlement, or clock operations. Those responsibilities belong to later service and backend phases.

## Project structure

```text
engine/
  src/       Matching engine implementation
  tests/     Golden scenarios, determinism, and property tests
```

## Getting started

Install the stable Rust toolchain, then run:

```bash
cd engine
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

## Testing

The test suite covers linked-list operations, price-time priority, YES/NO translation, partial and multi-level fills, IOC expiration, cancellation, self-trade prevention, market locking, rejection paths, event ordering, replay determinism, and quantity conservation.

Property tests also run randomized command streams through the engine and check its invariants after every command.

## Current scope

Phase 1 is the pure matching core. Persistence, networking, balances, positions, settlement, snapshots, Redis, Postgres, and on-chain reputation are intentionally outside this crate's current scope.
