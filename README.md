# BoilerMarket

BoilerMarket is a binary prediction market with a reputation layer, built by a Purdue student team. This repository contains the Phase 1 Rust matching engine and a Python reputation prototype. The `src/` Next.js app in this branch adds the initial wallet sign-in frontend.

Users trade YES and NO shares on questions with binary outcomes. Prices range from 1 to 99 and represent the market's estimated probability. The beta uses play money only.

## Phase 1 matching engine

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
engine/       Deterministic Rust matching engine and tests
reputation/   Python reputation prototype, Supabase access, and migrations
src/          Next.js wallet connection and authentication prototype
```

## Getting started

For the Rust engine, install the stable Rust toolchain and run:

```bash
cd engine
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

For the Next.js auth prototype:

1. Install Node.js 20.9 or newer and pnpm.
2. Run `pnpm install` from the repository root.
3. Copy `.env.example` to `.env.local` and set `NEXT_PUBLIC_REOWN_PROJECT_ID`, `NEXT_PUBLIC_APP_URL`, `SUPABASE_URL`, server-only `SUPABASE_SECRET_KEY`, and server-only `SEPOLIA_RPC_URL`.
4. Apply `reputation/migrations/002_wallet_auth.sql` to the same Supabase project used by the reputation prototype.
5. Run `pnpm dev` and open `http://localhost:3000`.

The auth prototype uses Reown AppKit/Wagmi and provisionally supports Sepolia. It issues and verifies SIWE messages for externally owned accounts and ERC-1271 smart-contract wallets, maps wallets to `public.users`, and stores hashed session IDs in Supabase behind an HttpOnly cookie. Smart-contract wallet verification uses a server-side Sepolia RPC endpoint and does not send a transaction. Confirm the product's target chain, add rate limiting, and protect product actions before production use.

`NEXT_PUBLIC_APP_URL` must match the app's exact origin in each environment. The Reown project ID is public client configuration. Keep `SUPABASE_SECRET_KEY` and `SEPOLIA_RPC_URL` server-only and never use a `NEXT_PUBLIC_` prefix for them.

## Reputation prototype

See [`reputation/README.md`](reputation/README.md) for setup and usage. The current reputation pipeline reads simulated trade data from Supabase and writes reputation snapshots.

## Current scope

The matching engine is a pure deterministic core. Networking, balances, positions, settlement, and integration between the web app, matching engine, and reputation service remain later phases.
