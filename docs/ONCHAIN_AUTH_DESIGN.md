# BoilerMarket Onchain Authentication Design

**Owner:** Adi
**Status:** Initial SIWE flow implemented with Supabase persistence; production hardening remains
**Scope:** WalletConnect integration and wallet-based sign-in for the BoilerMarket Next.js application

## 1. Purpose

This document describes how BoilerMarket will let a user connect an EVM wallet and authenticate with it. It covers the initial implementation design; it does not implement the integration.

The project plan describes BoilerMarket as a social forecasting platform. Users create profiles and predictions, predictions have deadlines and resolution rules, and user reputation is based on forecasting performance. The first version is explicitly not intended to involve real-money wagering. Onchain authentication should therefore establish a user identity for the application without requesting an onchain transaction.

## 2. Goals and non-goals

### Goals

- Let users connect a compatible EVM wallet from desktop or mobile through WalletConnect.
- Support a clear, separate sign-in step using a wallet signature.
- Verify signatures on the server and issue an application session tied to the verified address.
- Use the wallet address as a stable identity key for a BoilerMarket profile and prediction history.
- Keep the integration compatible with Next.js App Router and server rendering.
- Make connection, signing, rejection, disconnection, and session-expiry states understandable to users.

### Non-goals for this phase

- Sending transactions, managing funds, or enabling wagering.
- Deploying or integrating prediction-market contracts.
- Supporting non-EVM wallet ecosystems.
- Using wallet connection by itself as proof of authentication.
- Adding social login, email login, ENS profile resolution, or delegated permissions.

## 3. Proposed decisions

1. **Use Reown AppKit for WalletConnect UI and wallet discovery, with its Wagmi adapter.** The package formerly known as WalletConnect Web3Modal is now documented as Reown AppKit. AppKit's Next.js setup supports Wagmi and Ethers, and its Wagmi integration includes WalletConnect and injected wallet connectors. Use the Wagmi adapter because the proposed app is EVM-based and Wagmi/Viem provide a direct path for account state and signature verification.
2. **Use Sign-In with Ethereum (SIWE, EIP-4361) for application authentication.** Connecting gives the frontend access to the wallet account; it does not create an authenticated BoilerMarket session. The user must approve a SIWE message, and the server must verify the message and signature before creating the session.
3. **Keep the exact chain as configuration, not a hard-coded design assumption.** The project plan does not identify a production chain. Configure an allowlist and select the initial supported chain before implementation. The server must reject SIWE messages for chain IDs outside that allowlist.
4. **Use an opaque, server-managed session in an HttpOnly cookie.** Do not store session tokens in `localStorage`. The session identifies the authenticated address and is checked by server-side protected routes/actions.
5. **Keep authentication separate from wallet transport state.** A connected wallet is not an authenticated user. UI and backend authorization must use the verified server session.

## 4. User experience and request flow

```mermaid
sequenceDiagram
    actor User
    participant UI as Next.js client
    participant WC as Reown AppKit / WalletConnect
    participant API as Next.js auth routes
    participant DB as Session / nonce store
    participant Wallet

    User->>UI: Select Connect wallet
    UI->>WC: Open wallet picker
    WC->>Wallet: Establish wallet connection
    Wallet-->>UI: Connected address and chain
    Note over UI: Connected is not yet signed in
    User->>UI: Select Sign in
    UI->>API: Request one-time nonce
    API->>DB: Store nonce with expiry and pending state
    API-->>UI: Return nonce and SIWE fields
    UI->>Wallet: Request SIWE message signature
    Wallet-->>User: Show domain and sign-in message
    User->>Wallet: Approve or reject signature
    Wallet-->>UI: Signature or rejection
    UI->>API: Submit exact message and signature
    API->>DB: Atomically consume nonce
    API->>API: Validate SIWE fields and verify signature
    API->>DB: Find or create profile; create session
    API-->>UI: Set HttpOnly session cookie
    UI-->>User: Show authenticated profile state
```

### Happy path

