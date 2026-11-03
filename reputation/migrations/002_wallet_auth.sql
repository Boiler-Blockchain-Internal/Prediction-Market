-- Durable, server-only storage for SIWE challenges and opaque app sessions.
-- These tables are accessed only by Next.js server routes using the Supabase secret key.
CREATE TABLE public.wallet_auth_challenges (
    nonce_hash TEXT PRIMARY KEY CHECK (length(nonce_hash) = 64),
    wallet_address TEXT NOT NULL,
    chain_id BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ
);

CREATE INDEX idx_wallet_auth_challenges_expiry
ON public.wallet_auth_challenges (expires_at);

CREATE TABLE public.wallet_auth_sessions (
    id_hash TEXT PRIMARY KEY CHECK (length(id_hash) = 64),
    user_id BIGINT NOT NULL REFERENCES public.users(id) ON DELETE CASCADE,
    wallet_address TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ
);

CREATE INDEX idx_wallet_auth_sessions_user_expiry
ON public.wallet_auth_sessions (user_id, expires_at);

ALTER TABLE public.wallet_auth_challenges ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.wallet_auth_sessions ENABLE ROW LEVEL SECURITY;

REVOKE ALL ON TABLE public.wallet_auth_challenges, public.wallet_auth_sessions
FROM PUBLIC, anon, authenticated;