1. The user chooses **Connect wallet**. AppKit opens the wallet picker and establishes the WalletConnect or injected-wallet connection.
2. BoilerMarket displays the connected address and current chain. It clearly indicates that the user is connected but not yet signed in.
3. The user chooses **Sign in**. The client requests a one-time nonce from the server.
4. The server creates a cryptographically random nonce, stores it with a short expiry and unused status, and returns the SIWE fields needed by the client.
5. The client builds a SIWE message from server-approved values and asks the wallet to sign it as a message. The UI explains that this signature signs in to BoilerMarket and does not submit a transaction.
6. The client submits the exact message and signature to the server.
7. The server parses and validates the SIWE message, atomically consumes the database nonce, and verifies the externally owned account signature for the supplied address and chain.
8. On success, the server creates or finds the wallet profile, stores a hash of a random session ID in Supabase, and sets the raw ID in an HttpOnly cookie. The client refreshes its authenticated state from the server. Smart-contract wallet verification is follow-up work.

### Cancellation and errors

- Wallet selection dismissed: return to the disconnected state without an error banner.
- Connection rejected or unavailable: show a retry action and keep the user signed out.
- Signature rejected: keep the wallet connected, keep the user signed out, and allow retry.
- Wrong or unsupported chain: explain the supported chain and offer AppKit's network switch where available; never silently change the chain.
- Invalid, expired, or reused nonce; malformed message; wrong domain/URI/chain; invalid signature: reject authentication, do not create a session, and require a fresh nonce for retry.
- Network or server failure: show a retryable error; do not display an authenticated state based only on client state.
- Session expired: clear the authenticated UI and ask the user to sign in again.

## 5. Architecture

### Frontend

- Initialize AppKit once in a client-side provider mounted by the root layout.
- Keep the AppKit/Wagmi configuration in a dedicated module so it is not recreated during React renders.
- Use the AppKit connect control or a thin BoilerMarket wrapper so the connection entry point matches the product UI.
- Derive connection state from Wagmi/AppKit. Derive signed-in state from a server session endpoint or server-rendered session data.
- Use a dedicated sign-in action to request the nonce, request the wallet signature, and submit it to the verification route.
- Keep all wallet-dependent hooks in client components. Do not import browser-only wallet modules into server components.

### Backend

The initial implementation adds Next.js Route Handlers for challenge issuance, verification, session lookup, and logout. It uses the existing Supabase project and a server-only secret key. Apply `reputation/migrations/002_wallet_auth.sql` before using these routes. Challenges are consumed with one conditional database update; session IDs are random and stored only as SHA-256 hashes. The routes are:

| Operation | Proposed route | Responsibility |
|---|---|---|
| Issue challenge | `POST /api/auth/siwe/nonce` | Create a random, short-lived, single-use nonce and return the server-approved SIWE domain, URI, chain ID, and statement. |
| Verify sign-in | `POST /api/auth/siwe/verify` | Validate the submitted message against the challenge, consume the nonce once, verify an EOA signature, find/create the wallet user, and issue a session cookie. |
| Read session | `GET /api/auth/session` | Return the authenticated address for the current valid session. |
| Sign out | `POST /api/auth/logout` | Revoke the session in Supabase and expire the cookie. |

Protected page and API requests read the server session and use its address as the authenticated principal. The client should call a `GET /api/auth/session` endpoint or receive equivalent server-rendered data to rehydrate the signed-in UI after refresh.

### Proposed module boundaries

Adapt these names to the repository's existing structure when implementation begins:

- `src/lib/wallet/appkit.ts` — AppKit, Wagmi adapter, chain allowlist, metadata.
- `src/components/providers.tsx` — client provider for Wagmi, TanStack Query, and AppKit.
- `src/components/auth/WalletSignInButton.tsx` — connection and sign-in UI states.
- `src/lib/auth/siwe.ts` — SIWE construction/parsing and validation helpers.
- `src/lib/auth/server.ts` — SIWE validation, Supabase REST storage, user mapping, and cookie settings.
- `src/app/api/auth/siwe/nonce/route.ts` — nonce issuance.
- `src/app/api/auth/siwe/verify/route.ts` — challenge and signature verification.
- `src/app/api/auth/logout/route.ts` — logout.

## 6. Data model

Use existing project schema conventions and auth/session facilities if they exist. The reputation prototype already uses Supabase; this auth implementation uses the same Supabase project for wallet identities and auth records.

### Wallet identity / profile

- `id`: application user/profile ID.
- `wallet_address`: normalized EVM address, unique and indexed. Store a stable lowercase representation for lookup; checksum-format for display.
- `created_at`, `updated_at`.
- Existing profile fields from the product schema, such as display name and avatar, remain separate from the wallet identity.

### SIWE nonce challenge (`wallet_auth_challenges`)

- SHA-256 `nonce_hash` as the non-reusable challenge identifier.
- `address` when known at issuance, if the client supplies one.
- `created_at`, `expires_at`, `consumed_at`.
- Optional request/session binding value if the chosen implementation requires it to prevent cross-browser challenge use.

Challenges must be unpredictable, short-lived, and consumed atomically. They must not be reusable after any successful verification attempt.

### Application session (`wallet_auth_sessions`)

- Opaque random session identifier; store only its SHA-256 hash in Supabase.
- `user_id` and authenticated wallet address.
- `created_at`, `expires_at`, `revoked_at`.
- Optional metadata required by the session store; avoid storing raw signatures after verification.

## 7. SIWE validation requirements

The server must treat the submitted message as untrusted input and validate every security-relevant field before setting a session:

- Message domain exactly matches a configured trusted application host; do not trust a caller-provided `Host` or `Origin` value as the source of truth.
- URI matches the expected application sign-in URI/origin.
- SIWE version is `1`.
- Chain ID is in the configured chain allowlist.
- Address in the SIWE message equals the address whose signature is being checked and, when the nonce request was bound to an address, equals that address too.
- Nonce equals the outstanding, unused challenge and is within its expiry window.
- Issued-at and optional expiration/not-before timestamps are valid and within allowed clock skew.
- Signature verifies for the address. The initial implementation accepts 65-byte externally owned account signatures only; smart-contract wallet verification (ERC-1271) needs a chain-aware public client and RPC configuration.
- Challenge consumption and session creation are protected against concurrent replay (transaction or atomic conditional update).

The client should not be allowed to choose an arbitrary domain, URI, chain ID, statement, or nonce. The server constructs or validates those values against its own configuration.

## 8. Security and privacy

- Request a message signature only for login. Do not request transaction signatures or token/contract approvals in this phase.
- Use the standard human-readable SIWE message with an explicit BoilerMarket sign-in statement. The message must name the application domain and clearly describe the action.
- Use HTTPS outside local development.
- Set session cookies as `HttpOnly`, `Secure` in production, `SameSite=Lax`, and scoped to `/`; use a server-side expiry and rotate session IDs at login.
- Protect state-changing auth routes against CSRF using SameSite cookies plus origin checks and/or the project's established CSRF token mechanism.
- Add rate limiting for challenge issuance and verification by address and request source before production. Avoid logging raw signatures, nonce values, cookies, or full SIWE payloads.
- Do not send private keys, seed phrases, or wallet credentials to BoilerMarket. The wallet retains key custody.
- Treat WalletConnect project ID as public client configuration, but configure allowed domains/origins in the Reown dashboard and keep server-side RPC credentials (if any) in server-only environment variables.
- A wallet address is public data but can link a user's activity across services. Only persist it as needed for profile identity and product behavior; do not add third-party address enrichment without a separate product decision.
- On wallet account change or WalletConnect disconnect, clear stale connection-derived UI and require a new sign-in for the active address. Explicit **Sign out** revokes the server session. Protected server routes must never infer authorization solely from an active WalletConnect session.

## 9. Configuration and dependencies

Current environment/configuration values:

- `NEXT_PUBLIC_REOWN_PROJECT_ID` — public Reown project ID used by AppKit.
- `NEXT_PUBLIC_APP_URL` — trusted canonical origin used for wallet metadata and SIWE domain/URI validation.
- `SUPABASE_URL` — Supabase project URL used by the server routes.
- `SUPABASE_SECRET_KEY` — server-only key for accessing the locked auth tables and `public.users`; never expose it to client code.
- Sepolia (`11155111`) is currently configured in code as a provisional chain. Move this to an explicit allowlist configuration when the team confirms the target network.
- Server RPC endpoint(s), if needed for future smart-contract wallet signature validation — server-only values. Session IDs are random and hashed in Supabase, so no `SESSION_SECRET` is needed.

Use the package manager and package versions already selected by the repository. Reown's current Next.js guide documents `@reown/appkit`, `@reown/appkit-adapter-wagmi`, Wagmi 2, Viem, and TanStack Query; it also describes cookie storage and SSR configuration to avoid hydration mismatches. Confirm the current stable package compatibility at implementation time instead of copying beta versions from an example.

## 10. Rollout plan

1. **Repository and product decisions:** Supabase is the selected initial store, and Sepolia remains provisional pending product confirmation. Decide whether ERC-1271 contract-wallet support is required.
2. **Wallet connection:** implemented with AppKit/Wagmi for provisional Sepolia connection.
3. **Challenge and signature flow:** implemented with server-issued SIWE challenges, wallet signing, server-side EOA verification, single-use nonce consumption in Supabase, and origin/domain/chain checks.
4. **Session and profile mapping:** implemented with a hashed opaque session ID in Supabase, an HttpOnly cookie, and user rows keyed by normalized wallet address.
5. **Product integration:** expose the signed-in wallet identity in the profile and ensure prediction creation can rely on the server-authenticated user identity.
6. **Verification before merge:** verify desktop injected wallet and WalletConnect mobile/deep-link flows, sign-in and rejection cases, unsupported chain, invalid/expired/replayed challenge, page refresh, session expiry, wallet account change, logout, and unauthenticated access to protected operations.

## 11. Acceptance criteria

- A user can connect an EVM wallet using WalletConnect on desktop/mobile or an available injected wallet.
- A connected wallet remains unsigned-in until a SIWE signature is verified by the server.
- The sign-in prompt clearly says it is for BoilerMarket authentication and does not trigger a transaction.
- A valid 65-byte externally owned account signature for the configured Sepolia chain and current nonce creates or finds the `public.users` profile and a durable session in Supabase. Smart-contract wallet verification is not implemented yet.
- Invalid, expired, wrong-domain, wrong-chain, wrong-address, or replayed messages do not create a session.
- The session cookie persists across page refresh, is not readable by client JavaScript, and can be revoked by logout. Session records persist in Supabase.
- Authenticated profile and prediction operations use the server session, not a client-supplied address.
- Disconnect/account-change and wallet rejection states return to a safe, understandable UI state.

## 12. Open decisions

- Which EVM chain should be supported for the initial project demo?
- Should the app continue using the custom wallet session table or migrate to Supabase Auth after the initial prototype?
- Is contract-wallet (ERC-1271) signature verification required for the first milestone?
- Should a wallet identity be the only account identifier initially, or should future email/social linking be anticipated in the profile model?

## 13. References

- [BoilerMarket Planning Doc](https://docs.google.com/document/d/1NEJ8O4VzKV-6Ay7-3cZjwKRC1Dmn_EEZozz6eY0IOks/edit?tab=t.0) — project goals, product scope, and technical options.
- [Reown AppKit: Next.js installation](https://docs.reown.com/appkit/next/core/installation) — current App Router integration, Wagmi adapter setup, project ID, metadata, and SSR notes.
- [EIP-4361: Sign-In with Ethereum](https://eips.ethereum.org/EIPS/eip-4361) — SIWE message format, nonce requirements, signature verification, sessions, and origin validation.
- [Reown AppKit: Next.js hooks](https://docs.reown.com/appkit/next/core/hooks) — AppKit hooks and client integration reference.
